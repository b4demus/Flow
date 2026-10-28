//! Sharpen native branches. See spec/effects/batch3.md.
use crate::glow::AlphaMode;
use crate::{Edge, Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct SharpenParams {
    pub best: bool,
    pub amount: f32,
    pub luma: f32,
    pub chroma: f32,
    pub rgb: [f32; 3],
    pub width: f32,
    pub detail_size: f32,
    pub threshold: f32,
    pub detail_scale: [f32; 4],
    pub opacity: AlphaMode,
}

fn fast(src: &[[f32; 3]], w: usize, h: usize, r: f32, amp: [f32; 3]) -> Vec<[f32; 3]> {
    let blurred: Vec<Plane> = (0..3)
        .map(|c| {
            let mut p = Plane {
                w,
                h,
                data: src.iter().map(|v| v[c]).collect(),
            };
            crate::boxes::box_passes(&mut p, r.round(), r.round(), Edge::Reflect, 3);
            p
        })
        .collect();
    src.par_iter()
        .enumerate()
        .map(|(i, v)| std::array::from_fn(|c| v[c] + amp[c] * (v[c] - blurred[c].data[i])))
        .collect()
}

fn exp_approx(x: f32) -> f32 {
    let p = (x * 1.442695).max(-126.0);
    let z = p - p.trunc() + if p < 0.0 { 1.0 } else { 0.0 };
    let bits = (8388608.0 * (p + 121.274055 + 27.728024 / (4.8425255 - z) - 1.4901291 * z)) as u32;
    f32::from_bits(bits)
}

fn domain_line(values: &[[f32; 3]], domain: &[f32], radius: f32) -> Vec<[f32; 3]> {
    if radius <= 1.0 {
        return values.to_vec();
    }
    let n = values.len();
    let r = radius.floor() + 1.0;
    let f = radius - radius.floor();
    let mut sums = vec![[0.0; 3]; n + 1];
    for i in 0..n {
        sums[i + 1] = std::array::from_fn(|c| sums[i][c] + values[i][c]);
    }
    let (mut l, mut h) = (0, 0);
    (0..n)
        .map(|i| {
            while l < i && domain[l] < domain[i] - r {
                l += 1;
            }
            while h < n && domain[h] <= domain[i] + r {
                h += 1;
            }
            let mut il = l;
            let mut ih = h;
            if r > 2.0 {
                if domain[il] < domain[i] - (r - 1.0) {
                    il += 1;
                }
                if domain[i] + (r - 1.0) < domain[ih - 1] {
                    ih -= 1;
                }
            } else {
                il = i;
                ih = i + 1;
            }
            std::array::from_fn(|c| {
                let outer = (sums[h][c] - sums[l][c]) / (h - l) as f32;
                let inner = if ih > il {
                    (sums[ih][c] - sums[il][c]) / (ih - il) as f32
                } else {
                    values[i][c]
                };
                inner + (outer - inner) * f
            })
        })
        .collect()
}

fn best(
    input: &[[f32; 3]],
    w: usize,
    h: usize,
    mask: Option<&Plane>,
    p: &SharpenParams,
    amp: [f32; 3],
) -> Vec<[f32; 3]> {
    let mut current = input.to_vec();
    let mut result = vec![[0.0; 3]; input.len()];
    let mut sigma = p.detail_size * 0.5;
    for scale in 0..4 {
        let base = |i: usize, j: usize| {
            let m = mask.map(|m| m.data[i].min(m.data[j])).unwrap_or(1.0);
            if m >= 1.0 {
                1.0
            } else {
                (1.0 / m.max(1e-10)).min(2.687 * (sigma + 1.0))
            }
        };
        let ratio = sigma / (p.threshold * 3.0).max(1e-10);
        let delta = |i: usize, j: usize| -> f32 {
            (0..3)
                .map(|c| (input[i][c] - input[j][c]).abs())
                .sum::<f32>()
                * ratio
                + base(i, j)
        };
        let mut dx = vec![0.0; w * h];
        let mut dy = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                dx[i] = if x == 0 {
                    1.0
                } else {
                    dx[i - 1] + delta(i, i - 1)
                };
                // Y domain is stored as contiguous columns in the native filter.
                dy[x * h + y] = if y == 0 {
                    1.0
                } else {
                    dy[x * h + y - 1] + delta(i, i - w)
                };
            }
        }
        // Each scale filters the original; bands are differences between
        // consecutive independently filtered scales, not repeated blurs.
        let mut smooth = input.to_vec();
        for pass in 0..3 {
            let radius = 3.0 * sigma / (63.0f32).sqrt() * (1u32 << (2 - pass)) as f32;
            if radius <= 1.0 {
                continue;
            }
            let rows: Vec<Vec<[f32; 3]>> = (0..h)
                .into_par_iter()
                .map(|y| domain_line(&smooth[y * w..(y + 1) * w], &dx[y * w..(y + 1) * w], radius))
                .collect();
            smooth = rows.into_iter().flatten().collect();
            let cols: Vec<Vec<[f32; 3]>> = (0..w)
                .into_par_iter()
                .map(|x| {
                    let col: Vec<_> = (0..h).map(|y| smooth[y * w + x]).collect();
                    domain_line(&col, &dy[x * h..(x + 1) * h], radius)
                })
                .collect();
            for x in 0..w {
                for y in 0..h {
                    smooth[y * w + x] = cols[x][y];
                }
            }
        }
        for i in 0..input.len() {
            for c in 0..3 {
                let gain = 1.0 + (p.detail_scale[scale] - 1.0) * amp[c];
                let d = (current[i][c] - smooth[i][c]) * 2.125 * gain;
                result[i][c] += 2.0 / (1.0 + exp_approx(-d)) - 1.0;
            }
        }
        current = smooth;
        sigma *= 2.0;
    }
    result
        .iter()
        .zip(&current)
        .map(|(a, b)| std::array::from_fn(|c| a[c] + b[c]))
        .collect()
}

