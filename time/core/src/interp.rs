//! Building an in-between frame from two source frames and their flows.

use crate::image::{Flow, Gray, Image};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameInterp {
    Nearest,
    Blend,
    MotionWeightedBlend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warping {
    Inverse,
    InverseSmartBlend,
    Forward,
    ForwardNoSmartBlend,
}

impl Warping {
    fn smart(self) -> bool {
        matches!(self, Warping::InverseSmartBlend | Warping::Forward)
    }
}

/// Weight of frame B at fraction `t` for the chosen interpolation.
fn weight_b(t: f32, interp: FrameInterp) -> f32 {
    match interp {
        FrameInterp::Nearest => {
            if t < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        FrameInterp::Blend => t,
        // Leans harder towards the nearer source frame, so the amount of
        // blend-softening changes less from frame to frame.
        FrameInterp::MotionWeightedBlend => {
            let (a, b) = ((1.0 - t) * (1.0 - t), t * t);
            b / (a + b)
        }
    }
}

/// Plain cross-fade / nearest frame, used when no vectors are computed.
pub fn blend_only(a: &Image, b: &Image, t: f32, interp: FrameInterp) -> Image {
    let wb = weight_b(t, interp);
    if wb <= 0.0 {
        return a.clone();
    }
    if wb >= 1.0 {
        return b.clone();
    }
    Image::lerp(a, b, wb)
}

/// Forward-backward consistency: 1 where the vector at a pixel of the source
/// frame is confirmed by the opposite flow, falling to 0 in occlusions.
pub fn visibility(fwd: &Flow, bwd: &Flow) -> Gray {
    let _t = crate::prof::span("interp.visibility");
    let data = (0..fwd.w * fwd.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % fwd.w) as f32, (i / fwd.w) as f32);
            let v = fwd.data[i];
            let (tx, ty) = (x + v[0], y + v[1]);
            if tx < -0.5 || ty < -0.5 || tx > fwd.w as f32 - 0.5 || ty > fwd.h as f32 - 0.5 {
                return 0.0;
            }
            let b = bwd.sample(tx, ty);
            let e2 = (v[0] + b[0]).powi(2) + (v[1] + b[1]).powi(2);
            let mag2 = v[0] * v[0] + v[1] * v[1] + b[0] * b[0] + b[1] * b[1];
            let tol = 0.01 * mag2 + 0.5;
            (-e2 / tol).exp()
        })
        .collect();
    Gray { w: fwd.w, h: fwd.h, data }
}

/// Synthesises the frame at fraction `t` (0 = A, 1 = B).
/// `fab` maps A pixels into B, `fba` maps B pixels into A.
pub fn interpolate(a: &Image, b: &Image, fab: &Flow, fba: &Flow, t: f32, interp: FrameInterp, warping: Warping) -> Image {
    let t = t.clamp(0.0, 1.0);
    let wb = weight_b(t, interp);
    let vis = if warping.smart() { Some((visibility(fab, fba), visibility(fba, fab))) } else { None };
    match warping {
        Warping::Inverse | Warping::InverseSmartBlend => inverse_warp(a, b, fab, fba, t, wb, vis.as_ref()),
        Warping::Forward | Warping::ForwardNoSmartBlend => {
            let fallback = inverse_warp(a, b, fab, fba, t, wb, vis.as_ref());
            forward_warp(a, b, fab, fba, t, wb, vis.as_ref(), &fallback)
        }
    }
}

/// Backward sampling with the linear approximation of the intermediate flows
/// (as in Super SloMo): F_t0 = -(1-t)t F01 + t^2 F10, F_t1 = (1-t)^2 F01 - t(1-t) F10.
fn inverse_warp(a: &Image, b: &Image, fab: &Flow, fba: &Flow, t: f32, wb: f32, vis: Option<&(Gray, Gray)>) -> Image {
    let _t = crate::prof::span("interp.inverse");
    let (w, h) = (a.w, a.h);
    let data = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let f01 = fab.data[i];
            let f10 = fba.data[i];
            let ft0 = [-(1.0 - t) * t * f01[0] + t * t * f10[0], -(1.0 - t) * t * f01[1] + t * t * f10[1]];
            let ft1 = [
                (1.0 - t) * (1.0 - t) * f01[0] - t * (1.0 - t) * f10[0],
                (1.0 - t) * (1.0 - t) * f01[1] - t * (1.0 - t) * f10[1],
            ];
            let (ca, in_a) = a.sample(x + ft0[0], y + ft0[1]);
            let (cb, in_b) = b.sample(x + ft1[0], y + ft1[1]);
            let (mut wa, mut wbb) = (1.0 - wb, wb);
            if let Some((va, vb)) = vis {
                wa *= if in_a { va.sample(x + ft0[0], y + ft0[1]) } else { 0.0 };
                wbb *= if in_b { vb.sample(x + ft1[0], y + ft1[1]) } else { 0.0 };
                if wa + wbb < 1e-3 {
                    // Both sides disagree: fall back to the plain weights.
                    wa = if in_a || !in_b { 1.0 - wb } else { 0.0 };
                    wbb = if in_b || !in_a { wb } else { 0.0 };
                }
            }
            let s = wa + wbb;
            if s <= 0.0 {
                return ca;
            }
            let (ka, kb) = (wa / s, wbb / s);
            [
                ca[0] * ka + cb[0] * kb,
                ca[1] * ka + cb[1] * kb,
                ca[2] * ka + cb[2] * kb,
                ca[3] * ka + cb[3] * kb,
            ]
        })
        .collect();
    Image { w, h, data }
}

