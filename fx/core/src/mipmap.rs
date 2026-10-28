//! Filtered warp sampling: the older isotropic MipSampler used by
//! WarpTransform, and the recovered independent-X/Y RipSampler used by RGB warps.

use crate::image::Image;
use crate::shake::{Frame, Sampler, Wrap};
use rayon::prelude::*;

pub struct MipSampler {
    levels: Vec<Sampler>,
    /// Level sizes.
    sizes: Vec<(usize, usize)>,
}

fn half(src: &Image) -> Image {
    let (w, h) = (src.w.div_ceil(2).max(1), src.h.div_ceil(2).max(1));
    let mut out = Image::new(w, h);
    out.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let mut acc = [0f32; 4];
            let mut n = 0.0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (2 * x + dx, 2 * y + dy);
                if sx < src.w && sy < src.h {
                    let p = src.data[sy * src.w + sx];
                    for c in 0..4 {
                        acc[c] += p[c];
                    }
                    n += 1.0;
                }
            }
            *o = acc.map(|v| v / n);
        }
    });
    out
}

impl MipSampler {
    pub fn new(premult: &Image, wrap: [Wrap; 2]) -> Self {
        let mut levels = vec![Sampler::new(premult, wrap)];
        let mut sizes = vec![(premult.w, premult.h)];
        let mut cur = premult.clone();
        while cur.w > 1 || cur.h > 1 {
            cur = half(&cur);
            levels.push(Sampler::new(&cur, wrap));
            sizes.push((cur.w, cur.h));
            if levels.len() > 16 {
                break;
            }
        }
        Self { levels, sizes }
    }

    /// Sample at buffer pixel (px, py) of level 0 with level-of-detail `lod`.
    pub fn at_px(&self, px: f32, py: f32, lod: f32) -> [f32; 4] {
        let lod = lod.clamp(0.0, (self.levels.len() - 1) as f32);
        let l0 = lod.floor() as usize;
        let t = lod - l0 as f32;
        let a = self.level(l0, px, py);
        if t <= 0.0 || l0 + 1 >= self.levels.len() {
            return a;
        }
        let b = self.level(l0 + 1, px, py);
        let mut o = [0f32; 4];
        for c in 0..4 {
            o[c] = a[c] + (b[c] - a[c]) * t;
        }
        o
    }

    fn level(&self, l: usize, px: f32, py: f32) -> [f32; 4] {
        if l == 0 {
            return self.levels[0].at_px(px, py);
        }
        let s = (1u32 << l) as f32;
        let _ = self.sizes[l];
        self.levels[l].at_px((px + 0.5) / s - 0.5, (py + 0.5) / s - 0.5)
    }

    /// Sample at normalised point (x, y) of frame `fr`, `lod` as in `at_px`.
    pub fn at(&self, fr: &Frame, x: f32, y: f32, lod: f32) -> [f32; 4] {
        let s = 0.5 * fr.w;
        let (px, py) = (
            -0.5 + (x + 1.0) * s,
            (0.5 * fr.h - 0.5 - 0.5 * fr.w) + (-y + 1.0) * s,
        );
        self.at_px(px - fr.src_x as f32, py - fr.src_y as f32, lod)
    }
}

/// Level of detail from the sample-point steps (in normalised units) between
/// neighbouring output pixels along x and y.
#[inline]
pub fn lod(fr: &Frame, d_dx: (f32, f32), d_dy: (f32, f32)) -> f32 {
    let s = 0.5 * fr.w;
    let a = (d_dx.0 * d_dx.0 + d_dx.1 * d_dx.1).sqrt() * s;
    let b = (d_dy.0 * d_dy.0 + d_dy.1 * d_dy.1).sqrt() * s;
    let rho = a.max(b);
    if rho <= 1.0 {
        0.0
    } else {
        rho.log2()
    }
}

/// Independent X/Y pyramids with triangular interpolation of three levels.
/// Recovered from 0x1806687D8 and 0x1806696A1; see spec/effects/warp-rgb.md.
pub struct RipSampler {
    levels: Vec<Vec<Sampler>>,
    size: (usize, usize),
    wrap: [Wrap; 2],
}

fn half_axis(src: &Image, axis: usize, wrap: [Wrap; 2]) -> Image {
    let (w, h) = if axis == 0 {
        (src.w.div_ceil(2), src.h)
    } else {
        (src.w, src.h.div_ceil(2))
    };
    let smp = Sampler::new(src, wrap);
    let mut out = Image::new(w, h);
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let (sx, sy) = if axis == 0 { (x * 2, y) } else { (x, y * 2) };
                let a = smp.at_px(sx as f32, sy as f32);
                let b = smp.at_px(
                    (sx + (axis == 0) as usize) as f32,
                    (sy + (axis == 1) as usize) as f32,
                );
                *o = std::array::from_fn(|c| (a[c] + b[c]) * 0.5);
            }
        });
    out
}

