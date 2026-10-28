//! Film pipeline and independently generated dirt/stamp shapes.
use super::{blur, mix, Context, Settings};
use crate::{
    colorops::sat_scale,
    shake::{pixel_to_norm, Sampler, Wrap},
    Edge, Image, Plane,
};
use rayon::prelude::*;
fn random(seed: f32, index: f32) -> f32 {
    crate::temporal::random(seed + index * 0.73147)
}

pub fn vignette(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let darkness = p.f("Vignette Darkness");
    if darkness == 0.0 {
        return src.clone();
    }
    let r = p.f("Vignette Radius");
    let soft = p.f("Vignette Edge Softness");
    let height = p.f("Vignette Rel Height").max(0.001);
    let mut out = src.clone();
    out.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        let (x, y) = pixel_to_norm(
            &ctx.frame,
            (i % src.w) as f32 + ctx.frame.src_x as f32,
            (i / src.w) as f32 + ctx.frame.src_y as f32,
        );
        let linear = ((x * x + (y / height).powi(2)).sqrt() - (r - soft * 0.5)) / soft.max(0.001);
        let a = linear.clamp(0.0, 1.0);
        let smooth = (1.0 - (a * std::f32::consts::PI).cos()) * 0.5;
        let g = mix(
            linear,
            if linear > 0.5 {
                linear.max(smooth)
            } else {
                linear.min(smooth)
            },
            0.4,
        );
        // Static dithering is independently generated; it is not a copied image.
        let d = (random(0.2345, i as f32) - 0.5) / 256.0;
        let tex = (1.0 - g - d).clamp(0.0, 1.0);
        let k = mix(1.0, tex, darkness);
        for c in 0..3 {
            v[c] *= k;
        }
    });
    out
}

