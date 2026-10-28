//! Glint: four-way star rays from bright spots.
//! Measured in AE (spec/effects/glow-observations.md, "Glint"):
//!   src'  = triple-box blur of the source, radius Threshold Blur (before the cut)
//!   T     = Glow-style threshold of src' (per-channel cut, redistributed)
//!   ray   = 1-D triple box of T along X, Y, \ (Diag1), / (Diag2); radius
//!           Size · Size dir · Size channel in whole pixels (halves round down),
//!           diagonals counted in diagonal steps, mirrored at the frame edge
//!   g_dir = min(1, 4 · Brightness · Brightness dir · Size dir · ray), ×4/3 on diagonals
//!   G     = 1 − Π(1 − g_dir), then Blur Glint, Hue Shift, Scale Colors
//!   out   = Screen of G over the source (glow::mix)

use crate::blur::{transpose, Edge};
use crate::boxes::{box3, box3_line};
use crate::darks::GRAY;
use crate::glow::{mix, source_rgb, threshold, Mix};
use crate::image::{Image, Plane};
use rayon::prelude::*;

/// Frame-edge handling of X/Y rays, Blur Glint, Threshold Blur, diagonal rays
/// (best fit to the reference on noise; see glow-observations.md).
pub const EDGES: [Edge; 4] = [
    Edge::Transparent,
    Edge::Reflect,
    Edge::Reflect,
    Edge::Transparent,
];

/// What the Matte input scales the glints by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatteType {
    Luma,
    Color,
    Alpha,
}

#[derive(Clone, Debug)]
pub struct GlintParams {
    pub brightness: f32,
    pub hue_shift: f32,
    pub scale_colors: [f32; 3],
    /// X, Y, Diag1 (\), Diag2 (/).
    pub brightness_dir: [f32; 4],
    pub threshold: f32,
    pub threshold_add: [f32; 3],
    /// Pixels.
    pub threshold_blur: f32,
    /// Pixels.
    pub size: f32,
    pub size_dir: [f32; 4],
    pub size_rgb: [f32; 3],
    /// Pixels.
    pub blur_glint: f32,
    pub from_alpha: f32,
    pub invert_matte: bool,
    pub matte_type: MatteType,
    pub mix: Mix,
    /// Frame-edge handling of the X/Y rays, Blur Glint, Threshold Blur, diagonal rays.
    pub edges: [Edge; 4],
}

impl GlintParams {
    pub fn reach(&self) -> (f32, f32) {
        let sc = self.size_rgb.iter().cloned().fold(0.0f32, f32::max);
        let sd = self.size_dir.iter().cloned().fold(0.0f32, f32::max);
        let r = (self.size.max(0.0) * sc * sd
            + self.threshold_blur.max(0.0)
            + self.blur_glint.max(0.0))
        .ceil()
            + 2.0;
        (r, r)
    }
}

/// Matte as an RGB multiplier for the chosen Matte Type.
fn matte_rgb(m: [f32; 4], t: MatteType) -> [f32; 4] {
    match t {
        MatteType::Color => m,
        MatteType::Alpha => [1.0, 1.0, 1.0, m[3]],
        MatteType::Luma => {
            let l = GRAY[0] * m[0] + GRAY[1] * m[1] + GRAY[2] * m[2];
            [l, l, l, m[3]]
        }
    }
}

/// Ray radius in whole pixels, no sub-pixel blend. The reference rounds halves
/// both ways (12.5 → 13, 13.5 → 13, 22.5 → 22, 37.5 → 38); rounding half up
/// gave the lowest error on noise.
fn ray_radius(r: f32) -> f32 {
    r.max(0.0).round()
}

/// 1-D triple box along one of the four ray directions (`r` already whole).
fn ray(p: &Plane, dir: usize, r: f32, edge: Edge) -> Plane {
    let (w, h) = (p.w, p.h);
    let mut out = p.clone();
    if r <= 0.0 {
        return out;
    }
    match dir {
        0 => out
            .data
            .par_chunks_mut(w)
            .for_each(|row| box3_line(row, r, edge, true)),
        1 => {
            let mut t = transpose(&p.data, w, h);
            t.par_chunks_mut(h)
                .for_each(|col| box3_line(col, r, edge, true));
            out.data = transpose(&t, h, w);
        }
        _ => {
            // Diagonal lines: \ has x − y constant, / has x + y constant.
            let lines = w + h - 1;
            let walk = |k: usize| -> Vec<usize> {
                let mut idx = Vec::new();
                if dir == 2 {
                    // start on the top row or the left column, step (+1, +1)
                    let (mut x, mut y) = if k < w {
                        (w - 1 - k, 0)
                    } else {
                        (0, k - w + 1)
                    };
                    while x < w && y < h {
                        idx.push(y * w + x);
                        x += 1;
                        y += 1;
                    }
                } else {
                    // start on the top row or the right column, step (−1, +1)
                    let (mut x, mut y) = if k < w {
                        (k as isize, 0usize)
                    } else {
                        (w as isize - 1, k - w + 1)
                    };
                    while x >= 0 && y < h {
                        idx.push(y * w + x as usize);
                        x -= 1;
                        y += 1;
                    }
                }
                idx
            };
            let results: Vec<(Vec<usize>, Vec<f32>)> = (0..lines)
                .into_par_iter()
                .map(|k| {
                    let idx = walk(k);
                    let mut v: Vec<f32> = idx.iter().map(|&i| p.data[i]).collect();
                    box3_line(&mut v, r, edge, true);
                    (idx, v)
                })
                .collect();
            for (idx, v) in results {
                for (i, val) in idx.into_iter().zip(v) {
                    out.data[i] = val;
                }
            }
        }
    }
    out
}

