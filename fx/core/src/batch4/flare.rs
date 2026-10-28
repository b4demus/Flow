//! Recovered flare graph and independently designed procedural lens elements.
use super::{blur, mix, Context, Settings};
use crate::{
    colorops,
    composite::Combine,
    shake::{Sampler, Wrap},
    Edge, Image, Plane,
};
use rayon::prelude::*;
fn animation(distance: f32, width: f32, power: f32) -> f32 {
    if width <= 0.0 {
        0.0
    } else {
        (1.0 - distance / width)
            .max(0.0)
            .powf(1.0 / power.max(0.01))
    }
}
fn zones(point: [f32; 2], pivot: [f32; 2], p: &Settings, ctx: &Context) -> [f32; 2] {
    let x = point[0] * 2.0 / ctx.frame.w - 1.0;
    let y = point[1] * 2.0 / ctx.frame.h - 1.0;
    let aspect = ctx.frame.h / ctx.frame.w;
    let shift = p.f("Shift Out");
    let a = x.abs() - shift;
    let b = aspect * (y.abs() - shift);
    let d = if p.b("One Way") && (a > 1.0 || b > aspect) {
        0.0
    } else if a > 1.0 && b > aspect {
        ((a - 1.0).powi(2) + (b - aspect).powi(2)).sqrt()
    } else {
        (a - 1.0).abs().min((b - aspect).abs())
    };
    let center =
        ((point[0] - pivot[0]).powi(2) + (point[1] - pivot[1]).powi(2)).sqrt() * 2.0 / ctx.frame.w;
    [
        animation(d, p.f("Edge Width"), p.f("Edge Falloff")),
        animation(center, p.f("Center Radius"), p.f("Center Falloff")),
    ]
}
struct Ghost {
    center: [f32; 2],
    radius: f32,
    color: [f32; 3],
    ring: bool,
    aperture: bool,
}
pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let mode = p.mode();
    let bg = crate::temporal::to_working(ctx.background.unwrap_or(src).clone(), mode);
    let hotspot = p.xy("Hotspot XY");
    let pivot = p.xy("Pivot XY");
    let z = zones(hotspot, pivot, p, ctx);
    let mut opacity = 1.0;
    let mut mask_color = [1.0; 3];
    if let Some(mask) = ctx.mask {
        let mut mask_plane = Plane {
            w: mask.w,
            h: mask.h,
            data: mask
                .data
                .iter()
                .map(|v| {
                    let val = match p.f("Occlusion From") as i32 {
                        1 => 0.0,
                        2 => {
                            crate::flicker::mono(*v)
                                * if mode == crate::glow::AlphaMode::Normal {
                                    v[3]
                                } else {
                                    1.0
                                }
                        }
                        _ => v[3],
                    };
                    if p.b("Invert Occlusion") {
                        1.0 - val
                    } else {
                        val
                    }
                })
                .collect(),
        };
        let r = p.f("Occlusion Softness") * ctx.scale[0];
        crate::boxes::box_passes(&mut mask_plane, r, r, Edge::Reflect, 3);
        let image = Image {
            w: mask.w,
            h: mask.h,
            data: mask_plane.data.iter().map(|v| [*v, *v, *v, 1.0]).collect(),
        };
        let smp = Sampler::new(&image, [Wrap::No; 2]);
        opacity = (1.0
            - smp.at_px(
                hotspot[0] - 0.5 - ctx.frame.src_x as f32,
                hotspot[1] - 0.5 - ctx.frame.src_y as f32,
            )[0])
            .clamp(0.0, 1.0);
        if p.b("Use Color") {
            let s = Sampler::new(mask, [Wrap::No; 2]).at_px(
                hotspot[0] - 0.5 - ctx.frame.src_x as f32,
                hotspot[1] - 0.5 - ctx.frame.src_y as f32,
            );
            mask_color = [s[0], s[1], s[2]];
            if s[3] <= 0.0 {
                mask_color = [1.0; 3];
            }
        }
    }
    let width_scale = mix(1.0, p.f("Edge Scale Widths"), z[0])
        * mix(1.0, p.f("Center Scale Widths"), z[1])
        * mix(1.0, opacity, p.f("Occlusion Scale Widths"));
    let brightness_scale = mix(1.0, p.f("Edge Scale Brightness"), z[0])
        * mix(1.0, p.f("Center Scale Brightness"), z[1])
        * mix(1.0, opacity, p.f("Occlusion Scale Brightness"));
    let width = p.f("Scale Widths") * ctx.scale[0] * width_scale;
    let height = p.f("Rel Heights").max(0.001);
    let lens = p.f("Lens") as i32;
    let brightness = p.f("Brightness") * brightness_scale;
    let overall = p.c("Color");
    let hotcolor = p.c("Hotspot Color");
    let othercolor = p.c("Other Color");
    let hotbright = p.f("Hotspot Bright");
    let otherbright = p.f("Other Brightness");
    let raysbright = p.f("Rays Brightness");
    let raylength = p.f("Rays Length").max(0.0);
    let raythickness = p.f("Rays Thickness").max(0.001);
    let count = (p.f("Rays Count") * p.f("Rays Num Scale"))
        .round()
        .clamp(0.0, 512.0);
    let angle = p.f("Rays Rotate") * std::f32::consts::PI / 180.0;
    let ghost_count = p.f("Ghost Count").round().clamp(0.0, 64.0) as usize;
    let spacing = p.f("Ghost Spacing");
    let otherwidth = p.f("Other Width");
    let mut ghosts = Vec::new();
    let axis = [hotspot[0] - pivot[0], hotspot[1] - pivot[1]];
    for i in 0..ghost_count {
        let t = if ghost_count <= 1 {
            0.5
        } else {
            i as f32 / (ghost_count - 1) as f32
        };
        let along = mix(-1.1, 1.3, t) * spacing;
        let hue = (t * 0.7 + 0.2 + (lens - 1) as f32 * 0.11).fract();
        let color = colorops::hsv_to_rgb(hue, 0.3, 1.0);
        ghosts.push(Ghost {
            center: [pivot[0] + axis[0] * along, pivot[1] + axis[1] * along],
            radius: width * otherwidth * (0.025 + 0.065 * ((i * 7 % 11) as f32 / 10.0)),
            color,
            ring: i % 3 == 0,
            aperture: lens == 3 || i % 4 == 1,
        });
    }
    let gamma = p.f("Gamma").max(0.1);
    let saturation = p.f("Saturation");
    let hue = p.f("Hue Shift");
    let atmosphere = p.f("Atmosphere Amp");
    let atfreq = p.f("Atmosphere Freq");
    let atdetail = p.f("Atmosphere Detail");
    let atseed = p.f("Atmosphere Seed");
    let attime = p.f("Atmosphere Speed") * (ctx.time + 1.0) * 0.1;
    let flicker_amp = p.f("Flicker Amp");
    let flicker_noise = crate::shake::noise2(
        hotspot[0] / ctx.frame.w * 5.0 * p.f("Flicker Speed"),
        ctx.time * p.f("Flicker Speed"),
        4,
        (0.234 * 4096.0) as i32,
    );
    let flicker = 1.0
        - flicker_amp
            * (1.0 - flicker_noise).powf(1.0 / (1.0 - p.f("Flicker Randomness")).max(0.001));
    let mut flare = Image::new(src.w, src.h);
    flare
        .data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let gx = x as f32 + 0.5 + ctx.frame.src_x as f32;
                let gy = y as f32 + 0.5 + ctx.frame.src_y as f32;
                let dx = gx - hotspot[0];
                let dy = (gy - hotspot[1]) / height;
                let r2 = dx * dx + dy * dy;
                let radius = (width * 0.1).max(0.001);
                let halo = (-r2 / (radius * radius)).exp() * 0.55;
                let core = (-r2 / (radius * radius * 0.025)).exp() * 1.4;
                let mut rgb: [f32; 3] =
                    std::array::from_fn(|c| (halo + core) * hotbright * hotcolor[c]);
                if count > 0.0 && raylength > 0.0 {
                    let theta = dy.atan2(dx) - angle;
                    let radius = r2.sqrt();
                    let power = (24.0 / raythickness).max(0.1);
                    let lobes = ((theta * count * 0.5).cos().abs()).powf(power);
                    let star = lobes
                        * (-radius / (width * 0.4 * raylength).max(0.001)).exp()
                        * raysbright
                        * 0.35;
                    for c in 0..3 {
                        rgb[c] += star * hotcolor[c];
                    }
                }
                if lens == 2 {
                    let streak = (-dy * dy / (width * 0.0015 * raythickness).max(0.1).powi(2))
                        .exp()
                        * (-dx.abs() / (width * raylength).max(0.1)).exp()
                        * 0.45
                        * raysbright;
                    rgb[0] += streak * 0.4;
                    rgb[1] += streak * 0.65;
                    rgb[2] += streak;
                }
                for ghost in &ghosts {
                    let u = gx - ghost.center[0];
                    let v = (gy - ghost.center[1]) / height;
                    let rad = ghost.radius.max(0.01);
                    let rr = (u * u + v * v).sqrt() / rad;
                    let boundary = if ghost.aperture {
                        let theta = v.atan2(u);
                        let sector = (theta + std::f32::consts::PI / 6.0)
                            .rem_euclid(std::f32::consts::PI / 3.0)
                            - std::f32::consts::PI / 6.0;
                        rr * sector.cos() / (std::f32::consts::PI / 6.0).cos()
                    } else {
                        rr
                    };
                    let strength = if ghost.ring {
                        (-((rr - 1.0) * 18.0).powi(2)).exp() * 0.2
                    } else {
                        ((1.0 - boundary) * 12.0).clamp(0.0, 1.0) * 0.06
                    };
                    for c in 0..3 {
                        rgb[c] += strength * ghost.color[c] * otherbright * othercolor[c];
                    }
                }
                let nx = (gx / ctx.frame.w - 0.5) * 2.0;
                let ny = (ctx.frame.h * 0.5 - gy) * 2.0 / ctx.frame.w;
                let noise = if atmosphere != 0.0 {
                    1.0 - super::noise::atmosphere(
                        nx * atfreq,
                        ny * atfreq,
                        attime,
                        atseed,
                        atdetail,
                    ) * atmosphere
                } else {
                    1.0
                };
                for c in 0..3 {
                    rgb[c] = (rgb[c] * brightness * overall[c] * mask_color[c] * flicker * noise)
                        .max(0.0)
                        .powf(1.0 / gamma);
                }
                rgb = colorops::sat_scale(rgb, saturation);
                if hue != 0.0 {
                    let l = colorops::sat_luma(rgb);
                    let shifted = colorops::hue_shift(rgb, hue);
                    let l2 = colorops::sat_luma(shifted);
                    rgb = if l2 != 0.0 {
                        shifted.map(|v| v * l / l2)
                    } else {
                        shifted
                    };
                }
                let diffraction = p.f("Diffraction Glow");
                if diffraction != 0.0 && ctx.mask.is_some() {
                    let gr = (p.f("Glow Radius") * ctx.scale[0]).max(0.1);
                    let spread = (p.f("Glow Width") * ctx.scale[0]).max(0.1);
                    let glow = (-((r2.sqrt() - gr) / spread).powi(2)).exp()
                        * diffraction
                        * (1.0 - opacity)
                        * brightness;
                    let color = p.c("Glow Color");
                    for c in 0..3 {
                        rgb[c] += glow * color[c];
                    }
                }
                *o = [rgb[0], rgb[1], rgb[2], 0.0];
            }
        });
    let radius = p.f("Blur Flare") * ctx.scale[0];
    flare = blur(&flare, [radius, radius], 3);
    let combine = p.f("Combine") as i32;
    let bgbright = p.f("Bg Brightness");
    let affect = p.f("Affect Alpha");
    let tint = p.b("Tint Bg Whites");
    flare.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        let mut f = *v;
        f[3] = (f[0].max(f[1]).max(f[2]) * affect).clamp(0.0, 1.0);
        let b = bg.data[i];
        let b = [b[0] * bgbright, b[1] * bgbright, b[2] * bgbright, b[3]];
        let mut o = if combine == 3 {
            f
        } else {
            std::array::from_fn(|c| {
                if combine == 2 {
                    b[c] + f[c]
                } else {
                    Combine::Screen.apply(b[c], f[c])
                }
            })
        };
        if tint && combine != 3 {
            let l = colorops::sat_luma([f[0].min(1.0), f[1].min(1.0), f[2].min(1.0)]);
            for c in 0..3 {
                let base = if combine == 2 {
                    b[c] + l
                } else {
                    Combine::Screen.apply(b[c], l)
                };
                o[c] = base.min(1.0) + f[c] - l;
            }
        }
        if p.b("Show Edge Zones") || p.b("Show Center Zone") {
            let point = [
                (i % src.w) as f32 + 0.5 + ctx.frame.src_x as f32,
                (i / src.w) as f32 + 0.5 + ctx.frame.src_y as f32,
            ];
            let z = zones(point, pivot, p, ctx);
            let z = Combine::Screen.apply(
                if p.b("Show Edge Zones") { z[0] } else { 0.0 },
                if p.b("Show Center Zone") { z[1] } else { 0.0 },
            );
            for c in 0..3 {
                o[c] = Combine::Screen.apply(f[c], z);
            }
            o[3] = 1.0;
        }
        o[3] = o[3].clamp(0.0, 1.0);
        *v = o;
    });
    crate::temporal::from_working(flare, mode)
}