/// Native uniform-byte grain with a 65-entry luminance envelope. The envelope
/// is calculated, not copied from a film stock or colour LUT.
pub fn grain(src: &Image, p: &Settings, ctx: &Context, damage: bool) -> Image {
    let amp = p.f("Grain Amp")
        * if damage {
            1.0
        } else {
            0.02 * ((p.f("Neg Film") > 1.0) as i32 + (p.f("Print Film") > 1.0) as i32) as f32
        };
    if amp <= 0.0 {
        return src.clone();
    }
    let rgb = [
        p.f("Grain Amp Red"),
        p.f("Grain Amp Green"),
        p.f("Grain Amp Blue"),
    ];
    let dark = p.f("Grain Amp Darks");
    let bright = p.f("Grain Amp Brights");
    let mid = if damage {
        // FilmDamage's actual native envelope has its peak at half intensity.
        // Verified per level in AE; it differs from the embedded graph default.
        [0.5; 3]
    } else {
        [
            p.f("Midtone Pos Red"),
            p.f("Midtone Pos Green"),
            p.f("Midtone Pos Blue"),
        ]
    };
    let mut envelopes = [[0.0f32; 65]; 3];
    for c in 0..3 {
        for i in 0..65 {
            let x = i as f32 / 64.0;
            let (q, base) = if x < mid[c] {
                (x * (1.0 / mid[c].max(0.0001)), dark)
            } else {
                ((1.0 - x) * (1.0 / (1.0 - mid[c]).max(0.0001)), bright)
            };
            envelopes[c][i] = ((q * q - q * q * q + q) * (1.0 - base) + base) * amp * rgb[c];
        }
    }
    let seed = if damage {
        p.f("Seed") + 0.123
    } else {
        p.f("Grain Seed")
    };
    let seed = (seed * 4096.0) as i32;
    let h = ctx.frame.h as i32;
    let hold = p.f("Grain Hold") as i32 - 1;
    let n = ctx.frame_number.floor() as i32;
    let mut fields = [n * 2, n * 2];
    if hold == 0 {
        fields[1] += 1;
    } else if hold >= 2 {
        let f = ((n - (hold - 2)) + 7) * 2;
        fields = [
            (f / 5) * 2 + if f % 5 > 1 { 1 } else { 0 },
            ((f + 1) / 5) * 2 + if (f + 1) % 5 > 1 { 1 } else { 0 },
        ];
        if h & 1 == 0 {
            fields.swap(0, 1);
        }
        if p.f("Field Dominance") == 2.0 {
            fields.swap(0, 1);
        }
    }
    let w = ctx.frame.w as i32;
    let pixels = (src.w as i32).wrapping_mul(src.h as i32);
    if (h == 576) != (h & 1 != 0) {
        fields.swap(0, 1);
    }
    // UI pixels -> normalised coordinates * 4 -> width * 1/8 cancel.
    let blur_amp = p.f("Grain Blur") * ctx.scale[0];
    let rel = [
        p.f("Grain Blur Red"),
        p.f("Grain Blur Green"),
        p.f("Grain Blur Blue"),
    ];
    let mono = p.b("Grain Mono");
    let mut noise: Vec<Plane> = (0..3)
        .map(|c| {
            let channel = if mono { 1 } else { c };
            let base_seed = seed.wrapping_add(channel as i32 * 10001);
            let data = (0..src.data.len())
                .into_par_iter()
                .map(|i| {
                    let x = i % src.w;
                    let y = i / src.w;
                    let gx = x as i32 + ctx.frame.src_x;
                    let gy = y as i32 + ctx.frame.src_y;
                    let f = fields[(gy & 1) as usize];
                    let s = base_seed
                        .wrapping_add((pixels + 7).wrapping_mul(f))
                        .wrapping_add(w.wrapping_mul(gy))
                        .wrapping_add(gx & !3);
                    let bits = crate::shake::hash4(s as u32);
                    let byte = ((bits >> ((gx & 3) * 8)) & 255) as i32;
                    let mut sample = byte * 16;
                    let radius = blur_amp * rel[c];
                    if fields[0] != fields[1] && radius > 0.0 {
                        let vertical = (radius - 0.5).max(0.0);
                        let blend = ((vertical * 4096.0).round() as i32).min(4096) >> 1;
                        let adjacent = |offset: i32| {
                            let h = crate::shake::hash4(s.wrapping_add(offset) as u32);
                            ((h >> ((gx & 3) * 8)) & 255) as i32
                        };
                        let neighbors = adjacent(-w) + adjacent(w);
                        sample += ((neighbors * 8 - sample) * blend) >> 12;
                    }
                    let idx = (((src.data[i][c] * 4096.0) as i32) >> 6).clamp(0, 64) as usize;
                    (sample - 2048) as f32 * (1.0 / 4096.0) * envelopes[c][idx]
                })
                .collect();
            Plane {
                w: src.w,
                h: src.h,
                data,
            }
        })
        .collect();
    for c in 0..3 {
        if blur_amp * rel[c] > 0.0 {
            let radius = blur_amp * rel[c];
            let vertical = if fields[0] == fields[1] {
                radius
            } else {
                (radius - 0.5).max(0.0)
            };
            if fields[0] == fields[1] {
                crate::boxes::box_passes(&mut noise[c], radius, vertical, Edge::Transparent, 2);
            } else {
                crate::boxes::box_passes(&mut noise[c], radius, 0.0, Edge::Transparent, 2);
                for parity in 0..2 {
                    let rows = (src.h + 1 - parity) / 2;
                    if rows == 0 {
                        continue;
                    }
                    let mut field = Plane {
                        w: src.w,
                        h: rows,
                        data: (0..rows)
                            .flat_map(|y| {
                                let start = (y * 2 + parity) * src.w;
                                noise[c].data[start..start + src.w].iter().copied()
                            })
                            .collect(),
                    };
                    crate::boxes::box_passes(&mut field, 0.0, vertical * 0.5, Edge::Transparent, 2);
                    for y in 0..rows {
                        let start = (y * 2 + parity) * src.w;
                        noise[c].data[start..start + src.w]
                            .copy_from_slice(&field.data[y * src.w..(y + 1) * src.w]);
                    }
                }
            }
            for sample in &mut noise[c].data {
                *sample *= 1.0 + radius * 0.8;
            }
        }
    }
    let mut out = src.clone();
    out.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        for c in 0..3 {
            v[c] += noise[c].data[i];
        }
    });
    out
}