/// HSV hue rotation by `turns`, then rescaled to keep the (5,8,3)/16 luma.
pub fn hue_shift(c: [f32; 3], turns: f32) -> [f32; 3] {
    if turns == 0.0 {
        return c;
    }
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if max <= 0.0 || d <= 0.0 {
        return c;
    }
    let mut h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h = (h + turns * 6.0).rem_euclid(6.0);
    let s = d / max;
    let f = h - h.floor();
    let (p, q, t) = (
        max * (1.0 - s),
        max * (1.0 - s * f),
        max * (1.0 - s * (1.0 - f)),
    );
    let rgb = match h.floor() as i32 {
        0 => [max, t, p],
        1 => [q, max, p],
        2 => [p, max, t],
        3 => [p, q, max],
        4 => [t, p, max],
        _ => [max, p, q],
    };
    let l0 = GRAY[0] * r + GRAY[1] * g + GRAY[2] * b;
    let l1 = GRAY[0] * rgb[0] + GRAY[1] * rgb[1] + GRAY[2] * rgb[2];
    let k = if l1 > 0.0 { l0 / l1 } else { 1.0 };
    [rgb[0] * k, rgb[1] * k, rgb[2] * k]
}

pub fn render(src: &Image, bg: Option<&Image>, matte: Option<&Image>, p: &GlintParams) -> Image {
    let (w, h) = (src.w, src.h);
    let n = w * h;
    let mode = p.mix.alpha_mode;
    // 1. source as the glints see it, blurred before the cut
    let mut chans: Vec<Plane> = (0..3).map(|_| Plane::new(w, h)).collect();
    let cols: Vec<[f32; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            source_rgb(
                src.data[i],
                matte.map(|m| matte_rgb(m.data[i], p.matte_type)),
                mode,
                p.from_alpha,
                p.invert_matte,
            )
        })
        .collect();
    for (k, pl) in chans.iter_mut().enumerate() {
        pl.data = cols.iter().map(|c| c[k]).collect();
        if p.threshold_blur > 0.0 {
            box3(pl, p.threshold_blur, p.threshold_blur, p.edges[2], true);
        }
    }
    drop(cols);
    let thr: Vec<[f32; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            threshold(
                [chans[0].data[i], chans[1].data[i], chans[2].data[i]],
                p.threshold,
                p.threshold_add,
            )
        })
        .collect();
    let t_planes: Vec<Plane> = (0..3)
        .map(|k| Plane {
            w,
            h,
            data: thr.iter().map(|c| c[k]).collect(),
        })
        .collect();
    drop(thr);

    // 2. rays, screened together
    let mut star: Vec<Plane> = (0..3)
        .map(|_| Plane {
            w,
            h,
            data: vec![1.0; n],
        })
        .collect();
    for dir in 0..4 {
        let base = 4.0 * p.brightness * p.brightness_dir[dir];
        if base == 0.0 {
            continue;
        }
        let sd = p.size_dir[dir].max(0.0);
        for c in 0..3 {
            let r = ray_radius(p.size.max(0.0) * sd * p.size_rgb[c].max(0.0));
            // A ray keeps its height when Size X/Y/Diag shortens it (amplitude ∝
            // that factor, not the per-channel sizes); diagonal rays are 4/3 as
            // strong. A zero-length ray is the plain thresholded spot.
            let gain = if r >= 1.0 {
                base * sd * if dir >= 2 { 4.0 / 3.0 } else { 1.0 }
            } else {
                base
            };
            let rp = ray(
                &t_planes[c],
                dir,
                r,
                if dir >= 2 { p.edges[3] } else { p.edges[0] },
            );
            // Each ray is capped at 1 before the rays are screened together.
            star[c]
                .data
                .par_iter_mut()
                .zip(rp.data.par_iter())
                .for_each(|(s, &v)| *s *= 1.0 - (gain * v).clamp(0.0, 1.0));
        }
    }
    for pl in star.iter_mut() {
        pl.data.par_iter_mut().for_each(|v| *v = 1.0 - *v);
        if p.blur_glint > 0.0 {
            box3(pl, p.blur_glint, p.blur_glint, p.edges[1], true);
        }
    }

    // 3. colour and composite
    let data = (0..n)
        .into_par_iter()
        .map(|i| {
            let g = hue_shift(
                [star[0].data[i], star[1].data[i], star[2].data[i]],
                p.hue_shift,
            );
            let g = [
                g[0] * p.scale_colors[0],
                g[1] * p.scale_colors[1],
                g[2] * p.scale_colors[2],
            ];
            mix(src.data[i], bg.map(|b| b.data[i]), g, &p.mix)
        })
        .collect();
    Image { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_shift_like_ae() {
        // work/probe/g5.json hue_x: glint (0.2, 0.1, 0.05) turned a quarter
        let o = hue_shift([0.2, 0.1, 0.05], 0.25);
        for (a, b) in o.iter().zip([0.06883, 0.18354, 0.04589]) {
            assert!((a - b).abs() < 2e-4, "{o:?}");
        }
    }

    #[test]
    fn diagonal_ray_runs_down_right() {
        let mut p = Plane::new(9, 9);
        p.data[4 * 9 + 4] = 1.0;
        let d1 = ray(&p, 2, 3.0, Edge::Zero);
        assert!(d1.data[5 * 9 + 5] > 0.0 && d1.data[3 * 9 + 5] == 0.0);
        let d2 = ray(&p, 3, 3.0, Edge::Zero);
        assert!(d2.data[3 * 9 + 5] > 0.0 && d2.data[5 * 9 + 5] == 0.0);
    }
}