impl RipSampler {
    pub fn new(src: &Image, wrap: [Wrap; 2]) -> Self {
        let nx = (src.w.max(1) as f32).log2().ceil() as usize + 2;
        let ny = (src.h.max(1) as f32).log2().ceil() as usize + 2;
        let mut levels = Vec::with_capacity(nx);
        let mut x_image = src.clone();
        for lx in 0..nx {
            if lx > 0 {
                x_image = half_axis(&x_image, 0, wrap);
            }
            let mut y_image = x_image.clone();
            let mut column = Vec::with_capacity(ny);
            for ly in 0..ny {
                if ly > 0 {
                    y_image = half_axis(&y_image, 1, wrap);
                }
                column.push(Sampler::new(&y_image, wrap));
            }
            levels.push(column);
        }
        Self {
            levels,
            size: (src.w, src.h),
            wrap,
        }
    }

    pub fn at(&self, fr: &Frame, x: f32, y: f32, lod: [f32; 2]) -> [f32; 4] {
        let s = 0.5 * fr.w;
        // The native warp wraps at level zero BEFORE converting coordinates
        // for each mip level. Reflecting after reduction moves the seam.
        let px = crate::shake::wrap(
            -0.5 + (x + 1.0) * s - fr.src_x as f32,
            self.size.0 as i32,
            self.wrap[0],
        );
        let py = crate::shake::wrap(
            (0.5 * fr.h - 0.5 - 0.5 * fr.w) + (-y + 1.0) * s - fr.src_y as f32,
            self.size.1 as i32,
            self.wrap[1],
        );
        let lx = lod[0].clamp(0.0, self.levels.len() as f32 - 1.001);
        let ly = lod[1].clamp(0.0, self.levels[0].len() as f32 - 1.001);
        let (ix, iy) = (lx.floor() as usize, ly.floor() as usize);
        let (tx, ty) = (lx - ix as f32, ly - iy as f32);
        let sample = |a: usize, b: usize| {
            self.levels[a][b].at_px_wrapped(
                (px + 0.5) / (1u32 << a) as f32 - 0.5,
                (py + 0.5) / (1u32 << b) as f32 - 0.5,
            )
        };
        let a = sample(ix, iy);
        let b = sample(ix + 1, iy + 1);
        if tx == ty {
            return std::array::from_fn(|c| a[c] + (b[c] - a[c]) * tx);
        }
        if tx > ty {
            let m = sample(ix + 1, iy);
            std::array::from_fn(|c| a[c] * (1.0 - tx) + b[c] * ty + m[c] * (tx - ty))
        } else {
            let m = sample(ix, iy + 1);
            std::array::from_fn(|c| a[c] * (1.0 - ty) + b[c] * tx + m[c] * (ty - tx))
        }
    }
}

/// Native approximation of log2: exponent plus a linear mantissa.
pub fn fast_log2(x: f32) -> f32 {
    let bits = x.max(f32::MIN_POSITIVE).to_bits();
    f32::from_bits((bits & 0x807fffff) + 0x3f800000) + (((bits >> 23) & 255) as i32 - 128) as f32
}

#[cfg(test)]
mod rip_tests {
    use super::*;

    #[test]
    fn reflects_full_source_before_level_conversion() {
        let src = Image {
            w: 4,
            h: 1,
            data: (0..4).map(|x| [x as f32; 4]).collect(),
        };
        let rip = RipSampler::new(&src, [Wrap::Reflect; 2]);
        let fr = Frame {
            w: 4.0,
            h: 1.0,
            src_x: 0,
            src_y: 0,
        };
        let (x, y) = crate::shake::pixel_to_norm(&fr, 6.0, 0.0);
        // Base coordinate 6 reflects to 2. At level 1, .75 interpolates
        // averages .5 and 2.5. Wrapping after reduction incorrectly gives 2.5.
        assert_eq!(rip.at(&fr, x, y, [1.0, 0.0]), [2.0; 4]);
    }

    #[test]
    fn fractional_levels_keep_flat_odd_sized_tile() {
        let src = Image::filled(7, 5, [0.125, 0.25, 0.5, 1.0]);
        let rip = RipSampler::new(&src, [Wrap::Tile; 2]);
        let fr = Frame {
            w: 7.0,
            h: 5.0,
            src_x: 0,
            src_y: 0,
        };
        for lod in [[0.0, 0.0], [0.7, 1.2], [2.6, 0.3], [9.0, 8.0]] {
            let got = rip.at(&fr, 3.4, -2.2, lod);
            for (a, b) in got.into_iter().zip(src.data[0]) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }
}