#[derive(Clone)]
struct Spot {
    center: [f32; 2],
    radius: [f32; 2],
    angle: f32,
    color: [f32; 3],
    opacity: f32,
    shape: f32,
}
fn spots(
    src: &Image,
    p: &Settings,
    ctx: &Context,
    label: &str,
    seed: f32,
    negative: bool,
) -> Image {
    let density = p.f(&format!("{label} Density"))
        * p.f(if label == "Stain" {
            if negative {
                "Stain Negative"
            } else {
                "Stain Print"
            }
        } else if negative {
            "Dust On Negative"
        } else {
            "Dust On Print"
        });
    if density <= 0.0 {
        return src.clone();
    }
    let frame = ctx.frame_number.floor();
    let color1 = p.c(&format!("{label} Color1"));
    let color2 = p.c(&format!("{label} Color2"));
    let size = p.f(&format!("{label} Size"));
    let vary_size = p.f(&format!("Vary {label} Size"));
    let opacity = p.f(&format!("{label} Opacity"));
    let vary_opacity = p.f(&format!("Vary {label} Opacity"));
    let vary_density = p.f(&format!("Vary {label} Density"));
    let vary_bright = p.f(&format!("Vary {label} Brightness"));
    let vary_color = p.f(&format!("Vary {label} Color"));
    let density =
        density * (1.0 + (random(seed, frame * 19.0) - 0.5) * 2.0 * vary_density).max(0.0);
    let count = density * (if label == "Stain" { 5.0 } else { 1.0 });
    let n = (count.floor() as usize
        + if random(seed + 0.37, frame * 11.0) < count.fract() {
            1
        } else {
            0
        })
    .min(2000);
    let mut shapes = Vec::with_capacity(n);
    for i in 0..n {
        let j = i as f32 + frame * 101.0;
        let radius = size * ctx.frame.w * if label == "Stain" { 0.045 } else { 0.003 };
        let radius = radius * (1.0 + (2.0 * random(seed + 0.01, j) - 1.0) * vary_size).max(0.03);
        let t = random(seed + 0.02, j);
        let color: [f32; 3] = std::array::from_fn(|c| {
            let v = mix(color1[c], color2[c], t * vary_color);
            let v = v * (1.0 + (2.0 * random(seed + c as f32 + 0.9, j) - 1.0) * vary_bright);
            if negative {
                1.0 - v
            } else {
                v
            }
        });
        shapes.push(Spot {
            center: [
                random(seed + 0.03, j) * ctx.frame.w,
                random(seed + 0.04, j) * ctx.frame.h,
            ],
            radius: [radius, radius * (0.4 + random(seed + 0.05, j))],
            angle: random(seed + 0.06, j) * std::f32::consts::TAU,
            color,
            opacity: opacity * (1.0 + (2.0 * random(seed + 0.07, j) - 1.0) * vary_opacity),
            shape: random(seed + 0.08, j) * std::f32::consts::TAU,
        });
    }
    let flip = p.b("Flip Stamps Vertically");
    let mut out = src.clone();
    out.data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            let gy = y as f32 + ctx.frame.src_y as f32;
            let gy = if flip { ctx.frame.h - 1.0 - gy } else { gy };
            for (x, v) in row.iter_mut().enumerate() {
                let gx = x as f32 + ctx.frame.src_x as f32;
                for s in &shapes {
                    if (gx - s.center[0]).abs() > s.radius[0].max(s.radius[1]) * 1.4
                        || (gy - s.center[1]).abs() > s.radius[0].max(s.radius[1]) * 1.4
                    {
                        continue;
                    }
                    let (sin, cos) = s.angle.sin_cos();
                    let dx = gx - s.center[0];
                    let dy = gy - s.center[1];
                    let u = (dx * cos + dy * sin) / s.radius[0].max(0.05);
                    let w = (-dx * sin + dy * cos) / s.radius[1].max(0.05);
                    let theta = w.atan2(u);
                    let r = (u * u + w * w).sqrt();
                    let contour = 1.0
                        + 0.15 * (theta * 3.0 + s.shape).sin()
                        + 0.07 * (theta * 7.0 - s.shape).sin();
                    let feather = if label == "Stain" { 0.35 } else { 0.4 };
                    let a = ((contour - r) / feather).clamp(0.0, 1.0) * s.opacity;
                    for c in 0..3 {
                        v[c] = mix(v[c], s.color[c], a);
                    }
                }
            }
        });
    out
}

