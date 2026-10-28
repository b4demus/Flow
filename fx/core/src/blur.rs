//! Separable Gaussian blur of any size: direct kernel for small sigma (smooth
//! down to sigma -> 0, needed for subpixel width animation); above that the line
//! is reduced, blurred and interpolated back, so cost barely grows with radius.

use crate::image::Plane;
use rayon::prelude::*;

/// What the blur sees outside the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Zero outside (glow fades out at the frame border).
    Transparent,
    /// Mirror image (symmetric, edge pixel repeated once).
    Reflect,
    /// Zero outside, but what one pass spills past the border is kept and
    /// can come back in the next pass (box filters only).
    Zero,
}

const DIRECT_MAX_SIGMA: f32 = 4.0;

/// Blur with independent sigmas (pixels) along x and y.
pub fn gaussian(p: &Plane, sx: f32, sy: f32, edge: Edge) -> Plane {
    let mut out = p.clone();
    gaussian_in_place(&mut out, sx, sy, edge);
    out
}

pub fn gaussian_in_place(p: &mut Plane, sx: f32, sy: f32, edge: Edge) {
    if sx > 0.0 {
        blur_rows(&mut p.data, p.w, sx, edge);
    }
    if sy > 0.0 {
        let mut t = transpose(&p.data, p.w, p.h);
        blur_rows(&mut t, p.h, sy, edge);
        p.data = transpose(&t, p.h, p.w);
    }
}

fn blur_rows(data: &mut [f32], w: usize, sigma: f32, edge: Edge) {
    if w == 0 {
        return;
    }
    data.par_chunks_mut(w).for_each_init(
        || (Vec::new(), Vec::new()),
        |(pad, tmp), row| blur_line(row, sigma, edge, pad, tmp),
    );
}

#[inline]
fn mirror(i: isize, n: usize) -> usize {
    let period = 2 * n as isize;
    let m = i.rem_euclid(period);
    if m >= n as isize {
        (period - 1 - m) as usize
    } else {
        m as usize
    }
}

fn blur_line(row: &mut [f32], sigma: f32, edge: Edge, pad: &mut Vec<f32>, tmp: &mut Vec<f32>) {
    let n = row.len();
    let r = (sigma * 4.0).ceil() as usize + 3;
    pad.clear();
    pad.reserve(n + 2 * r);
    for i in -(r as isize)..(n + r) as isize {
        let v = if i >= 0 && (i as usize) < n {
            row[i as usize]
        } else if edge == Edge::Reflect {
            row[mirror(i, n)]
        } else {
            0.0
        };
        pad.push(v);
    }
    if sigma <= DIRECT_MAX_SIGMA {
        direct(pad, r, row, sigma, tmp);
    } else {
        reduced(pad, r, row, sigma, tmp);
    }
}

/// Large sigma: box-average the padded line by `d`, blur exactly at the
/// reduced rate, interpolate back. The box (variance (d²-1)/12) and the linear
/// reconstruction (d²/6) widen the kernel; that variance is taken off the blur.
fn reduced(pad: &[f32], r: usize, row: &mut [f32], sigma: f32, k: &mut Vec<f32>) {
    let d = ((sigma / 3.0).floor() as usize).max(2);
    let df = d as f32;
    let m = pad.len().div_ceil(d);
    let mut small: Vec<f32> = (0..m)
        .map(|j| {
            let s = &pad[j * d..((j + 1) * d).min(pad.len())];
            s.iter().sum::<f32>() / s.len() as f32
        })
        .collect();
    let var = sigma * sigma - (df * df - 1.0) / 12.0 - df * df / 6.0;
    let s_small = var.max(0.0).sqrt() / df;
    let kr = ((s_small * 3.5).ceil() as usize).max(1);
    let mut padded = vec![0.0f32; m + 2 * kr];
    // The line was already padded by r >= 4 sigma: the ends are far from the image.
    let (first, last) = (small[0], small[m - 1]);
    for (i, v) in padded.iter_mut().enumerate() {
        let j = i as isize - kr as isize;
        *v = if j < 0 {
            first
        } else if j as usize >= m {
            last
        } else {
            small[j as usize]
        };
    }
    direct(&padded, kr, &mut small, s_small, k);
    // Sample j sits at the centre of its box: j*d + (d-1)/2 in padded coordinates.
    let c0 = (df - 1.0) * 0.5;
    for (i, o) in row.iter_mut().enumerate() {
        let x = ((i + r) as f32 - c0) / df;
        let j = x.floor().clamp(0.0, (m - 1) as f32) as usize;
        let t = (x - j as f32).clamp(0.0, 1.0);
        let b = small[(j + 1).min(m - 1)];
        *o = small[j] + (b - small[j]) * t;
    }
}

