//! One retimed frame: optional layer separation by mattes, per-layer flow,
//! warping and compositing.

use crate::filters::{dilate, fill_image, prep, ImagePrep};
use crate::flow::{compute_flow, FlowParams, Guide, Quality};
use crate::image::{Flow, Gray, Image};
use crate::interp::{blend_only, interpolate, motion_blur, FrameInterp, Warping};
use crate::timemap::PairOverride;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub quality: Quality,
    pub prep: ImagePrep,
    pub track_rgba: bool,
    pub interp: FrameInterp,
    pub warping: Warping,
    pub draft: bool,
    /// Motion Blur Compensation (0 = off, 0.5 = 180 degree shutter).
    pub mb_amount: f32,
    /// Source frames advanced per output frame (for motion blur on speed-up).
    pub frames_per_output: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: Quality::Best,
            prep: ImagePrep::None,
            track_rgba: false,
            interp: FrameInterp::Blend,
            warping: Warping::Inverse,
            draft: false,
            mb_amount: 0.0,
            frames_per_output: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Output,
    Source,
    Fg(usize),
    Background,
}

/// A foreground layer: its matte at both source frames and its own tracking settings.
pub struct FgLayer {
    pub matte_a: Gray,
    pub matte_b: Gray,
    pub sensitivity: f32,
    pub inv_shrink: f32,
    pub guides: Vec<Guide>,
}

pub struct Frames<'a> {
    pub a: &'a Image,
    pub b: &'a Image,
    /// Alt Motion Source at the same two frames (tracking only).
    pub motion_a: Option<&'a Image>,
    pub motion_b: Option<&'a Image>,
    /// Stable ids of the two source frames, for the flow cache.
    pub id_a: i64,
    pub id_b: i64,
}

pub struct FlowPair {
    pub fab: Flow,
    pub fba: Flow,
}

/// Small LRU of computed flow pairs ("Plenty Memory?"). Concurrent renders
/// asking for the same pair (multi-frame rendering) wait for one computation
/// instead of each doing it.
pub struct FlowCache {
    cap: usize,
    items: Mutex<VecDeque<(u64, Arc<OnceLock<Arc<FlowPair>>>)>>,
}

impl FlowCache {
    pub fn new(cap: usize) -> Self {
        Self { cap, items: Mutex::new(VecDeque::new()) }
    }

    fn get_or_compute(&self, key: u64, compute: impl FnOnce() -> FlowPair) -> Arc<FlowPair> {
        let cell = match self.items.lock() {
            Ok(mut q) => {
                let cell = match q.iter().position(|(k, _)| *k == key) {
                    Some(pos) => q.remove(pos).unwrap().1,
                    None => Arc::new(OnceLock::new()),
                };
                q.push_front((key, cell.clone()));
                q.truncate(self.cap);
                cell
            }
            Err(_) => Arc::new(OnceLock::new()),
        };
        if let Some(v) = cell.get() {
            drop(crate::prof::span("cache.hit"));
            return v.clone();
        }
        cell.get_or_init(|| {
            drop(crate::prof::span("cache.miss"));
            Arc::new(compute())
        })
        .clone()
    }

    pub fn clear(&self) {
        if let Ok(mut q) = self.items.lock() {
            q.clear();
        }
    }
}
/// Forward-backward check: a vector is trusted when the opposite flow brings
/// the pixel back to where it started. Untrusted vectors (mismatches, and
/// occlusions, which have no true match) are replaced by a smooth fill from
/// trusted neighbours, so a bad patch cannot tear out a block of the image.
fn clean_flow(fwd: &Flow, bwd: &Flow) -> Flow {
    let _t = crate::prof::span("flow.clean");
    use rayon::prelude::*;
    let (w, h) = (fwd.w, fwd.h);
    let trust: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let v = fwd.data[i];
            let (tx, ty) = (x + v[0], y + v[1]);
            if tx < 0.0 || ty < 0.0 || tx > (w - 1) as f32 || ty > (h - 1) as f32 {
                // Leaves the frame: nothing to check against, keep it.
                return 1.0;
            }
            let b = bwd.sample(tx, ty);
            let err = (v[0] + b[0]).hypot(v[1] + b[1]);
            let tol = 1.0 + 0.05 * v[0].hypot(v[1]);
            if err <= tol { 1.0 } else if err >= 3.0 * tol { 0.0 } else { 1.0 - (err - tol) / (2.0 * tol) }
        })
        .collect();
    if trust.iter().all(|&k| k >= 1.0) {
        return fwd.clone();
    }
    // Keep a little of the original so isolated cases do not snap to zero.
    let weights: Vec<f32> = trust.iter().map(|k| k.max(0.02)).collect();
    crate::filters::fill_flow(fwd, &Gray { w, h, data: weights })
}

fn mix(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x100000001b3).rotate_left(17)
}

#[allow(clippy::too_many_arguments)]
fn flows(
    track_a: &Gray,
    track_b: &Gray,
    mask_a: Option<&Gray>,
    mask_b: Option<&Gray>,
    guides: &[Guide],
    params: &FlowParams,
    cache: Option<(&FlowCache, u64)>,
) -> Arc<FlowPair> {
    let compute = || {
        let back: Vec<Guide> = guides.iter().map(|g| Guide { from: g.to, to: g.from }).collect();
        let (fab, fba) = rayon::join(
            || compute_flow(track_a, track_b, mask_a, guides, params),
            || compute_flow(track_b, track_a, mask_b, &back, params),
        );
        let (fab2, fba2) = rayon::join(|| clean_flow(&fab, &fba), || clean_flow(&fba, &fab));
        FlowPair { fab: fab2, fba: fba2 }
    };
    match cache {
        Some((c, key)) => c.get_or_compute(key, compute),
        None => Arc::new(compute()),
    }
}