fn hairs(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let count = p.f("Hairs");
    if count <= 0.0 {
        return src.clone();
    }
    let epoch = (ctx.frame_number / p.f("Hair Persistence").max(0.1)).floor();
    let seed = p.f("Seed") + 0.4 + epoch;
    let mut out = src.clone();
    let color = p.c("Hair Color");
    let opacity = p.f("Hair Opacity");
    let size = p.f("Hair Size");
    let vary = p.f("Vary Hair Size");
    for i in 0..(count.ceil() as usize).min(200) {
        let i = i as f32;
        if i >= count.floor() && random(seed, i) > count.fract() {
            continue;
        }
        let center = [
            random(seed + 0.1, i) * ctx.frame.w,
            random(seed + 0.2, i) * ctx.frame.h,
        ];
        let angle = random(seed + 0.3, i) * std::f32::consts::TAU;
        let length = size
            * (1.0 + (random(seed + 0.4, i) * 2.0 - 1.0) * vary).max(0.05)
            * ctx.frame.w
            * 0.08;
        let bend = (random(seed + 0.5, i) - 0.5) * length;
        let (sin, cos) = angle.sin_cos();
        let mut prev = center;
        for step in 1..=40 {
            let t = step as f32 / 40.0;
            let wiggle = p.f("Hair Wiggle Amp")
                * (crate::shake::noise2(
                    t * 4.0,
                    ctx.time * p.f("Hair Wiggle Freq"),
                    1,
                    ((seed + i) * 4096.0) as i32,
                ) - 0.5)
                * length;
            let u = (t - 0.5) * length;
            let v = 4.0 * t * (1.0 - t) * bend + wiggle;
            let mut point = [center[0] + u * cos - v * sin, center[1] + u * sin + v * cos];
            if p.b("Flip Stamps Vertically") {
                point[1] = ctx.frame.h - point[1];
            }
            if step > 1 {
                stroke(
                    &mut out,
                    prev,
                    point,
                    (ctx.frame.w / 1920.0).max(0.5) * size,
                    opacity,
                    color,
                    &ctx.frame,
                );
            }
            prev = point;
        }
    }
    out
}
fn stroke(
    out: &mut Image,
    a: [f32; 2],
    b: [f32; 2],
    width: f32,
    opacity: f32,
    color: [f32; 3],
    fr: &crate::shake::Frame,
) {
    let lo = [
        (a[0].min(b[0]) - width - 1.0).floor() as i32 - fr.src_x,
        (a[1].min(b[1]) - width - 1.0).floor() as i32 - fr.src_y,
    ];
    let hi = [
        (a[0].max(b[0]) + width + 1.0).ceil() as i32 - fr.src_x,
        (a[1].max(b[1]) + width + 1.0).ceil() as i32 - fr.src_y,
    ];
    let d = [b[0] - a[0], b[1] - a[1]];
    let denom = (d[0] * d[0] + d[1] * d[1]).max(1e-6);
    for y in lo[1].max(0)..hi[1].min(out.h as i32) {
        for x in lo[0].max(0)..hi[0].min(out.w as i32) {
            let p = [
                x as f32 + fr.src_x as f32 - a[0],
                y as f32 + fr.src_y as f32 - a[1],
            ];
            let t = ((p[0] * d[0] + p[1] * d[1]) / denom).clamp(0.0, 1.0);
            let dist = ((p[0] - d[0] * t).powi(2) + (p[1] - d[1] * t).powi(2)).sqrt();
            let alpha = (width + 0.5 - dist).clamp(0.0, 1.0) * opacity;
            for c in 0..3 {
                let v = &mut out.data[y as usize * out.w + x as usize][c];
                *v = mix(*v, color[c], alpha);
            }
        }
    }
}

