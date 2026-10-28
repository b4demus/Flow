//! Per-pixel colour effects: Duo Tone and Vignette. spec/effects/batch2.md.
//!
//! Images in and out are straight colour.

use crate::composite::Combine;
use crate::flicker::mono;
use crate::image::Image;
use rayon::prelude::*;

/// `a + (b − a)·t` (the original's interp; t not clamped).
#[inline]
pub fn interp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The original's dissolve: interp with t cut to 0..1 (t ≤ 0 gives a, t ≥ 1 gives b exactly).
#[inline]
pub fn dissolve(a: f32, b: f32, t: f32) -> f32 {
    if t <= 0.0 {
        a
    } else if t >= 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

/// `0.5 + 0.5·cos(2π·x)` (computed as the original: cosf, then in double).
#[inline]
pub fn ncos(x: f32) -> f32 {
    let c = (x * 6.2831855).cos();
    (c as f64 * 0.5 + 0.5) as f32
}

// ───────────────────────────── Duo Tone ─────────────────────────────

#[derive(Clone, Debug)]
pub struct DuoToneParams {
    pub color0: [f32; 3],
    pub color1: [f32; 3],
    pub invert: bool,
    pub threshold: f32,
    pub softness: f32,
    pub mix: f32,
}

/// Colour between color0 and color1 by brightness (of the straight colour),
/// mixed back toward the source by `mix`; alpha stays the source's.
pub fn duotone(src: &Image, p: &DuoToneParams) -> Image {
    let mut out = src.clone();
    out.data.par_iter_mut().for_each(|px| {
        let bw = mono(*px) / p.softness + (0.5 - p.threshold / p.softness);
        let t = if p.invert { 1.0 - bw } else { bw };
        let s = *px;
        for c in 0..3 {
            px[c] = interp(dissolve(p.color0[c], p.color1[c], t), s[c], p.mix);
        }
    });
    out
}

// ───────────────────────────── Vignette ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VignetteCombine {
    Composite,
    Mult,
    Add,
    Screen,
    SubtractInv,
    VignetteOnly,
    VignetteOnlyInv,
}

#[derive(Clone, Debug)]
pub struct VignetteParams {
    /// Centre in normalised coordinates (x −1..1 across the width, y up, same scale).
    pub center: (f32, f32),
    pub squareness: f32,
    pub radius: f32,
    pub rel_height: f32,
    pub rel_width: f32,
    /// Degrees.
    pub rotate: f32,
    pub edge_softness: f32,
    pub smooth_curve: f32,
    pub color: [f32; 3],
    pub opacity: f32,
    pub source_brightness: f32,
    pub combine: VignetteCombine,
}

/// Vignette amount g (0 inside, 1 outside) at normalised point (x, y).
pub fn vignette_amount(p: &VignetteParams, x: f32, y: f32) -> f32 {
    let (dx, dy) = (x - p.center.0, y - p.center.1);
    // the original rotates by −rotate/360 turns, clockwise
    let a = (p.rotate / 360.0) * std::f32::consts::TAU;
    let (s, c) = a.sin_cos();
    let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
    let (mx, my) = (
        (rx * (1.0 / p.rel_width)).abs(),
        (ry * (1.0 / p.rel_height)).abs(),
    );
    let inner = p.radius - 0.5 * p.edge_softness;
    let corner = inner * p.squareness;
    let rounded = if mx.min(my) > corner {
        let (ex, ey) = (mx - corner, my - corner);
        corner + (ex * ex + ey * ey).sqrt()
    } else {
        mx.max(my)
    };
    let (ex, ey) = (mx.min(15.0), my.min(15.0));
    let ellipse = (ex * ex + ey * ey).sqrt();
    let amt = (1.0 - p.squareness) * (1.0 - p.squareness);
    let distance = interp(rounded, ellipse, amt);
    let linear = if p.edge_softness > 0.0 {
        ((distance - inner) / p.edge_softness).clamp(0.0, 1.0)
    } else {
        if distance > inner {
            1.0
        } else {
            0.0
        }
    };
    let smooth = ncos(linear * -0.5 + 0.5);
    interp(linear, smooth, p.smooth_curve)
}

/// `w`×`h` is the full layer (normalisation); `src` covers it from (ox, oy).
pub fn vignette(src: &Image, p: &VignetteParams, w: f32, h: f32, ox: i32, oy: i32) -> Image {
    let mut out = src.clone();
    let k = 2.0 / w;
    out.data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            let y = (0.5 * h - ((j as i32 + oy) as f32 + 0.5)) * k;
            for (i, px) in row.iter_mut().enumerate() {
                let x = ((i as i32 + ox) as f32 + 0.5) * k - 1.0;
                let g = vignette_amount(p, x, y);
                let alpha = p.opacity * g;
                let sb = p.source_brightness;
                let s = [px[0] * sb, px[1] * sb, px[2] * sb, px[3]];
                let col = p.color;
                *px = match p.combine {
                    VignetteCombine::VignetteOnly => [g, g, g, 1.0],
                    VignetteCombine::VignetteOnlyInv => [1.0 - g, 1.0 - g, 1.0 - g, 1.0],
                    VignetteCombine::Composite => [
                        interp(s[0], col[0], alpha),
                        interp(s[1], col[1], alpha),
                        interp(s[2], col[2], alpha),
                        s[3],
                    ],
                    VignetteCombine::Mult => [
                        s[0] * interp(1.0, col[0], alpha),
                        s[1] * interp(1.0, col[1], alpha),
                        s[2] * interp(1.0, col[2], alpha),
                        s[3],
                    ],
                    VignetteCombine::Add => [
                        s[0] + col[0] * alpha,
                        s[1] + col[1] * alpha,
                        s[2] + col[2] * alpha,
                        s[3],
                    ],
                    VignetteCombine::Screen => [
                        Combine::Screen.apply(s[0], col[0] * alpha),
                        Combine::Screen.apply(s[1], col[1] * alpha),
                        Combine::Screen.apply(s[2], col[2] * alpha),
                        s[3],
                    ],
                    VignetteCombine::SubtractInv => [
                        s[0] - (1.0 - col[0]) * alpha,
                        s[1] - (1.0 - col[1]) * alpha,
                        s[2] - (1.0 - col[2]) * alpha,
                        s[3],
                    ],
                };
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_softness_has_a_finite_hard_boundary() {
        let p = VignetteParams {
            center: (0.0, 0.0),
            squareness: 0.0,
            radius: 1.0,
            rel_height: 1.0,
            rel_width: 1.0,
            rotate: 0.0,
            edge_softness: 0.0,
            smooth_curve: 0.4,
            color: [0.0; 3],
            opacity: 1.0,
            source_brightness: 1.0,
            combine: VignetteCombine::Composite,
        };
        assert_eq!(vignette_amount(&p, 0.9, 0.0), 0.0);
        assert_eq!(vignette_amount(&p, 1.0, 0.0), 0.0);
        assert_eq!(vignette_amount(&p, 1.1, 0.0), 1.0);
    }
}