pub fn render(src: &Image, mask: Option<&Plane>, p: &SharpenParams) -> Image {
    let input: Vec<[f32; 3]> = src
        .data
        .par_iter()
        .map(|v| {
            let a = if p.opacity == AlphaMode::Normal {
                v[3]
            } else {
                1.0
            };
            [v[0] * a, v[1] * a, v[2] * a]
        })
        .collect();
    let process = |pixels: &[[f32; 3]], gain: [f32; 3]| {
        if p.best {
            best(pixels, src.w, src.h, mask, p, gain)
        } else {
            fast(pixels, src.w, src.h, p.width, gain)
        }
    };
    let sharp = if p.luma == p.chroma {
        process(
            &input,
            p.rgb
                .map(|v| v * p.amount * if p.best { 1.0 } else { p.luma }),
        )
    } else {
        let luma: Vec<[f32; 3]> = input
            .iter()
            .map(|v| [v[0] * 0.3125 + v[1] * 0.5 + v[2] * 0.1875; 3])
            .collect();
        let chroma: Vec<[f32; 3]> = input
            .iter()
            .zip(&luma)
            .map(|(v, l)| std::array::from_fn(|c| v[c] - l[c]))
            .collect();
        let l = process(&luma, [p.amount * p.luma; 3]);
        let c = process(
            &chroma,
            p.rgb
                .map(|v| p.amount * p.chroma * if p.best { 1.0 } else { v }),
        );
        l.iter()
            .zip(&c)
            .map(|(l, c)| std::array::from_fn(|k| l[k] + c[k]))
            .collect()
    };
    let data = src
        .data
        .par_iter()
        .enumerate()
        .map(|(i, s)| {
            let m = if p.best {
                1.0
            } else {
                mask.map(|m| m.data[i]).unwrap_or(1.0)
            };
            let a = if p.opacity == AlphaMode::AllOpaque {
                1.0
            } else {
                s[3]
            };
            let k = if p.opacity == AlphaMode::Normal && a > 1e-6 {
                1.0 / a
            } else {
                1.0
            };
            let mut v = [0.0; 4];
            for c in 0..3 {
                v[c] = (input[i][c] + (sharp[i][c] - input[i][c]) * m) * k;
            }
            v[3] = a;
            v
        })
        .collect();
    Image {
        w: src.w,
        h: src.h,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> SharpenParams {
        SharpenParams {
            best: false,
            amount: 1.0,
            luma: 1.0,
            chroma: 1.0,
            rgb: [1.0; 3],
            width: 1.0,
            detail_size: 2.4,
            threshold: 0.15,
            detail_scale: [3.0, 1.5, 1.0, 2.0],
            opacity: AlphaMode::Normal,
        }
    }

    #[test]
    fn fast_impulse_has_three_box_kernel_and_keeps_hdr_overshoot() {
        let mut src = Image::filled(15, 1, [0.0, 0.0, 0.0, 0.5]);
        src.data[7] = [1.0, 1.0, 1.0, 0.5];
        let mut p = params();
        p.width = 3.0; // Three radius-one boxes: [1,3,6,7,6,3,1] / 27.
        let out = render(&src, None, &p);
        assert!((out.data[7][0] - (2.0 - 7.0 / 27.0)).abs() < 1e-6);
        assert!((out.data[6][0] + 6.0 / 27.0).abs() < 1e-6);
        assert!(out.data.iter().all(|v| v[3] == 0.5));
    }

    #[test]
    fn both_qualities_preserve_flat_rgba() {
        let src = Image::filled(13, 9, [0.25, 0.5, 0.8, 0.6]);
        let mut p = params();
        for best in [false, true] {
            p.best = best;
            for px in render(&src, None, &p).data {
                for c in 0..4 {
                    assert!((px[c] - src.data[0][c]).abs() < 0.0002, "{px:?}");
                }
            }
        }
    }
}
