//! Rays graph and native weighted walk. spec/effects/batch3.md.
use crate::glow::{self, AlphaMode, Mix};
use crate::shake::{pixel_to_norm, Frame, Sampler, Wrap};
use crate::{Edge, Image, Plane};
use rayon::prelude::*;
#[derive(Clone, Debug)]
pub struct RaysParams {
    pub center: [f32; 2],
    pub length: f32,
    pub length_rgb: [f32; 3],
    pub reverse: f32,
    pub brightness: f32,
    pub color: [f32; 3],
    pub dark: bool,
    pub bias: f32,
    pub res: usize,
    pub blur: f32,
    pub blur_rel: [f32; 2],
    pub threshold: f32,
    pub threshold_add: [f32; 3],
    pub chroma: f32,
    pub from_alpha: f32,
    pub matte_type: i32,
    pub invert_matte: bool,
    pub blur_matte: f32,
    pub shimmer: f32,
    pub shimmer_freq: f32,
    pub shimmer_seed: f32,
    pub shimmer_shift: [f32; 2],
    pub time: f32,
    pub mix: Mix,
}

pub fn render(
    src: &Image,
    bg: Option<&Image>,
    matte: Option<&Image>,
    fr: &Frame,
    p: &RaysParams,
) -> Image {
    let mut masks: Vec<Plane> = (0..3)
        .map(|c| {
            let mut m = Plane {
                w: src.w,
                h: src.h,
                data: (0..src.data.len())
                    .map(|i| {
                        let v = matte
                            .map(|m| {
                                let v = m.data[i];
                                let a = if p.mix.alpha_mode == AlphaMode::Normal {
                                    v[3]
                                } else {
                                    1.0
                                };
                                match p.matte_type {
                                    2 => v[c] * a,
                                    3 if p.mix.alpha_mode != AlphaMode::AllOpaque => v[3],
                                    3 => v[0] * a,
                                    _ => crate::flicker::mono(v) * a,
                                }
                            })
                            .unwrap_or(1.0);
                        if p.invert_matte {
                            1.0 - v
                        } else {
                            v
                        }
                    })
                    .collect(),
            };
            crate::boxes::box_passes(
                &mut m,
                p.blur_matte.round(),
                p.blur_matte.round(),
                Edge::Reflect,
                3,
            );
            m
        })
        .collect();
    let mut gen = Image::new(src.w, src.h);
    gen.data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, g) in row.iter_mut().enumerate() {
                let i = y * src.w + x;
                let s = src.data[i];
                let mut v = glow::source_rgb(s, None, p.mix.alpha_mode, 0.0, false);
                let a = if p.mix.alpha_mode == AlphaMode::AllOpaque {
                    1.0
                } else {
                    s[3]
                };
                for c in 0..3 {
                    if p.dark {
                        v[c] = 1.0 - v[c];
                    }
                    v[c] += (a - v[c]) * p.from_alpha;
                }
                let total = v.iter().map(|v| v.max(0.0)).sum::<f32>();
                let excess = (0..3)
                    .map(|c| {
                        let t = if p.dark {
                            1.0 - p.threshold - p.threshold_add[c]
                        } else {
                            p.threshold + p.threshold_add[c]
                        };
                        (v[c] - t).max(0.0)
                    })
                    .sum::<f32>();
                let hot = v.map(|v| if total > 0.0 { v * excess / total } else { 0.0 });
                let mono = hot[0] * 0.3125 + hot[1] * 0.5 + hot[2] * 0.1875;
                let n = pixel_to_norm(fr, x as f32 + fr.src_x as f32, y as f32 + fr.src_y as f32);
                // The Noise image primitive's reconstruction still differs from
                // warp_noise. This pattern uses the recovered hash/kernel, and is
                // explicitly tracked as approximate until the image resampler lands.
                let noise = if p.shimmer == 0.0 {
                    1.0
                } else {
                    1.0 + p.shimmer * 0.5
                        - p.shimmer
                            * crate::shake::noise2(
                                n.0 * p.shimmer_freq * 2.0 - p.shimmer_shift[0],
                                n.1 * p.shimmer_freq * 2.0 - p.shimmer_shift[1],
                                1,
                                (p.shimmer_seed * 4096.0) as i32,
                            )
                };
                for c in 0..3 {
                    g[c] = (mono + (hot[c] - mono) * p.chroma) * noise * masks[c].data[i];
                }
                g[3] = 1.0;
            }
        });
    masks.clear();
    let factor = p.res.max(1);
    if factor > 1 {
        gen = crate::resample::shrink(&gen, factor);
    }
    let smp = Sampler::new(&gen, [Wrap::No; 2]);
    let f = factor as f32;
    let center = [
        (p.center[0] - fr.src_x as f32 + 0.5) / f - 0.5,
        (p.center[1] - fr.src_y as f32 + 0.5) / f - 0.5,
    ];
    let mut lengths = p.length_rgb.map(|v| v * p.length);
    if p.dark && lengths.iter().all(|v| *v != 0.0) {
        let mag = lengths.iter().map(|v| v * v).sum::<f32>().sqrt();
        let inv = lengths.map(|v| 1.0 / v);
        let invmag = inv.iter().map(|v| v * v).sum::<f32>().sqrt();
        lengths = inv.map(|v| v * mag / invmag);
    }
    let weights = if p.reverse > 0.0 {
        [p.bias, 1.0 - p.bias, p.bias]
    } else {
        [1.0 - p.bias, p.bias, p.bias]
    };
    let split = if p.reverse > 0.0 {
        p.reverse / (p.reverse + 1.0)
    } else {
        1.0
    };
    let mut rays = Image::new(gen.w, gen.h);
    rays.data
        .par_chunks_mut(gen.w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let d = [x as f32 - center[0], y as f32 - center[1]];
                for c in 0..3 {
                    let rev = (-lengths[c] * p.reverse).min(1.0);
                    let a = [
                        center[0] + d[0] * (1.0 - rev),
                        center[1] + d[1] * (1.0 - rev),
                    ];
                    let b = [
                        center[0] + d[0] * (1.0 - lengths[c]),
                        center[1] + d[1] * (1.0 - lengths[c]),
                    ];
                    o[c] = smp.line(a, b, weights, split, true)[c] * p.brightness;
                }
                o[3] = 1.0;
            }
        });
    if factor > 1 {
        rays = crate::resample::grow(&rays, factor, src.w, src.h);
    }
    for c in 0..3 {
        let mut pl = rays.channel(c);
        crate::boxes::box_passes(
            &mut pl,
            p.blur * p.blur_rel[0],
            p.blur * p.blur_rel[1],
            Edge::Reflect,
            3,
        );
        rays.set_channel(c, &pl);
    }
    let data = src
        .data
        .par_iter()
        .enumerate()
        .map(|(i, s)| {
            let light = std::array::from_fn(|c| rays.data[i][c] * p.color[c]);
            let mut out = glow::mix(*s, bg.map(|b| b.data[i]), light, &p.mix);
            if p.dark {
                let (sp, sa) = glow::source_layer(*s, p.mix.alpha_mode, p.mix.source_opacity);
                let (bp, ba) = glow::bg_layer(bg.map(|b| b.data[i]), p.mix.bg_brightness);
                let (base, _) = glow::over(sp, sa, bp, ba);
                let dark: [f32; 3] = std::array::from_fn(|c| rays.data[i][c] * (1.0 - p.color[c]));
                let color = std::array::from_fn(|c| {
                    let over =
                        1.0 - crate::composite::Combine::Screen.apply(1.0 - base[c], dark[c]);
                    let b = if bg.is_some() {
                        1.0 - crate::composite::Combine::Screen.apply(1.0 - bp[c], dark[c])
                    } else {
                        1.0 - dark[c]
                    };
                    let under = sp[c] + b * (1.0 - sa);
                    over + (under - over) * p.mix.under_source
                });
                let a = out[3];
                out = glow::finish(color, a, p.mix.alpha_mode);
            }
            out
        })
        .collect();
    Image {
        w: src.w,
        h: src.h,
        data,
    }
}
