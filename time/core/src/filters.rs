//! Small image filters: pyramids, blur, morphology, outlier removal, hole filling.

use crate::image::{Flow, Gray, Image};
use rayon::prelude::*;

/// Halve a plane with a [1 2 1] prefilter. Odd sizes round up.
pub fn downsample(g: &Gray) -> Gray {
    let w = g.w.div_ceil(2).max(1);
    let h = g.h.div_ceil(2).max(1);
    let src = &g.data;
    let (sw, sh) = (g.w as isize, g.h as isize);
    let px = |x: isize, y: isize| src[(y.clamp(0, sh - 1) * sw + x.clamp(0, sw - 1)) as usize];
    let data = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((2 * (i % w)) as isize, (2 * (i / w)) as isize);
            let mut acc = 0.0;
            for (dy, ky) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                for (dx, kx) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                    acc += ky * kx * px(x + dx, y + dy);
                }
            }
            acc / 16.0
        })
        .collect();
    Gray { w, h, data }
}

/// Builds levels until the short side drops below `min_side`.
pub fn pyramid(g: &Gray, min_side: usize, max_levels: usize) -> Vec<Gray> {
    let mut levels = vec![g.clone()];
    while levels.len() < max_levels {
        let last = levels.last().unwrap();
        if last.w.min(last.h) / 2 < min_side {
            break;
        }
        levels.push(downsample(last));
    }
    levels
}

/// Separable box blur repeated three times (close to a gaussian).
pub fn blur(g: &Gray, radius: usize) -> Gray {
    if radius == 0 {
        return g.clone();
    }
    let mut cur = g.data.clone();
    for _ in 0..3 {
        cur = box_pass(&cur, g.w, g.h, radius, true);
        cur = box_pass(&cur, g.w, g.h, radius, false);
    }
    Gray { w: g.w, h: g.h, data: cur }
}

fn box_pass(src: &[f32], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0; w * h];
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = move |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let cols: Vec<Vec<f32>> = (0..lines)
        .into_par_iter()
        .map(|line| {
            let mut res = vec![0.0; n];
            let ri = r as isize;
            let get = |i: isize| src[idx(line, i.clamp(0, n as isize - 1) as usize)];
            let mut acc: f32 = (-ri..=ri).map(get).sum();
            let k = 1.0 / (2 * r + 1) as f32;
            for i in 0..n as isize {
                res[i as usize] = acc * k;
                acc += get(i + ri + 1) - get(i - ri);
            }
            res
        })
        .collect();
    for (line, col) in cols.into_iter().enumerate() {
        for (i, v) in col.into_iter().enumerate() {
            out[idx(line, i)] = v;
        }
    }
    out
}

/// Grayscale dilation with a square window (used to grow mattes by N pixels).
pub fn dilate(g: &Gray, radius: f32) -> Gray {
    let r = radius.ceil() as isize;
    if r <= 0 {
        return g.clone();
    }
    let (w, h) = (g.w as isize, g.h as isize);
    let rows: Vec<f32> = (0..g.w * g.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % g.w) as isize, (i / g.w) as isize);
            let mut m: f32 = 0.0;
            for xx in (x - r).max(0)..=(x + r).min(w - 1) {
                m = m.max(g.data[(y * w + xx) as usize]);
            }
            m
        })
        .collect();
    let data = (0..g.w * g.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % g.w) as isize, (i / g.w) as isize);
            let mut m: f32 = 0.0;
            for yy in (y - r).max(0)..=(y + r).min(h - 1) {
                m = m.max(rows[(yy * w + x) as usize]);
            }
            m
        })
        .collect();
    Gray { w: g.w, h: g.h, data }
}

/// 3x3 per-component median: removes isolated wrong vectors after densification.
pub fn median3(f: &Flow) -> Flow {
    let (w, h) = (f.w as isize, f.h as isize);
    let data = (0..f.w * f.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % f.w) as isize, (i / f.w) as isize);
            let mut a = [0.0f32; 9];
            let mut b = [0.0f32; 9];
            let mut n = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let v = f.data[((y + dy).clamp(0, h - 1) * w + (x + dx).clamp(0, w - 1)) as usize];
                    a[n] = v[0];
                    b[n] = v[1];
                    n += 1;
                }
            }
            [median9(a), median9(b)]
        })
        .collect();
    Flow { w: f.w, h: f.h, data }
}

/// Median of 9 with a fixed comparison network (Paeth / Devillard).
#[inline]
fn median9(mut p: [f32; 9]) -> f32 {
    macro_rules! s {
        ($a:expr, $b:expr) => {
            if p[$a] > p[$b] {
                p.swap($a, $b);
            }
        };
    }
    s!(1, 2); s!(4, 5); s!(7, 8); s!(0, 1); s!(3, 4); s!(6, 7); s!(1, 2); s!(4, 5); s!(7, 8);
    s!(0, 3); s!(5, 8); s!(4, 7); s!(3, 6); s!(1, 4); s!(2, 5); s!(4, 7); s!(4, 2); s!(6, 4); s!(4, 2);
    p[4]
}

