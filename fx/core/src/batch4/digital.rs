//! Seven-stage DigitalDamage graph, including the recovered JPEG damage stage.
use super::noise::Blocks;
use super::{mix, old_frame_number, rescale, Context, Settings};
use crate::{
    colorops::sat_scale,
    shake::{pixel_to_norm, Sampler, Wrap},
    Image,
};
use rayon::prelude::*;

fn gate(p: &Settings, ctx: &Context, prefix: &str, seed: f32, offset: f32) -> f32 {
    let threshold = mix(
        1.0,
        1.0 - p.f(&format!("{prefix} Always")),
        p.f("Time Intensity"),
    );
    let v = crate::shake::random_sawtooth(
        seed,
        (ctx.time - offset) * p.f(&format!("{prefix} Blink Freq")),
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
    );
    if v > threshold {
        1.0
    } else {
        0.0
    }
}
fn block(
    grid: [f32; 2],
    freq: [f32; 3],
    ctx: &Context,
    seed: f32,
    drift: f32,
    speed: f32,
    layers: usize,
    decay: f32,
    grid_scale: f32,
    noise_scale: f32,
    phase: [f32; 2],
) -> Blocks {
    Blocks {
        grid,
        freq,
        time: ctx.time,
        seed: (seed * 4096.0) as i32,
        drift,
        drift_speed: speed,
        layers,
        decay,
        grid_scale,
        noise_scale,
        phase,
    }
}
fn image_map(
    src: &Image,
    ctx: &Context,
    f: impl Fn(usize, [f32; 2], [f32; 4]) -> [f32; 4] + Sync,
) -> Image {
    let data = src
        .data
        .par_iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = pixel_to_norm(
                &ctx.frame,
                (i % src.w) as f32 + ctx.frame.src_x as f32,
                (i / src.w) as f32 + ctx.frame.src_y as f32,
            );
            f(i, [x, y], *v)
        })
        .collect();
    Image {
        w: src.w,
        h: src.h,
        data,
    }
}

pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let intensity = p.f("Intensity");
    let seed = p.f("Seed");
    let time_seed = seed + ctx.time;
    let freq = [
        1.0 / (p.f("Damage Size") * p.f("Damage Size Rel X")).max(0.001),
        1.0 / p.f("Damage Size").max(0.001),
    ];
    let freq3 = [freq[0], freq[1], 1.0];
    let old_n = old_frame_number(p, ctx.frame_number, ctx.total_frames);
    let old = ctx
        .old
        .map(|old| crate::temporal::to_working(old.clone(), p.mode()))
        .unwrap_or_else(|| src.clone());
    let time_freeze =
        seed + crate::temporal::round_step(ctx.frame_number, p.f("Freeze Frames").max(1.0));
    let mut out = src.clone();
    if p.b("Freeze") && gate(p, ctx, "Freeze", seed + 1.0, 0.0) > 0.0 {
        let mut frozen = old.clone();
        let sat = p.f("Freeze Saturation");
        for v in &mut frozen.data {
            let s = sat_scale([v[0], v[1], v[2]], sat);
            v[..3].copy_from_slice(&s);
        }
        frozen = super::jpeg::render(
            &frozen,
            p.f("Freeze Quality"),
            p.f("Freeze Errs"),
            seed + 1.111,
        );
        let bands = block(
            [freq[0] * 1.5, freq[1] * 4.0],
            [freq[0] * 0.2, freq[1] * 15.0, 0.0],
            ctx,
            time_freeze,
            1.0,
            0.0,
            5,
            0.78,
            0.6,
            1.15,
            [0.5; 2],
        );
        let threshold = mix(1.0, p.f("Freeze Threshold"), intensity);
        out = image_map(&out, ctx, |i, xy, v| {
            if bands.at(xy[0], xy[1]).clamp(0.0, 1.0) > threshold {
                frozen.data[i]
            } else {
                v
            }
        });
    }
    if p.b("Shift") {
        let blink = gate(p, ctx, "Shift", seed + 2.0, 0.0);
        let amount = p.f("Shift Amount") * intensity * blink;
        if amount != 0.0 {
            let smp = Sampler::new(&out, [Wrap::No; 2]);
            let bands = block(
                [freq[0] * 0.5, freq[1] * 6.5],
                [freq3[0] * 1.8, freq3[1] * 6.8, 2.5],
                ctx,
                seed + 1.34,
                0.7,
                5.0,
                4,
                0.74,
                0.7,
                1.5,
                [0.0; 2],
            );
            let threshold = p.f("Shift Threshold") - (intensity - 1.0).max(0.0) * 0.5;
            out = image_map(&out, ctx, |_, xy, _| {
                let n = bands.at(xy[0], xy[1]);
                let n = if n.abs() < threshold { 0.0 } else { n };
                smp.at(&ctx.frame, xy[0] + amount * n, xy[1])
            });
        }
    }
    let color_blocks: Vec<Blocks> = (0..3)
        .map(|c| {
            block(
                [freq[0] * 11.0, freq[1] * 19.0],
                [freq[0] * 10.0, freq[1] * 17.0, 1.0],
                ctx,
                time_seed + c as f32 + 1.0,
                1.0,
                1.0,
                5,
                0.88,
                0.66,
                1.3,
                [0.0; 2],
            )
        })
        .collect();
    let subtract = block(
        [freq[0] * 5.5, freq[1] * 10.4],
        [freq[0] * 1.6, freq[1] * 3.0, 1.0],
        ctx,
        time_seed + 4.0,
        1.0,
        1.0,
        5,
        0.56,
        0.66,
        1.3,
        [0.0; 2],
    );
    let rgb_noise = |xy: [f32; 2]| -> [f32; 3] {
        let t = [0.31, 0.36, 0.52];
        std::array::from_fn(|c| rescale(color_blocks[c].at(xy[0], xy[1]), t[c], 1.0))
    };
    let diff_noise = |xy: [f32; 2], rgb: [f32; 3]| {
        let b = rescale(subtract.at(xy[0], xy[1]), 0.2, 0.25);
        rgb.map(|v| (v - b).abs())
    };
    if p.b("Brights Noise") && gate(p, ctx, "Brights", seed + 1.1112, 1.5) > 0.0 {
        let threshold = p.f("Brights Threshold");
        let band_threshold = mix(1.0, p.f("Brights Band Threshold"), intensity);
        let band_freq = p.f("Brights Band Freq");
        out = image_map(&out, ctx, |i, xy, v| {
            if crate::flicker::mono(src.data[i]) > threshold
                && crate::shake::noise2(0.0, xy[1] * band_freq, 4, (time_seed * 4096.0) as i32)
                    > band_threshold
            {
                let rgb = diff_noise(xy, rgb_noise(xy));
                [rgb[0], rgb[1], rgb[2], 1.0]
            } else {
                v
            }
        });
    }
    if p.b("Pixelate") {
        let blink = gate(p, ctx, "Pixelate", seed + 4.11, 0.0);
        if blink > 0.0 {
            let twitch_seed = crate::shake::random_sawtooth(
                seed + 9.876,
                ctx.time,
                0.0,
                1.0,
                0.0,
                1.0,
                p.f("Pixelate Hold"),
            );
            let bands = block(
                [1.5, 4.0],
                [0.2, 15.0, 0.0],
                ctx,
                twitch_seed,
                1.0,
                0.0,
                5,
                0.78,
                0.6,
                1.15,
                [0.5; 2],
            );
            let threshold = mix(1.0, p.f("Pixelate Threshold"), intensity);
            let inv = [
                2.0 / (p.f("Pixelate Frequency").max(1.0) * freq[0]),
                2.0 / (p.f("Pixelate Frequency").max(1.0) * freq[1]),
            ];
            let smp = Sampler::new(&out, [Wrap::No; 2]);
            let overdrive = p.f("Pixelate Overdrive");
            out = image_map(&out, ctx, |_, xy, v| {
                let m = rescale(
                    bands.at(xy[0], xy[1]).clamp(0.0, 1.0),
                    threshold,
                    threshold + 0.2,
                ) * overdrive;
                let pos: [f32; 2] = std::array::from_fn(|c| {
                    crate::temporal::round_step(xy[c], inv[c]) + 0.5 * inv[c]
                });
                let q = smp.at(&ctx.frame, pos[0], pos[1]);
                std::array::from_fn(|c| mix(v[c], q[c], m))
            });
        }
    }
    if p.b("Block Noise") {
        let blink1 = gate(p, ctx, "Blocks", seed + 3.1415, 0.0);
        let blink2 = gate(p, ctx, "Blocks", seed + 3.15, 0.0);
        let big = block(
            [freq[0] * 5.5, freq[1] * 10.4],
            [freq[0] * 1.6, freq[1] * 3.0, 1.0],
            ctx,
            seed + 7.89,
            1.0,
            1.0,
            5,
            0.56,
            0.66,
            1.3,
            [0.0; 2],
        );
        let med = block(
            [freq[0] * 8.0, freq[1] * 12.0],
            [freq[0] * 2.0, freq[1] * 5.0, 1.0],
            ctx,
            seed + 7.99,
            1.0,
            1.0,
            5,
            0.56,
            0.66,
            1.3,
            [0.0; 2],
        );
        let threshold = p.f("Blocks Threshold") - 0.5;
        let big_th = mix(1.0, 0.2, intensity.clamp(0.0, 1.0)) + threshold;
        let med_th = mix(1.0, 0.3, intensity.clamp(0.0, 1.0)) + threshold;
        let soft = p.f("Blocks Softness");
        let chroma = p.f("Blocks Chroma");
        let gain = p.f("Blocks Intensity");
        let affect = p.f("Blocks Affect Alpha");
        out = image_map(&out, ctx, |_, xy, v| {
            let rgb = rgb_noise(xy);
            let rgb2 = diff_noise(xy, rgb);
            let b = rescale(big.at(xy[0], xy[1]), big_th, big_th + soft);
            let m = rescale(med.at(xy[0], xy[1]), med_th, med_th + soft);
            let a: [f32; 3] = std::array::from_fn(|c| b * mix(1.0, rgb[c], chroma));
            let b: [f32; 3] = std::array::from_fn(|c| m * mix(1.0, rgb2[c], chroma));
            let ga = if crate::colorops::sat_luma(a) > 0.1 {
                blink1
            } else {
                0.0
            };
            let gb = if crate::colorops::sat_luma(b) > 0.1 {
                blink2
            } else {
                0.0
            };
            let rgb: [f32; 3] = std::array::from_fn(|c| (b[c] * gb - a[c] * ga - v[c]).abs());
            [
                mix(v[0], rgb[0], gain),
                mix(v[1], rgb[1], gain),
                mix(v[2], rgb[2], gain),
                1.0 - (1.0 - v[3]) * (1.0 - affect * rgb[0].max(rgb[1]).max(rgb[2])),
            ]
        });
    }
    if p.b("Invert") && gate(p, ctx, "Invert", seed + 0.7, 0.0) > 0.0 {
        let f = p.f("Invert Pattern Freq");
        let bands = block(
            [freq[0] * f * 0.2, freq[1] * f],
            [freq[0] * 0.2, freq[1] * 15.0, 0.0],
            ctx,
            seed + 0.7 + crate::temporal::round_step(ctx.frame_number, 3.0),
            1.0,
            0.0,
            4,
            0.78,
            0.6,
            1.15,
            [0.5; 2],
        );
        let threshold = mix(1.0, p.f("Invert Threshold"), intensity);
        let darken = p.f("Invert Darken");
        out = image_map(&out, ctx, |_, xy, v| {
            if bands.at(xy[0], xy[1]).clamp(0.0, 1.0) > threshold {
                [
                    (1.0 - v[0]) * darken,
                    (1.0 - v[1]) * darken,
                    (1.0 - v[2]) * darken,
                    v[3],
                ]
            } else {
                v
            }
        });
    }
    if p.b("Flow") && gate(p, ctx, "Flow", seed + 0.027, 0.0) > 0.0 {
        let f = p.f("Flow Block Freq");
        let bx = block(
            [f, f],
            [f * 0.2, f * 0.15, 0.0],
            ctx,
            time_freeze + 0.9,
            1.0,
            0.0,
            4,
            0.78,
            0.6,
            1.15,
            [0.5; 2],
        );
        let by = Blocks {
            seed: ((time_freeze + 0.912) * 4096.0) as i32,
            ..bx
        };
        let threshold = mix(1.0, p.f("Flow Threshold"), intensity);
        let amount = p.f("Flow Damage Amount") * intensity;
        let mut damaged = old.clone();
        for v in &mut damaged.data {
            for c in 0..4 {
                v[c] = mix(-amount, 1.0 + amount, v[c]);
            }
            let rgb = sat_scale([v[0], v[1], v[2]], 1.0 + amount);
            for c in 0..3 {
                v[c] = rgb[c].clamp(0.0, 1.0);
            }
            v[3] = v[3].clamp(0.0, 1.0);
        }
        let smp = Sampler::new(&damaged, [Wrap::No; 2]);
        let scale = p.f("Flow Speed") * 0.03 * (ctx.frame_number - old_n as f32);
        out = image_map(&out, ctx, |_, xy, v| {
            let vel = [
                bx.at(xy[0], xy[1]).clamp(-1.0, 1.0),
                by.at(xy[0], xy[1]).clamp(-1.0, 1.0),
            ];
            if vel[0] * vel[0] + vel[1] * vel[1] > threshold * threshold {
                smp.at(&ctx.frame, xy[0] + vel[0] * scale, xy[1] + vel[1] * scale)
            } else {
                v
            }
        });
    }
    out
}