fn direct(pad: &[f32], r: usize, row: &mut [f32], sigma: f32, k: &mut Vec<f32>) {
    let kr = ((sigma * 3.5).ceil() as usize).max(1).min(r);
    k.clear();
    let inv = 1.0 / (2.0 * sigma * sigma).max(1e-12);
    for d in -(kr as isize)..=kr as isize {
        k.push((-(d * d) as f32 * inv).exp());
    }
    let s: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    for (i, o) in row.iter_mut().enumerate() {
        let base = i + r - kr;
        let mut acc = 0.0;
        for (j, &kv) in k.iter().enumerate() {
            acc += kv * pad[base + j];
        }
        *o = acc;
    }
}

/// Cache-blocked transpose of a `w x h` plane into `h x w`.
pub fn transpose(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    const B: usize = 32;
    let mut dst = vec![0.0f32; w * h];
    dst.par_chunks_mut(h * B)
        .enumerate()
        .for_each(|(bx, chunk)| {
            let x0 = bx * B;
            let x1 = (x0 + B).min(w);
            for y0 in (0..h).step_by(B) {
                let y1 = (y0 + B).min(h);
                for x in x0..x1 {
                    let drow = &mut chunk[(x - x0) * h..(x - x0 + 1) * h];
                    for y in y0..y1 {
                        drow[y] = src[y * w + x];
                    }
                }
            }
        });
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(n: usize) -> Plane {
        let mut p = Plane::new(n, 1);
        p.data[n / 2] = 1.0;
        p
    }

    fn moments(p: &Plane) -> (f32, f32) {
        let c = (p.w / 2) as f32;
        let sum: f32 = p.data.iter().sum();
        let var: f32 = p
            .data
            .iter()
            .enumerate()
            .map(|(i, v)| v * (i as f32 - c).powi(2))
            .sum::<f32>()
            / sum;
        (sum, var.sqrt())
    }

    #[test]
    fn sigma_is_honoured() {
        // Below ~1 px a sampled Gaussian is narrower than its nominal sigma.
        for sigma in [1.0f32, 2.0, 3.9, 4.1, 10.0, 60.0, 130.0] {
            let out = gaussian(&impulse(3001), sigma, 0.0, Edge::Transparent);
            let (sum, sd) = moments(&out);
            assert!((sum - 1.0).abs() < 1e-3, "sigma {sigma}: sum {sum}");
            assert!((sd - sigma).abs() / sigma < 0.03, "sigma {sigma}: sd {sd}");
        }
    }

    #[test]
    fn reflect_keeps_flat_field() {
        let p = Plane {
            w: 50,
            h: 40,
            data: vec![0.7; 2000],
        };
        let out = gaussian(&p, 30.0, 30.0, Edge::Reflect);
        assert!(out.data.iter().all(|v| (v - 0.7).abs() < 1e-3));
    }

    #[test]
    fn transpose_roundtrip() {
        let (w, h) = (37, 70);
        let src: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let t = transpose(&src, w, h);
        assert_eq!(t[3 * h + 5], src[5 * w + 3]);
        assert_eq!(transpose(&t, h, w), src);
    }
}