/// Splats one source frame along `flow * k` into an accumulator.
fn splat(src: &Image, flow: &Flow, k: f32, vis: Option<&Gray>) -> (Vec<[f32; 4]>, Vec<f32>) {
    let _t = crate::prof::span("interp.splat");
    let (w, h) = (src.w, src.h);
    // Scatter is sequential (cheap compared to flow); a parallel version would
    // need a full-frame buffer per thread.
    let mut acc = vec![[0.0f32; 4]; w * h];
    let mut wt = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let v = flow.data[i];
            let (tx, ty) = (x as f32 + v[0] * k, y as f32 + v[1] * k);
            // Occluded pixels lose against visible ones landing on the same spot.
            let z = vis.map_or(1.0, |m| 0.02 + m.data[i]);
            let (x0, y0) = (tx.floor(), ty.floor());
            let (fx, fy) = (tx - x0, ty - y0);
            let c = src.data[i];
            for (dx, dy, bw) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
                let (qx, qy) = (x0 as isize + dx, y0 as isize + dy);
                if qx < 0 || qy < 0 || qx >= w as isize || qy >= h as isize {
                    continue;
                }
                let j = qy as usize * w + qx as usize;
                let ww = bw * z;
                for ch in 0..4 {
                    acc[j][ch] += c[ch] * ww;
                }
                wt[j] += ww;
            }
        }
    }
    (acc, wt)
}

#[allow(clippy::too_many_arguments)]
fn forward_warp(
    a: &Image,
    b: &Image,
    fab: &Flow,
    fba: &Flow,
    t: f32,
    wb: f32,
    vis: Option<&(Gray, Gray)>,
    fallback: &Image,
) -> Image {
    let (va, vb) = match vis {
        Some((va, vb)) => (Some(va), Some(vb)),
        None => (None, None),
    };
    let (acc_a, wt_a) = splat(a, fab, t, va);
    let (acc_b, wt_b) = splat(b, fba, 1.0 - t, vb);
    let data = (0..a.w * a.h)
        .into_par_iter()
        .map(|i| {
            // Coverage: how confidently each side landed here.
            let ca = (wt_a[i] / 0.25).min(1.0);
            let cb = (wt_b[i] / 0.25).min(1.0);
            let mut ka = (1.0 - wb) * ca;
            let mut kb = wb * cb;
            if ka + kb < 1e-3 {
                // One side is fully weighted off (Nearest) or nothing landed: take
                // whichever side has coverage, else the inverse-warp result.
                ka = ca * if wb < 1.0 { 1.0 } else { 0.0 };
                kb = cb * if wb > 0.0 { 1.0 } else { 0.0 };
                if ka + kb < 1e-3 {
                    return fallback.data[i];
                }
            }
            let s = ka + kb;
            let mut out = [0.0f32; 4];
            for ch in 0..4 {
                let pa = if wt_a[i] > 0.0 { acc_a[i][ch] / wt_a[i] } else { 0.0 };
                let pb = if wt_b[i] > 0.0 { acc_b[i][ch] / wt_b[i] } else { 0.0 };
                out[ch] = (pa * ka + pb * kb) / s;
            }
            // Partially covered pixels lean on the fallback to avoid speckles.
            let cover = (ca.max(cb)).min(1.0);
            for ch in 0..4 {
                out[ch] = out[ch] * cover + fallback.data[i][ch] * (1.0 - cover);
            }
            out
        })
        .collect();
    Image { w: a.w, h: a.h, data }
}

/// Smears the frame along the local motion. `amount` is the virtual shutter as a
/// fraction of the frame interval (0.5 = 180 degrees); `frames_per_output` is
/// how many source frames pass per output frame.
pub fn motion_blur(img: &Image, flow: &Flow, amount: f32, frames_per_output: f32) -> Image {
    let span = amount * (frames_per_output.abs() - 1.0).max(0.0);
    if span <= 1e-3 {
        return img.clone();
    }
    let data = (0..img.w * img.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % img.w) as f32, (i / img.w) as f32);
            let v = flow.data[i];
            let (dx, dy) = (v[0] * span, v[1] * span);
            let len = (dx * dx + dy * dy).sqrt();
            let n = (len.ceil() as usize).clamp(1, 48);
            if n == 1 {
                return img.data[i];
            }
            let mut acc = [0.0f32; 4];
            for k in 0..n {
                let s = k as f32 / (n - 1) as f32 - 0.5;
                let (c, _) = img.sample(x + dx * s, y + dy * s);
                for ch in 0..4 {
                    acc[ch] += c[ch];
                }
            }
            let inv = 1.0 / n as f32;
            [acc[0] * inv, acc[1] * inv, acc[2] * inv, acc[3] * inv]
        })
        .collect();
    Image { w: img.w, h: img.h, data }
}