pub fn damage(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let mut out = grain(src, p, ctx, true);
    let sat = p.f("Saturation");
    let lights = p.c("Tint Lights");
    let darks = p.c("Tint Darks");
    let scale = p.f("Scale Lights");
    let offset = p.f("Offset Darks");
    out.data.par_iter_mut().for_each(|v| {
        let rgb = sat_scale([v[0], v[1], v[2]], sat);
        for c in 0..3 {
            let lo = darks[c] + offset;
            v[c] = rgb[c] * (lights[c] * scale - lo).max(0.0) + lo;
        }
    });
    out = spots(&out, p, ctx, "Stain", p.f("Seed"), true);
    out = spots(&out, p, ctx, "Dust", p.f("Seed") + 0.5, true);
    out = super::scratches::render(&out, p, ctx);
    out = spots(&out, p, ctx, "Stain", p.f("Seed") + 0.1, false);
    out = spots(&out, p, ctx, "Dust", p.f("Seed") + 0.6, false);
    let freq = p.f("Shake Frequency") / 4.0;
    let t = (ctx.frame_number - p.f("Shake Time Offset")) * 30.0 / ctx.fps.max(1.0);
    let seed = p.f("Seed") + 0.3;
    let signal = if p.f("Shake Always") > 0.0 {
        crate::shake::projector_signal(
            seed,
            t,
            freq,
            p.f("Shake Random"),
            p.f("Shake Jumpiness"),
            1.2 / (p.f("Shake Always") + 0.2) - 1.0,
        )
    } else {
        [0.0; 2]
    };
    let aspect = ctx.frame.h / ctx.frame.w;
    let shift = signal[0] * p.f("Shake Amplitude") * 2.0 * aspect;
    let slope = (signal[0] - signal[1]).abs() * p.f("Shake Amplitude") * ctx.frame.h;
    if shift != 0.0 {
        let smp = Sampler::new(&out, [Wrap::No; 2]);
        let period = (1.0 + p.f("Interframe Border Height")) * 2.0 * aspect;
        let mut shifted = out.clone();
        shifted.data.par_iter_mut().enumerate().for_each(|(i, v)| {
            let x = (i % out.w) as f32;
            let y = (i / out.w) as f32;
            let n = pixel_to_norm(
                &ctx.frame,
                x + ctx.frame.src_x as f32,
                y + ctx.frame.src_y as f32,
            );
            let sy = (n.1 + shift + aspect).rem_euclid(period) - aspect;
            *v = smp.at(&ctx.frame, n.0, sy);
        });
        out = shifted;
    }
    let noise = |seed: f32, freq: f32| {
        2.0 * (crate::shake::noise2(ctx.time * freq * 2.0, 0.0, 1, (seed * 4096.0) as i32) - 0.5)
    };
    let flicker = p.f("Flicker Rand Amp") * noise(p.f("Seed"), p.f("Flicker Rand Freq"))
        + p.f("Flicker Wave Amp")
            * ((ctx.time * p.f("Flicker Wave Freq") - 0.25) * std::f32::consts::TAU).sin();
    let focus = p.f("Defocus Rand Amp") * noise(p.f("Seed") + 0.2, p.f("Defocus Rand Freq"))
        + p.f("Defocus Wave Amp")
            * ((ctx.time * p.f("Defocus Wave Freq") - 1.25) * std::f32::consts::TAU).sin();
    let radius = (p.f("Defocus") * focus * ctx.frame.w * 0.5).max(0.0);
    out = blur(
        &out,
        [radius, (radius + p.f("Shake Motion Blur") * slope).max(0.0)],
        3,
    );
    let gain = 1.0 + p.f("Flicker") * flicker;
    for v in &mut out.data {
        for c in 0..3 {
            v[c] *= gain;
        }
    }
    out = vignette(&out, p, ctx);
    let mut out = hairs(&out, p, ctx);
    // FilmDamage's graph is RGB-only: the host retains the input matte even
    // when projector weave leaves black film borders in the colour channels.
    for (v, s) in out.data.iter_mut().zip(&src.data) {
        v[3] = s[3];
    }
    out
}