/// Push-pull hole filling: pixels with weight ~0 get a smooth extension of
/// their weighted neighbourhood. Works for any fixed-size channel vector.
pub fn push_pull<const N: usize>(vals: &[[f32; N]], weights: &[f32], w: usize, h: usize) -> Vec<[f32; N]> {
    // Level 0: premultiply by weight.
    let mut levels: Vec<(Vec<[f32; N]>, Vec<f32>, usize, usize)> = Vec::new();
    let mut cur_v: Vec<[f32; N]> = vals
        .iter()
        .zip(weights)
        .map(|(v, &k)| {
            let k = k.clamp(0.0, 1.0);
            let mut o = [0.0; N];
            for c in 0..N {
                o[c] = v[c] * k;
            }
            o
        })
        .collect();
    let mut cur_w: Vec<f32> = weights.iter().map(|k| k.clamp(0.0, 1.0)).collect();
    let (mut cw, mut ch) = (w, h);
    levels.push((cur_v.clone(), cur_w.clone(), cw, ch));
    while cw > 1 || ch > 1 {
        let nw = cw.div_ceil(2);
        let nh = ch.div_ceil(2);
        let mut nv = vec![[0.0; N]; nw * nh];
        let mut nwt = vec![0.0; nw * nh];
        for y in 0..ch {
            for x in 0..cw {
                let j = (y / 2) * nw + x / 2;
                let i = y * cw + x;
                for c in 0..N {
                    nv[j][c] += cur_v[i][c];
                }
                nwt[j] += cur_w[i];
            }
        }
        // Normalise so the weight stays in 0..1 (a fully covered parent has weight 1).
        for j in 0..nw * nh {
            let k = nwt[j];
            if k > 1.0 {
                for c in 0..N {
                    nv[j][c] /= k;
                }
                nwt[j] = 1.0;
            }
        }
        cur_v = nv;
        cur_w = nwt;
        cw = nw;
        ch = nh;
        levels.push((cur_v.clone(), cur_w.clone(), cw, ch));
    }
    // Pull: blend each level with the upsampled coarser estimate.
    let mut est: Vec<[f32; N]> = {
        let (v, wt, _, _) = levels.last().unwrap();
        v.iter()
            .zip(wt)
            .map(|(p, &k)| {
                let mut o = [0.0; N];
                if k > 1e-6 {
                    for c in 0..N {
                        o[c] = p[c] / k;
                    }
                }
                o
            })
            .collect()
    };
    for li in (0..levels.len() - 1).rev() {
        let (v, wt, lw, lh) = &levels[li];
        let (_, _, pw, ph) = &levels[li + 1];
        let mut next = vec![[0.0; N]; lw * lh];
        for y in 0..*lh {
            for x in 0..*lw {
                let i = y * lw + x;
                // Bilinear upsample of the coarser estimate.
                let sx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (*pw - 1) as f32);
                let sy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (*ph - 1) as f32);
                let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(pw - 1), (y0 + 1).min(ph - 1));
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                let k = wt[i].min(1.0);
                for c in 0..N {
                    let a = est[y0 * pw + x0][c] * (1.0 - fx) + est[y0 * pw + x1][c] * fx;
                    let b = est[y1 * pw + x0][c] * (1.0 - fx) + est[y1 * pw + x1][c] * fx;
                    let up = a * (1.0 - fy) + b * fy;
                    let own = if wt[i] > 1e-6 { v[i][c] / wt[i] } else { 0.0 };
                    next[i][c] = own * k + up * (1.0 - k);
                }
            }
        }
        est = next;
    }
    est
}

pub fn fill_flow(f: &Flow, weights: &Gray) -> Flow {
    Flow { w: f.w, h: f.h, data: push_pull(&f.data, &weights.data, f.w, f.h) }
}

pub fn fill_image(img: &Image, weights: &Gray) -> Image {
    Image { w: img.w, h: img.h, data: push_pull(&img.data, &weights.data, img.w, img.h) }
}

/// Image Prep modes from the spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePrep {
    None,
    ContrastEdge,
    Delinearize,
}

pub fn prep(g: &Gray, mode: ImagePrep) -> Gray {
    match mode {
        ImagePrep::None => g.clone(),
        ImagePrep::Delinearize => g.map(|v| v.max(0.0).powf(1.0 / 2.2)),
        ImagePrep::ContrastEdge => {
            // Local contrast normalisation: subtract the local mean and divide by
            // the local standard deviation, then squash back into 0..1.
            let r = (g.w.max(g.h) / 120).max(4);
            let mean = blur(g, r);
            let diff: Vec<f32> = g.data.iter().zip(&mean.data).map(|(a, m)| a - m).collect();
            let var = blur(&Gray { w: g.w, h: g.h, data: diff.iter().map(|d| d * d).collect() }, r);
            let data = diff
                .par_iter()
                .zip(var.data.par_iter())
                .map(|(d, v)| 0.5 + 0.25 * (d / (v.sqrt() + 0.02)).clamp(-2.0, 2.0))
                .collect();
            Gray { w: g.w, h: g.h, data }
        }
    }
}
