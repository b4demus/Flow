//! Triple box blur: three running-mean passes whose integer radii add up to R
//! (spread evenly, remainder on the last passes). Compact support of exactly ±R
//! and close to Gaussian (sd ≈ R/3). Fractional R blends the floor and ceil
//! kernels along each axis, so width animates smoothly.

use crate::blur::{transpose, Edge};
use crate::image::Plane;
use rayon::prelude::*;

/// Radii of the three passes for total radius `r`: 10 -> [3, 3, 4], 11 -> [3, 4, 4].
pub fn split_radius(r: usize) -> [usize; 3] {
    let (b, rem) = (r / 3, r % 3);
    [b, b + (rem >= 2) as usize, b + (rem >= 1) as usize]
}

/// Blurs `p` with total radii `rx`, `ry` (pixels). `subpixel = false` rounds them.
pub fn box3(p: &mut Plane, rx: f32, ry: f32, edge: Edge, subpixel: bool) {
    let (rx, ry) = if subpixel {
        (rx.max(0.0), ry.max(0.0))
    } else {
        (rx.max(0.0).round(), ry.max(0.0).round())
    };
    if rx > 0.0 {
        rows(&mut p.data, p.w, rx, edge);
    }
    if ry > 0.0 {
        let mut t = transpose(&p.data, p.w, p.h);
        rows(&mut t, p.h, ry, edge);
        p.data = transpose(&t, p.h, p.w);
    }
}

fn rows(data: &mut [f32], w: usize, r: f32, edge: Edge) {
    if w == 0 {
        return;
    }
    data.par_chunks_mut(w)
        .for_each_init(|| Scratch::default(), |s, row| line(row, r, edge, s));
}

/// Triple box along one line of samples (any direction the caller extracted).
pub fn box3_line(line_px: &mut [f32], r: f32, edge: Edge, subpixel: bool) {
    let r = if subpixel {
        r.max(0.0)
    } else {
        r.max(0.0).round()
    };
    if r > 0.0 && !line_px.is_empty() {
        line(line_px, r, edge, &mut Scratch::default());
    }
}

/// Box / Triangle / Gauss use one / two / three box passes. The radius
/// is the total support, with the remainder assigned to the later passes.
pub fn box_passes(p: &mut Plane, rx: f32, ry: f32, edge: Edge, passes: usize) {
    if passes == 3 {
        let mx = 3 * (p.w / 2).saturating_sub(1);
        let my = 3 * (p.h / 2).saturating_sub(1);
        box3(p, rx.min(mx as f32), ry.min(my as f32), edge, true);
        return;
    }
    let passes = passes.clamp(1, 3);
    let rows = |data: &mut [f32], w: usize, radius: f32| {
        if w == 0 || radius <= 0.0 {
            return;
        }
        // Native split_radius caps each pass to half the available dimension.
        let max_r = passes * (w / 2).saturating_sub(1);
        let r = radius.min(max_r as f32);
        let lo = r.floor() as usize;
        let hi = r.ceil() as usize;
        let t = (r - lo as f32) as f64;
        data.par_chunks_mut(w).for_each(|row| {
            let pad = hi + 1;
            let src: Vec<f64> = (-(pad as isize)..(w + pad) as isize)
                .map(|i| {
                    if i >= 0 && i < w as isize {
                        row[i as usize] as f64
                    } else if edge == Edge::Reflect {
                        row[mirror(i, w)] as f64
                    } else {
                        0.0
                    }
                })
                .collect();
            let filter = |r: usize| {
                let mut out = src.clone();
                let mut used = 0;
                for pass in 0..passes {
                    let rad = (r - used) / (passes - pass);
                    used += rad;
                    if rad == 0 {
                        continue;
                    }
                    let mut sums = Vec::with_capacity(out.len() + 1);
                    sums.push(0.0);
                    for &v in &out {
                        sums.push(sums.last().unwrap() + v);
                    }
                    for (i, o) in out.iter_mut().enumerate() {
                        *o = (sums[(i + rad + 1).min(src.len())] - sums[i.saturating_sub(rad)])
                            / (2 * rad + 1) as f64;
                    }
                    if edge == Edge::Transparent {
                        out[..pad].fill(0.0);
                        out[pad + w..].fill(0.0);
                    }
                }
                out
            };
            let a = filter(lo);
            if lo == hi {
                for (i, o) in row.iter_mut().enumerate() {
                    *o = a[i + pad] as f32;
                }
            } else {
                let b = filter(hi);
                for (i, o) in row.iter_mut().enumerate() {
                    *o = (a[i + pad] * (1.0 - t) + b[i + pad] * t) as f32;
                }
            }
        });
    };
    rows(&mut p.data, p.w, rx);
    if ry > 0.0 && p.w > 0 && p.h > 0 {
        let mut t = transpose(&p.data, p.w, p.h);
        rows(&mut t, p.h, ry);
        p.data = transpose(&t, p.h, p.w);
    }
}

