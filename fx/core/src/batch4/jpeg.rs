//! JPEG damage recovered from 180424B76, 180425391, 18042570E, 180426038/325.
//! No entropy stream is needed: quantized blocks go directly to the decoder.
use super::jpeg_dct::{fdct, idct};
use crate::{shake::hash4, Image};
use rayon::prelude::*;

// Standard JPEG example quantizers, not film/color LUTs or artistic resources.
const LUMA: [i32; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA: [i32; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];
fn unit(hash: u32) -> f64 {
    (hash & 0x3fffffff) as f64 / 1073741824.0
}

fn quantizers(quality: f32) -> [[i32; 64]; 2] {
    let q = ((quality * 100.0) as f64 + 0.5) as i32;
    let q = q.clamp(1, 100);
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    [LUMA, CHROMA].map(|base| base.map(|v| ((v * scale + 50) / 100).clamp(1, 32767)))
}

struct Damage {
    counter: u32,
    remaining: i32,
    seed: u32,
    probability: f32,
    max_run: i32,
    errors: f32,
}
impl Damage {
    fn new(width: usize, seed: f32, errors: f32, counter: u32) -> Self {
        let max_run = (width as f64 * 0.5 + 1.0) as i32;
        let half_run = (max_run as f64 * 0.5).max(1.0) as f32;
        Self {
            counter,
            remaining: 0,
            seed: ((seed * 4096.0) as i32 as u32).wrapping_mul(0x8235),
            probability: (0.75 / (half_run as f64 * 0.25 + 1.0)) as f32,
            max_run,
            errors,
        }
    }
    fn apply(&mut self, coef: &mut [i32; 64]) {
        let count = self.counter;
        self.counter = count.wrapping_add(1);
        let mut random = count.wrapping_add(self.seed);
        if self.remaining == 0 {
            let probability = if count == 0 { 0.75 } else { self.probability };
            random = hash4(random);
            if unit(random) >= probability as f64 {
                return;
            }
            let length = if count == 0 {
                (self.max_run / 2).max(1)
            } else {
                self.max_run
            };
            random = hash4(random);
            self.remaining = (unit(random) * length as f64) as i32 + 1;
        }
        self.remaining -= 1;
        random = hash4(random);
        let mut n = self.errors as i32;
        if unit(random) < (self.errors - n as f32) as f64 {
            n += 1;
        }
        for _ in 0..n {
            random = hash4(random);
            let i = (random & 63) as usize;
            let frequency = (i / 8).max(i % 8);
            let amplitude = (1000.0 * (1.0 - frequency as f64 * (0.8 / 7.0))) as i32;
            random = hash4(random);
            coef[i] += (unit(random) * (2 * amplitude + 1) as f64) as i32 - amplitude;
        }
    }
}

fn ycc(rgb: [i32; 3]) -> [u8; 3] {
    let [r, g, b] = rgb;
    [
        ((19595 * r + 38470 * g + 7471 * b + 32768) >> 16) as u8,
        ((-11059 * r - 21709 * g + 32768 * b + 8421375) >> 16) as u8,
        ((32768 * r - 27439 * g - 5329 * b + 8421375) >> 16) as u8,
    ]
}
fn rgb(y: u8, cb: u8, cr: u8) -> [f32; 3] {
    let (y, cb, cr) = (y as i32, cb as i32 - 128, cr as i32 - 128);
    [
        y + ((91881 * cr + 32768) >> 16),
        y + ((-22554 * cb - 46802 * cr + 32768) >> 16),
        y + ((116130 * cb + 32768) >> 16),
    ]
    .map(|v| v.clamp(0, 255) as f32 / 256.0)
}

fn strip(
    src: &Image,
    top: usize,
    height: usize,
    quality: f32,
    errors: f32,
    seed: f32,
    counter: u32,
) -> Vec<[f32; 3]> {
    let w = src.w;
    let packed: Vec<[u8; 3]> = (0..w * height)
        .map(|i| {
            // Sapphire's working RGB is signed fixed point with 4096 == 1.
            ycc(std::array::from_fn(|c| {
                let fixed = (src.data[top * w + i][c] * 4096.0) as i32;
                (fixed >> 4).clamp(0, 255)
            }))
        })
        .collect();
    let quant = quantizers(quality);
    let inverse_quant = quant.map(|q| {
        std::array::from_fn::<_, 64, _>(|i| {
            if i == 0 {
                q[i]
            } else {
                (q[i] as f32 * (1.0 + errors)) as i32
            }
        })
    });
    let mcu_w = w.div_ceil(16);
    let mcu_h = height.div_ceil(16);
    let mut planes: [Vec<u8>; 3] = std::array::from_fn(|_| vec![0; mcu_w * mcu_h * 256]);
    let stride = mcu_w * 16;
    let mut damage = Damage::new(w, seed, errors, counter);
    for my in 0..mcu_h {
        for mx in 0..mcu_w {
            for component in 0..3 {
                let factor = if component == 0 { 1 } else { 2 };
                let count = if component == 0 { 2 } else { 1 };
                for by in 0..count {
                    for bx in 0..count {
                        // Decoder omits IDCT (and its damage callback) for dummy
                        // luma blocks at the right/bottom of an MCU.
                        if mx * 16 + bx * 8 * factor >= w || my * 16 + by * 8 * factor >= height {
                            continue;
                        }
                        let mut block = [0; 64];
                        for y in 0..8 {
                            for x in 0..8 {
                                let sx = mx * 16 + (bx * 8 + x) * factor;
                                // Pad vertically after downsampling. A repeated
                                // chroma row contains the final pair's average.
                                let sy = (my * 16 + (by * 8 + y) * factor)
                                    .min((height.div_ceil(factor) - 1) * factor);
                                let at = |dx: usize, dy: usize| {
                                    packed[(sy + dy).min(height - 1) * w + (sx + dx).min(w - 1)]
                                        [component] as i32
                                };
                                block[y * 8 + x] = if factor == 1 {
                                    at(0, 0) as u8
                                } else {
                                    ((at(0, 0)
                                        + at(1, 0)
                                        + at(0, 1)
                                        + at(1, 1)
                                        + if x % 2 == 0 { 1 } else { 2 })
                                        >> 2) as u8
                                };
                            }
                        }
                        let qi = usize::from(component != 0);
                        let mut coef = fdct(&block, &quant[qi]);
                        for i in 0..64 {
                            coef[i] *= inverse_quant[qi][i];
                        }
                        if errors > 0.0 {
                            damage.apply(&mut coef);
                        }
                        let decoded = idct(&coef);
                        for y in 0..8 {
                            for x in 0..8 {
                                for dy in 0..factor {
                                    for dx in 0..factor {
                                        let ox = mx * 16 + (bx * 8 + x) * factor + dx;
                                        let oy = my * 16 + (by * 8 + y) * factor + dy;
                                        planes[component][oy * stride + ox] = decoded[y * 8 + x];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (0..w * height)
        .map(|i| {
            let x = i % w;
            let y = i / w;
            let at = y * stride + x;
            let chroma = |component: usize| {
                if w.div_ceil(2) <= 2 {
                    return planes[component][at];
                }
                let cx = x / 2;
                let cy = y / 2;
                let nx = if x % 2 == 0 {
                    cx.saturating_sub(1)
                } else {
                    (cx + 1).min(w.div_ceil(2) - 1)
                };
                let ny = if y % 2 == 0 {
                    cy.saturating_sub(1)
                } else {
                    (cy + 1).min(height.div_ceil(2) - 1)
                };
                let sample = |px, py| planes[component][py * 2 * stride + px * 2] as i32;
                let near = sample(cx, cy) * 3 + sample(cx, ny);
                let far = sample(nx, cy) * 3 + sample(nx, ny);
                ((near * 3 + far + if x % 2 == 0 { 8 } else { 7 }) >> 4) as u8
            };
            rgb(planes[0][at], chroma(1), chroma(2))
        })
        .collect()
}

pub(super) fn render(src: &Image, quality: f32, errors: f32, seed: f32) -> Image {
    if src.w == 0 || src.h == 0 {
        return src.clone();
    }
    let step = ((src.h + 15) / 4) & !7;
    let strips: Vec<_> = (0..4usize)
        .into_par_iter()
        .filter_map(|n| {
            let top = step * n;
            let height = if n == 3 {
                src.h.saturating_sub(top)
            } else {
                step.min(src.h.saturating_sub(top))
            };
            if height == 0 {
                None
            } else {
                Some((
                    top,
                    strip(
                        src,
                        top,
                        height,
                        quality,
                        errors,
                        seed,
                        ((step * 3 * src.w) >> 6) as u32 * n as u32,
                    ),
                ))
            }
        })
        .collect();
    let mut out = src.clone();
    for (top, pixels) in strips {
        for (i, pixel) in pixels.iter().enumerate() {
            out.data[top * src.w + i][..3].copy_from_slice(pixel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quality_endpoints_and_jpeg_fixed_point_scale() {
        assert_eq!(quantizers(1.0), [[1; 64]; 2]);
        assert_eq!(quantizers(0.0)[0][0], 800);
        for (value, expected) in [(0.0, 0.0), (0.5, 0.5), (1.0, 255.0 / 256.0)] {
            let src = Image::filled(17, 19, [value, value, value, 0.37]);
            let out = render(&src, 1.0, 0.0, 1.23);
            assert!(out
                .data
                .iter()
                .all(|p| p[..3] == [expected; 3] && p[3] == 0.37));
        }
    }
    #[test]
    fn corruption_depends_on_seed_and_preserves_alpha_for_partial_blocks() {
        let src = Image::filled(35, 37, [0.4, 0.6, 0.8, 0.23]);
        let a = render(&src, 0.1, 0.8, 2.0);
        assert_eq!(a.data, render(&src, 0.1, 0.8, 2.0).data);
        assert_ne!(a.data, render(&src, 0.1, 0.8, 3.0).data);
        assert!(a
            .data
            .iter()
            .all(|p| p[3] == 0.23 && p.iter().all(|v| v.is_finite())));
    }
}
