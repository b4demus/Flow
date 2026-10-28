//! Premultiplied box reduction and pixel-centred bilinear enlargement.
use crate::shake::{Sampler, Wrap};
use crate::Image;
use rayon::prelude::*;
pub fn shrink(src: &Image, f: usize) -> Image {
    let f = f.max(1);
    let (w, h) = (src.w.div_ceil(f), src.h.div_ceil(f));
    let mut out = Image::new(w, h);
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let mut n = 0.0f32;
                for sy in y * f..((y + 1) * f).min(src.h) {
                    for sx in x * f..((x + 1) * f).min(src.w) {
                        for c in 0..4 {
                            o[c] += src.data[sy * src.w + sx][c];
                        }
                        n += 1.0;
                    }
                }
                for v in o {
                    *v /= n.max(1.0);
                }
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_size_reduction_keeps_edge_blocks_and_flat_premult_color() {
        let src = Image::filled(13, 7, [0.1, 0.2, 0.3, 0.4]);
        for f in [2, 4] {
            let small = shrink(&src, f);
            assert_eq!(
                (small.w, small.h),
                (13usize.div_ceil(f), 7usize.div_ceil(f))
            );
            let out = grow(&small, f, 13, 7);
            for px in out.data {
                for c in 0..4 {
                    assert!((px[c] - src.data[0][c]).abs() < 1e-6);
                }
            }
        }
    }
}
pub fn grow(src: &Image, f: usize, w: usize, h: usize) -> Image {
    let smp = Sampler::new(src, [Wrap::Reflect; 2]);
    let mut out = Image::new(w, h);
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                *o = smp.at_px(
                    ((x as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (src.w - 1) as f32),
                    ((y as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, (src.h - 1) as f32),
                );
            }
        });
    out
}