#[derive(Default)]
struct Scratch {
    pad: Vec<f64>,
    a: Vec<f64>,
    b: Vec<f64>,
    sum: Vec<f64>,
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

fn line(row: &mut [f32], r: f32, edge: Edge, s: &mut Scratch) {
    let n = row.len();
    let rf = r.floor() as usize;
    let frac = r - rf as f32;
    let rc = if frac > 1e-6 { rf + 1 } else { rf };
    // Reflect: mirror once, and what spills past the border during one pass
    // comes back in the next. Transparent: every pass sees zero outside, so
    // spilled light is lost (both measured against the reference). Zero: zero
    // padding, spill kept.
    let pad = if edge == Edge::Transparent { 0 } else { rc + 1 };
    s.pad.clear();
    for i in -(pad as isize)..(n + pad) as isize {
        let v = if i >= 0 && (i as usize) < n {
            row[i as usize]
        } else if edge == Edge::Reflect {
            row[mirror(i, n)]
        } else {
            0.0
        };
        s.pad.push(v as f64);
    }
    run3(&s.pad, rf, &mut s.a, &mut s.sum);
    if rc != rf {
        run3(&s.pad, rc, &mut s.b, &mut s.sum);
        let f = frac as f64;
        for (i, o) in row.iter_mut().enumerate() {
            *o = (s.a[i + pad] * (1.0 - f) + s.b[i + pad] * f) as f32;
        }
    } else {
        for (i, o) in row.iter_mut().enumerate() {
            *o = s.a[i + pad] as f32;
        }
    }
}

/// Three box passes of total radius `r` over `src`; `out` has the same length.
/// Outside `src` every pass sees zeros (the window is still divided by 2r+1).
fn run3(src: &[f64], r: usize, out: &mut Vec<f64>, sum: &mut Vec<f64>) {
    out.clear();
    out.extend_from_slice(src);
    for rad in split_radius(r) {
        if rad == 0 {
            continue;
        }
        let n = out.len();
        sum.clear();
        sum.push(0.0);
        let mut acc = 0.0;
        for v in out.iter() {
            acc += v;
            sum.push(acc);
        }
        let k = 1.0 / (2 * rad + 1) as f64;
        for i in 0..n {
            let lo = i.saturating_sub(rad);
            let hi = (i + rad + 1).min(n);
            out[i] = (sum[hi] - sum[lo]) * k;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn triangle_transparent_border_discards_each_pass_spill() {
        let mut p = crate::Plane {
            w: 7,
            h: 1,
            data: vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        super::box_passes(&mut p, 2.0, 0.0, crate::Edge::Transparent, 2);
        for (got, want) in p
            .data
            .iter()
            .zip([2.0 / 9.0, 2.0 / 9.0, 1.0 / 9.0, 0.0, 0.0, 0.0, 0.0])
        {
            assert!((got - want).abs() < 1e-7);
        }
    }
    use super::*;

    fn step(n: usize, edge_at: usize) -> Plane {
        Plane {
            w: n,
            h: 1,
            data: (0..n)
                .map(|i| if i <= edge_at { 1.0 } else { 0.0 })
                .collect(),
        }
    }

    /// k(d) = s(edge+d) - s(edge+1+d) from a blurred step, as measured in AE.
    fn kernel_from_step(p: &Plane, edge: usize, d: isize) -> f32 {
        let i = (edge as isize + d) as usize;
        p.data[i] - p.data[i + 1]
    }

    #[test]
    fn radii_split_like_reference() {
        assert_eq!(split_radius(1), [0, 0, 1]);
        assert_eq!(split_radius(10), [3, 3, 4]);
        assert_eq!(split_radius(11), [3, 4, 4]);
        assert_eq!(split_radius(14), [4, 5, 5]);
        assert_eq!(split_radius(40), [13, 13, 14]);
        assert_eq!(split_radius(56), [18, 19, 19]);
    }

    #[test]
    fn matches_measured_kernel_r10() {
        // S_Glow, Glow Width 10, red channel (work/probe/e3.json, step_w10).
        let mut p = step(400, 199);
        box3(&mut p, 10.0, 0.0, Edge::Reflect, true);
        for (d, want) in [
            (0, 0.09751),
            (2, 0.08844),
            (4, 0.06349),
            (6, 0.03401),
            (8, 0.01361),
            (10, 0.00227),
            (11, 0.0),
        ] {
            let got = kernel_from_step(&p, 199, d);
            assert!((got - want).abs() < 2e-5, "d {d}: {got} vs {want}");
        }
    }

    #[test]
    fn subpixel_blends_neighbours() {
        // Glow Width 10.5: measured sd 4.000 (= sqrt of the mean of the R=10 and R=11 variances).
        let mut p = step(400, 199);
        box3(&mut p, 10.5, 0.0, Edge::Reflect, true);
        let (mut s, mut v) = (0.0f64, 0.0f64);
        for d in -20..=20 {
            let k = kernel_from_step(&p, 199, d) as f64;
            s += k;
            v += k * (d * d) as f64;
        }
        assert!(((v / s).sqrt() - 4.0).abs() < 2e-3);
        // and the measured centre tap
        assert!((kernel_from_step(&p, 199, 0) - 0.09373).abs() < 2e-5);
    }

    #[test]
    fn transparent_edge_like_ae() {
        // S_Glow, Edge Mode Transparent, width 40, white over x 0..59 (work/probe/c1.json edge_transp).
        let mut p = Plane {
            w: 400,
            h: 1,
            data: (0..400).map(|i| if i < 60 { 1.0 } else { 0.0 }).collect(),
        };
        box3(&mut p, 40.0, 0.0, Edge::Transparent, true);
        for (x, want) in [(0, 0.3371), (5, 0.4986), (20, 0.9125), (30, 0.9761)] {
            assert!(
                (p.data[x] - want).abs() < 2e-4,
                "x {x}: {} vs {want}",
                p.data[x]
            );
        }
    }

    #[test]
    fn flat_field_is_kept() {
        let mut p = Plane {
            w: 30,
            h: 20,
            data: vec![0.4; 600],
        };
        box3(&mut p, 45.0, 17.3, Edge::Reflect, true);
        assert!(p.data.iter().all(|v| (v - 0.4).abs() < 1e-5));
    }
}