/// Renders the frame at fraction `t` between `frames.a` and `frames.b`.
///
/// `cache_key` should identify the clip and every setting that affects the
/// flow (quality, prep, sensitivities, mattes); frame ids are mixed in here.
#[allow(clippy::too_many_arguments)]
pub fn retime(
    frames: &Frames,
    t: f32,
    over: PairOverride,
    fg: &[FgLayer],
    bg_sensitivity: f32,
    bg_guides: &[Guide],
    s: &Settings,
    display: Display,
    cache: Option<(&FlowCache, u64)>,
) -> Image {
    if display == Display::Source || frames.id_a == frames.id_b || t <= 0.0 {
        return frames.a.clone();
    }
    match over {
        PairOverride::Nearest => return blend_only(frames.a, frames.b, t, FrameInterp::Nearest),
        PairOverride::Blend => return blend_only(frames.a, frames.b, t, FrameInterp::Blend),
        PairOverride::None => {}
    }
    if s.quality == Quality::NoVectors {
        return blend_only(frames.a, frames.b, t, s.interp);
    }

    let track = |img: &Image| { let _t = crate::prof::span("track_prep"); prep(&img.luma(s.track_rgba), s.prep) };
    let ta = track(frames.motion_a.unwrap_or(frames.a));
    let tb = track(frames.motion_b.unwrap_or(frames.b));
    let pair_key = |layer: u64| cache.map(|(c, k)| (c, mix(mix(mix(k, frames.id_a as u64), frames.id_b as u64), layer)));
    let fp = |sens: f32| FlowParams { quality: s.quality, sensitivity: sens, draft: s.draft };

    let finish = |img: Image, f: &FlowPair| {
        if s.mb_amount > 0.0 {
            motion_blur(&img, &f.fab, s.mb_amount, s.frames_per_output)
        } else {
            img
        }
    };

    if fg.is_empty() {
        let f = flows(&ta, &tb, None, None, bg_guides, &fp(bg_sensitivity), pair_key(0));
        let img = interpolate(frames.a, frames.b, &f.fab, &f.fba, t, s.interp, s.warping);
        return finish(img, &f);
    }

    // Layer separation. Background = everything outside the (grown) foreground mattes.
    let union = |pick: fn(&FgLayer) -> &Gray, grow: bool| {
        let mut u = Gray::new(ta.w, ta.h, 0.0);
        for l in fg {
            let m = pick(l);
            let m = if grow { dilate(m, l.inv_shrink) } else { m.clone() };
            for (o, v) in u.data.iter_mut().zip(m.data) {
                *o = o.max(v.clamp(0.0, 1.0));
            }
        }
        u
    };
    let bg_a = union(|l| &l.matte_a, true).map(|v| 1.0 - v);
    let bg_b = union(|l| &l.matte_b, true).map(|v| 1.0 - v);

    let render_bg = || {
        let f = flows(&ta, &tb, Some(&bg_a), Some(&bg_b), bg_guides, &fp(bg_sensitivity), pair_key(0));
        // Fill the holes left by the foreground so the background can be warped
        // under it without dragging foreground colour along.
        let ca = fill_image(frames.a, &bg_a);
        let cb = fill_image(frames.b, &bg_b);
        let img = interpolate(&ca, &cb, &f.fab, &f.fba, t, s.interp, s.warping);
        finish(img, &f)
    };
    let render_fg = |i: usize| {
        let l = &fg[i];
        let f = flows(&ta, &tb, Some(&l.matte_a), Some(&l.matte_b), &l.guides, &fp(l.sensitivity), pair_key(1 + i as u64));
        let ca = frames.a.scale_by(&l.matte_a);
        let cb = frames.b.scale_by(&l.matte_b);
        let img = interpolate(&ca, &cb, &f.fab, &f.fba, t, s.interp, s.warping);
        finish(img, &f)
    };

    match display {
        Display::Background => render_bg(),
        Display::Fg(i) if i < fg.len() => render_fg(i),
        Display::Fg(_) => Image::new(ta.w, ta.h),
        _ => {
            let mut out = render_bg();
            for i in (0..fg.len()).rev() {
                out = render_fg(i).over(&out);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(w: usize, h: usize, x0: usize) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Textured background so the flow has something to lock on to.
                let bgv = 0.3 + 0.2 * (((x * 7 + y * 13) % 17) as f32 / 17.0);
                img.data[y * w + x] = [bgv, bgv, bgv, 1.0];
            }
        }
        for y in 20..40 {
            for x in x0..x0 + 20 {
                let v = 0.8 + 0.2 * (((x - x0) * 3 + y) % 5) as f32 / 5.0;
                img.data[y * w + x] = [v, v * 0.5, 0.1, 1.0];
            }
        }
        img
    }

    #[test]
    fn midpoint_of_moving_square() {
        let (w, h) = (96, 64);
        let a = square(w, h, 20);
        let b = square(w, h, 30);
        let fr = Frames { a: &a, b: &b, motion_a: None, motion_b: None, id_a: 0, id_b: 1 };
        let mut s = Settings::default();
        s.warping = Warping::InverseSmartBlend;
        let out = retime(&fr, 0.5, PairOverride::None, &[], 100.0, &[], &s, Display::Output, None);
        // The square should now start near x = 25: check the centre of mass of the red-ish pixels.
        let mut sx = 0.0;
        let mut n = 0.0;
        for y in 22..38 {
            for x in 0..w {
                let p = out.data[y * w + x];
                if p[0] > 0.7 && p[2] < 0.3 {
                    sx += x as f32;
                    n += 1.0;
                }
            }
        }
        let cx = sx / n;
        assert!((cx - 34.5).abs() < 1.5, "centre {cx}");
    }
}