/// Own analytic colour profiles. Sapphire stock tables are artistic resources
/// and are intentionally not read or embedded. IDs select independent curves.
fn stock_curve(v: f32, stock: usize, print: bool, c: usize) -> f32 {
    if stock <= 1 {
        return v;
    }
    let neg_contrast = [
        1.08, 1.03, 1.16, 1.12, 0.92, 1.2, 1.24, 1.1, 1.06, 1.18, 0.96, 1.1, 1.13,
    ];
    let print_contrast = [1.13, 1.2, 1.09, 1.16, 1.28, 1.22];
    let index = stock - 2;
    let contrast = if print {
        print_contrast[index % 6]
    } else {
        neg_contrast[index % 13]
    };
    let warmth = if c == 0 {
        0.012
    } else if c == 2 {
        -0.012
    } else {
        0.0
    };
    let warmth = warmth * ((index % 4) as f32 - 1.0);
    let x = v.max(0.0);
    let shoulder = if print { 0.08 } else { 0.04 };
    let base = ((x + 1e-6) / 0.18).powf(contrast) * 0.18;
    (base * (1.0 + shoulder) / (1.0 + base * shoulder) + warmth * x.sqrt()).max(0.0)
}
fn fields(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let mode = p.f("Fields") as i32;
    if mode <= 1 {
        return src.clone();
    }
    let mut out = src.clone();
    let parity = if mode == 2 { 1 } else { 0 };
    let old = ctx.previous.unwrap_or(src);
    let index = (ctx.frame_number.round() as i32 + 11 - (mode - 1)).rem_euclid(5);
    for y in 0..src.h {
        for x in 0..src.w {
            let q = if mode >= 5 {
                let upper = (y & 1) == if p.f("Field Dominance") == 2.0 { 1 } else { 0 };
                let use_previous = if upper { index > 3 } else { index > 1 };
                let frame = if use_previous { old } else { src };
                let mut v = frame.data[y * src.w + x];
                if !std::ptr::eq(frame, src) && p.mode() == crate::glow::AlphaMode::Normal {
                    for c in 0..3 {
                        v[c] *= v[3];
                    }
                }
                v
            } else if mode == 4 {
                let a = if y > 0 { y - 1 } else { (y + 1).min(src.h - 1) };
                let b = if y + 1 < src.h {
                    y + 1
                } else {
                    y.saturating_sub(1)
                };
                std::array::from_fn(|c| {
                    src.data[y * src.w + x][c] * 0.5
                        + (src.data[a * src.w + x][c] + src.data[b * src.w + x][c]) * 0.25
                })
            } else if y & 1 != parity {
                let a = if y > 0 { y - 1 } else { (y + 1).min(src.h - 1) };
                let b = if y + 1 < src.h {
                    y + 1
                } else {
                    y.saturating_sub(1)
                };
                std::array::from_fn(|c| {
                    (src.data[a * src.w + x][c] + src.data[b * src.w + x][c]) * 0.5
                })
            } else {
                src.data[y * src.w + x]
            };
            out.data[y * src.w + x] = q;
        }
    }
    out
}
pub fn effect(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let mut out = fields(src, p, ctx);
    out = blur(
        &out,
        [
            p.f("Blur Input") * ctx.scale[0],
            p.f("Blur Input") * ctx.scale[1],
        ],
        3,
    );
    let neg = p.f("Neg Film") as usize;
    let print = p.f("Print Film") as usize;
    let cc = p.f("Scale CC");
    let gi = p.f("Input Gamma").max(0.01);
    let go = p.f("Output Gamma").max(0.01);
    let exposure = p.f("Neg Exposure");
    let print_exp = p.f("Print Exposure");
    let lights = [
        p.f("Print Lights Red"),
        p.f("Print Lights Green"),
        p.f("Print Lights Blue"),
    ];
    out.data.par_iter_mut().for_each(|v| {
        for c in 0..3 {
            let linear = v[c].max(0.0).powf(gi)
                * 2.0f32.powf(if neg > 1 || print > 1 { exposure } else { 0.0 });
            let n = stock_curve(linear, neg, false, c);
            let q = n * 2.0f32.powf(print_exp + (lights[c] - 25.0) / 12.0);
            let q = stock_curve(q, print, true, c).max(0.0).powf(1.0 / go);
            v[c] = mix(v[c], q, cc);
        }
    });
    let gain = p.f("Glow Brightness");
    let soft = p.f("Glow Soft Focus");
    if gain != 0.0 || soft != 0.0 {
        let mut blurred = out.clone();
        let rel = [
            p.f("Glow Width Red"),
            p.f("Glow Width Green"),
            p.f("Glow Width Blue"),
        ];
        for c in 0..3 {
            let mut pl = out.channel(c);
            let r = p.f("Glow Width") * ctx.scale[0] * rel[c];
            crate::boxes::box_passes(&mut pl, r, r, Edge::Reflect, 3);
            blurred.set_channel(c, &pl);
        }
        out.data.par_iter_mut().enumerate().for_each(|(i, v)| {
            for c in 0..3 {
                let b = blurred.data[i][c];
                v[c] = v[c] * (1.0 - soft) + b * (gain + soft) - v[c] * b * gain;
            }
        });
    }
    out = grain(&out, p, ctx, false);
    let bright = p.f("Scale Brights");
    let dark = p.f("Offset Darks");
    for v in &mut out.data {
        for c in 0..3 {
            v[c] = v[c] * (bright - dark).max(0.0) + dark;
        }
    }
    vignette(&out, p, ctx)
}
