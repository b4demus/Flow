//! Time effects that pick, mix or average other frames of the same clip.
//! Behaviour: spec/effects/time.md. Every effect says which frames it needs
//! (`needs`) and then renders from them (`render`); the host fetches frames.

pub use crate::glow::AlphaMode;
use crate::image::Image;
use rayon::prelude::*;

/// Where the clip is: current frame number (may be fractional), length in frames.
#[derive(Clone, Copy, Debug)]
pub struct Clip {
    pub frame: f32,
    pub total: f32,
}

/// Frame access for `render`: frame number -> image in working form (`to_working`).
pub trait Frames {
    fn get(&self, n: i32) -> &Image;
}

/// Straight frame -> the form the effects work on: premultiplied (Normal),
/// as is (As Premult) or opaque (All Opaque).
pub fn to_working(mut img: Image, mode: AlphaMode) -> Image {
    match mode {
        AlphaMode::Normal => img.premultiply(),
        AlphaMode::AllOpaque => img.data.par_iter_mut().for_each(|p| p[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    img
}

/// Result in working form -> straight colour. Alpha is cut to 1 first (a
/// brightened average keeps its brighter colour).
pub fn from_working(mut img: Image, mode: AlphaMode) -> Image {
    match mode {
        AlphaMode::Normal => {
            img.data.par_iter_mut().for_each(|p| p[3] = p[3].min(1.0));
            img.unpremultiply()
        }
        AlphaMode::AllOpaque => img.data.par_iter_mut().for_each(|p| p[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    img
}

// ───────────────────────────── shared maths ─────────────────────────────

/// Quantise down to a multiple of `step` (negative values: one more step down
/// at exact multiples, as the original does).
#[inline]
pub fn round_step(x: f32, step: f32) -> f32 {
    if step == 0.0 {
        x
    } else if x >= 0.0 {
        (x / step).floor() * step
    } else {
        (x / step).ceil() * step - step.abs()
    }
}

/// Hash of x in 0..1 (12 bits), the original's `random`.
#[inline]
pub fn random(x: f32) -> f32 {
    let h = crate::shake::hash4((x * 4096.0) as i32 as u32);
    (h & 0xfff) as f32 * (1.0 / 4096.0)
}

/// Frames a `get_frame(n, interp)` reads: n is clamped to the clip (0 ..
/// total − 1); without interpolation the frame below n.
pub fn frames_for(n: f32, interp: bool, total: f32) -> Vec<i32> {
    let n = n.min(total - 1.0).max(0.0);
    if interp && n.fract() != 0.0 {
        vec![n.floor() as i32, n.ceil() as i32]
    } else {
        vec![n.floor() as i32]
    }
}

/// `get_frame(n, interp)`.
pub fn get_frame(fr: &dyn Frames, n: f32, interp: bool, total: f32) -> Image {
    let n = n.min(total - 1.0).max(0.0);
    let a = n.floor();
    if !interp || n == a {
        return fr.get(a as i32).clone();
    }
    let (p, q) = (fr.get(a as i32), fr.get(n.ceil() as i32));
    let t = n - a;
    let mut out = p.clone();
    out.data
        .par_iter_mut()
        .zip(q.data.par_iter())
        .for_each(|(o, b)| {
            for c in 0..4 {
                o[c] += (b[c] - o[c]) * t;
            }
        });
    out
}

/// How Reverse Clip also reverses the fields (progressive frames): by moving
/// the picture a row up or down, or by averaging both moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldSwap {
    No,
    Ave,
    Up1,
    Down1,
}

/// Picture moved by `dy` rows (positive: up); the missing edge row repeats the nearest one.
fn shift_rows(src: &Image, dy: i32) -> Image {
    let (w, h) = (src.w, src.h as i32);
    let mut out = src.clone();
    out.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let mut sy = y as i32 + dy;
        if sy < 0 {
            sy = -sy;
        } else if sy >= h {
            sy = 2 * (h - 1) - sy;
        }
        let sy = sy.clamp(0, h - 1) as usize;
        row.copy_from_slice(&src.data[sy * w..(sy + 1) * w]);
    });
    out
}

pub fn swap_fields(src: Image, mode: FieldSwap) -> Image {
    match mode {
        FieldSwap::No => src,
        FieldSwap::Up1 => shift_rows(&src, 1),
        FieldSwap::Down1 => shift_rows(&src, -1),
        FieldSwap::Ave => {
            let (mut a, b) = (shift_rows(&src, 1), shift_rows(&src, -1));
            a.data
                .par_iter_mut()
                .zip(b.data.par_iter())
                .for_each(|(p, q)| {
                    for c in 0..4 {
                        p[c] = (p[c] + q[c]) * 0.5;
                    }
                });
            a
        }
    }
}

// ───────────────────────────── effects ─────────────────────────────

#[derive(Clone, Debug)]
pub enum Pick {
    /// Holds every `frames`-th frame, counted from `start`.
    FreezeFrame { start: f32, frames: f32 },
    /// Always frame `frame` (optionally blended between neighbours).
    GetFrame { frame: f32, interp: bool },
    /// Random jumps of up to `dist` frames forward, new jump every `len` frames.
    JitterFrames { len: f32, dist: f32, seed: f32 },
    /// Random `len`-frame pieces from anywhere in the clip.
    RandomEdits { len: f32, seed: f32 },
    /// The clip backwards.
    ReverseClip { fields: FieldSwap },
    /// Every `len`-frame piece played backwards (from `start`), optionally the whole clip reversed too.
    ReverseEdits {
        len: f32,
        start: f32,
        reverse_all: bool,
        fields: FieldSwap,
    },
}

impl Pick {
    /// Frame number this effect shows at `clip`.
    pub fn frame(&self, clip: Clip) -> f32 {
        let f = clip.frame;
        match *self {
            Pick::FreezeFrame { start, frames } => round_step(f - start, frames) + start,
            Pick::GetFrame { frame, .. } => frame,
            Pick::JitterFrames { len, dist, seed } => {
                let cut = round_step(f, len);
                f + round_step(random(cut + seed) * (dist + 0.99), 1.0)
            }
            Pick::RandomEdits { len, seed } => {
                let max = (clip.total - len) + 0.99;
                let cut = round_step(f, len);
                let df = f - cut;
                round_step(random(cut + seed) * max, 1.0) + df
            }
            Pick::ReverseClip { .. } => (clip.total - 1.0) - round_step(f, 1.0),
            Pick::ReverseEdits {
                len,
                start,
                reverse_all,
                ..
            } => {
                let cut = round_step(f - start, len) + start;
                let df = round_step(f, 1.0) - cut;
                let frame = cut + ((len - 1.0) - df);
                if reverse_all {
                    (clip.total - 1.0) - frame
                } else {
                    frame
                }
            }
        }
    }

    fn interp(&self) -> bool {
        matches!(self, Pick::GetFrame { interp: true, .. })
    }

    pub fn needs(&self, clip: Clip) -> Vec<i32> {
        frames_for(self.frame(clip), self.interp(), clip.total)
    }

    pub fn render(&self, clip: Clip, fr: &dyn Frames) -> Image {
        let img = get_frame(fr, self.frame(clip), self.interp(), clip.total);
        match *self {
            Pick::ReverseClip { fields } | Pick::ReverseEdits { fields, .. } => {
                swap_fields(img, fields)
            }
            _ => img,
        }
    }
}

/// Time Warp RGB: each channel from its own frame offset.
#[derive(Clone, Debug)]
pub struct WarpRgb {
    pub shift: [f32; 3],
    pub clamp_chroma: f32,
    /// Output alpha 1 (else the current frame's alpha is kept).
    pub opaque: bool,
}

impl WarpRgb {
    pub fn needs(&self, clip: Clip) -> Vec<i32> {
        self.shift
            .iter()
            .flat_map(|s| frames_for(clip.frame + s, false, clip.total))
            .collect()
    }

    /// `cur`: the current frame (working form), for its alpha.
    pub fn render(&self, clip: Clip, cur: &Image, fr: &dyn Frames) -> Image {
        let ch: Vec<Image> = self
            .shift
            .iter()
            .map(|s| get_frame(fr, clip.frame + s, false, clip.total))
            .collect();
        let mut out = cur.clone();
        let opaque = self.opaque;
        let k = self.clamp_chroma;
        out.data.par_iter_mut().enumerate().for_each(|(i, o)| {
            let mut c = [ch[0].data[i][0], ch[1].data[i][1], ch[2].data[i][2]];
            if k < 1.0 {
                let spread = c.iter().cloned().fold(f32::MIN, f32::max)
                    - c.iter().cloned().fold(f32::MAX, f32::min);
                c = crate::colorops::sat_scale(c, (k / spread).min(1.0));
            }
            o[0] = c[0];
            o[1] = c[1];
            o[2] = c[2];
            if opaque {
                o[3] = 1.0;
            }
        });
        out
    }
}

/// Time Average: mean of the last `frames` frames (incl. this one) × brightness.
#[derive(Clone, Debug)]
pub struct TimeAverage {
    pub frames: f32,
    pub brightness: f32,
}

impl TimeAverage {
    fn range(&self, clip: Clip) -> (f32, f32) {
        let f = clip.frame;
        let min = (f - (self.frames - 1.0)).max(0.0);
        (f, min)
    }

    pub fn needs(&self, clip: Clip) -> Vec<i32> {
        let (f, min) = self.range(clip);
        let mut v = Vec::new();
        let mut n = min;
        while n < f {
            v.extend(frames_for(n, false, clip.total));
            n += 1.0;
        }
        v
    }

    /// `src` is the current frame.
    pub fn render(&self, clip: Clip, src: &Image, fr: &dyn Frames) -> Image {
        let (f, min) = self.range(clip);
        let b = self.brightness;
        if !(f > min + 1.0) {
            let mut out = src.clone();
            out.data
                .par_iter_mut()
                .for_each(|p| p.iter_mut().for_each(|v| *v *= b));
            return out;
        }
        let scale = b / (f - (min - 1.0));
        let mut prev = Image::new(src.w, src.h);
        let mut n = min;
        while n < f {
            let cur = get_frame(fr, n, false, clip.total);
            prev.data
                .par_iter_mut()
                .zip(cur.data.par_iter())
                .for_each(|(p, c)| {
                    for k in 0..4 {
                        p[k] = c[k] * scale + p[k];
                    }
                });
            n += 1.0;
        }
        prev.data
            .par_iter_mut()
            .zip(src.data.par_iter())
            .for_each(|(p, s)| {
                for k in 0..4 {
                    p[k] += s[k] * scale;
                }
            });
        prev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_step_matches_original() {
        assert_eq!(round_step(7.0, 5.0), 5.0);
        assert_eq!(round_step(10.0, 5.0), 10.0);
        assert_eq!(round_step(-3.0, 5.0), -5.0);
        assert_eq!(round_step(-5.0, 5.0), -10.0);
        assert_eq!(round_step(2.5, 1.0), 2.0);
    }
}
