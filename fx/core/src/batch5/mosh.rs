use super::{flow, Context, Settings};
use crate::{
    composite::Combine,
    temporal::{random, round_step},
    Image,
};
use rayon::prelude::*;
fn holds(p: &Settings, n: f32) -> (f32, f32, f32) {
    if p.f("Vary Hold Frame") == 2.0 {
        return (p.f("Flow Warp Hold Frame"), p.f("Pulse Hold Frame"), 50.0);
    }
    let interval = p.f("Frame Interval");
    let id = if interval != 0.0 {
        round_step(n / interval, 1.0)
    } else {
        0.0
    };
    let r = 2.0 * (0.5 - random(p.f("Seed") + id));
    let flow_step = interval + round_step(r * p.f("Vary Frame Interval") * interval, 1.0);
    let pulse_step = p.f("Pulse Interval")
        + round_step(r * p.f("Vary Pulse Interval") * p.f("Pulse Interval"), 1.0);
    (
        if interval > 0.0 {
            round_step(n, flow_step)
        } else {
            n
        },
        if p.f("Pulse Interval") > 0.0 {
            round_step(n, pulse_step)
        } else {
            n
        },
        pulse_step,
    )
}
pub fn needs(p: &Settings, n: f32, total: f32) -> Vec<i32> {
    let (f, q, _) = holds(p, n);
    let mut out = vec![];
    if p.b("Use Flow Warp") {
        out.extend(crate::temporal::frames_for(f, true, total));
        out.extend(crate::temporal::frames_for(f - 1.0, true, total));
    }
    if p.b("Add Pulse Frame") {
        out.extend(crate::temporal::frames_for(q, true, total));
        out.extend(crate::temporal::frames_for(q - 1.0, true, total));
    }
    out.sort_unstable();
    out.dedup();
    out
}
fn threshold_abs(d: f32, t: f32) -> f32 {
    let gain = 1.0 / (1.0 - t);
    // The native CPU primitive truncates this gain to a signed integer.
    let gain = if !gain.is_finite() || gain >= 2147483648.0 || gain < -2147483648.0 {
        i32::MIN
    } else {
        gain as i32
    } as f32;
    if d > t {
        (d - t) * gain
    } else if d < -t {
        (d + t) * gain
    } else {
        0.0
    }
}
fn get(ctx: &Context, p: &Settings, src: &Image, n: f32) -> Image {
    let n = n.clamp(0.0, (ctx.total_frames - 1.0).max(0.0));
    let a = n.floor() as i32;
    let f = |k| {
        ctx.frames
            .iter()
            .find(|(v, _)| *v == k)
            .map(|(_, img)| img)
            .unwrap_or(src)
    };
    let mut out = crate::temporal::to_working(f(a).clone(), p.mode());
    if n.fract() != 0.0 {
        let b = crate::temporal::to_working(f(a + 1).clone(), p.mode());
        for (i, v) in out.data.iter_mut().enumerate() {
            for c in 0..4 {
                v[c] += (b.data[i][c] - v[c]) * n.fract();
            }
        }
    }
    out
}
pub(super) fn sample(img: &Image, x: f32, y: f32, black: bool, quant: bool) -> [f32; 4] {
    if !x.is_finite() || !y.is_finite() {
        return [0.0; 4];
    }
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (mut dx, mut dy) = (x - ix as f32, y - iy as f32);
    if quant {
        dx = (dx * 256.0).round() / 256.0;
        dy = (dy * 256.0).round() / 256.0;
    }
    let at = |x: i32, y: i32| {
        if black && (x < 0 || y < 0 || x >= img.w as i32 || y >= img.h as i32) {
            [0.0; 4]
        } else {
            img.data[y.clamp(0, img.h as i32 - 1) as usize * img.w
                + x.clamp(0, img.w as i32 - 1) as usize]
        }
    };
    let (a, b, c, d) = (
        at(ix, iy),
        at(ix + 1, iy),
        at(ix, iy + 1),
        at(ix + 1, iy + 1),
    );
    [0, 1, 2, 3].map(|k| {
        let top = (b[k] - a[k]).mul_add(dx, a[k]);
        let bot = (d[k] - c[k]).mul_add(dx, c[k]);
        (bot - top).mul_add(dy, top)
    })
}
fn block(src: &Image, bx: f32, by: f32) -> Image {
    let mut out = src.clone();
    let width = src.w as f32;
    let step = 2.0 / width;
    let freq = [1.0 / (0.5 * (width / bx)), (by / bx) / (0.5 * (width / bx))];
    out.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        // Preserve the graph's normalized, y-up coordinates and fused xy
        // generation: integer-space simplification shifts odd-size boundaries.
        let xy = [
            (i.rem_euclid(src.w) as f32).mul_add(step, -1.0 + 1.0 / width),
            (-(i.div_euclid(src.w) as f32)).mul_add(step, (src.h as f32 - 1.0) / width),
        ];
        let snapped = [0, 1].map(|k| {
            if freq[k].is_finite() && freq[k] > 0.0 {
                round_step(xy[k] + freq[k] * 0.5, freq[k])
            } else { xy[k] }
        });
        let pos = [
            (snapped[0] + 1.0) * (width * 0.5) - 0.5,
            (src.h as f32 / width - snapped[1]) * (width * 0.5) - 0.5,
        ];
        *v = sample(src, pos[0], pos[1], true, true);
        v[3] = 1.0;
    });
    out
}
fn unfiltered(src: &Image, x: f32, y: f32) -> [f32; 4] {
    let (x, y) = ((x + 0.5).floor() as i32, (y + 0.5).floor() as i32);
    if x < 0 || y < 0 || x >= src.w as i32 || y >= src.h as i32 {
        [0.0; 4]
    } else {
        src.at(x as usize, y as usize)
    }
}
fn combine(fg: &Image, bg: &Image, intensity: f32, brightness: f32, mode: i32) -> Image {
    let mut out = bg.clone();
    for (i, v) in out.data.iter_mut().enumerate() {
        for c in 0..4 {
            let (a, b) = (
                fg.data[i][c] * intensity,
                bg.data[i][c] * if c < 3 { brightness } else { 1.0 },
            );
            v[c] = match mode {
                2 => Combine::Screen.apply(b, a),
                3 => b + (fg.data[i][c] - b) * intensity,
                4 => {
                    if b > 0.5 {
                        if a > 1.0 || b > 1.0 {
                            a.max(b)
                        } else {
                            2.0 * a.mul_add(1.0 - b, b) - 1.0
                        }
                    } else {
                        a.mul_add(b, a * b)
                    }
                }
                5 => (a + b) * 0.5,
                6 => a.min(b),
                7 => a.max(b),
                8 => a,
                _ => a + b,
            };
        }
    }
    out
}
pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let (w, h) = (src.w, src.h);
    let (f, q, pulse_step) = holds(p, ctx.frame_number);
    let use_flow = p.b("Use Flow Warp");
    let pulse_on = p.b("Add Pulse Frame");
    let mask = crate::warp::warp_mask(
        ctx.mask,
        w,
        h,
        (p.f("Mask Use") >= 2.0).then_some(if p.mode() == crate::glow::AlphaMode::AllOpaque {
            0
        } else {
            3
        }),
        p.b("Invert Mask"),
        p.f("Blur Mask") * ctx.frame.w * 0.125,
        p.mode(),
    );
    let flow_mask = p.f("Mask Use") == 3.0;
    let mask_at = |i| mask.as_ref().map(|m| m.data[i]).unwrap_or(1.0);
    let held = if use_flow {
        get(ctx, p, src, f)
    } else {
        src.clone()
    };
    let previous = if use_flow {
        get(ctx, p, src, f - 1.0)
    } else {
        src.clone()
    };
    let mut pulse = Image::filled(w, h, [0.0, 0.0, 0.0, 1.0]);
    if pulse_on {
        let (a, b) = (get(ctx, p, src, q), get(ctx, p, src, q - 1.0));
        for i in 0..w * h {
            for c in 0..3 {
                let d = a.data[i][c] - b.data[i][c];
                pulse.data[i][c] = threshold_abs(d, p.f("Pulse Threshold"));
            }
        }
        pulse = block(
            &pulse,
            p.f("Pulse Block Size X") * ctx.scale[0],
            p.f("Pulse Block Size Y") * ctx.scale[1],
        );
        for (i, v) in pulse.data.iter_mut().enumerate() {
            for c in 0..3 {
                v[c] *= (ctx.frame_number - q) * if flow_mask { mask_at(i) } else { 1.0 };
            }
        }
    }
    let intensity = if pulse_step != 0.0 {
        p.f("Pulse Intensity") / pulse_step
    } else {
        0.0
    };
    let previous = if pulse_on && p.b("Pulse Before Flow") {
        combine(
            &pulse,
            &previous,
            intensity,
            p.f("Pulse Bg Brightness"),
            p.f("Pulse Combine") as i32,
        )
    } else {
        previous
    };
    let mut flow = if let Some(alt) = ctx.alt_flow {
        let mut a = crate::temporal::to_working(alt.clone(), p.mode());
        for v in &mut a.data {
            for c in 0..3 {
                v[c] *= p.f("AltFlowData Scale");
            }
        }
        a
    } else if use_flow {
        let r = p.f("Blur Flow");
        let a = crate::batch4::blur(&held, [r * ctx.scale[0], r * ctx.scale[1]], 4);
        let b = crate::batch4::blur(src, [r * ctx.scale[0], r * ctx.scale[1]], 4);
        flow::compute(
            &a,
            &b,
            (50.0 * (1.0 - p.f("Flow Field Smoothness")) + 20.0).clamp(1.0, 100.0),
            p.f("Emphasize Edges") * 0.5,
        )
    } else {
        Image::filled(w, h, [0.0, 0.0, 0.0, 1.0])
    };
    if flow_mask {
        for (i, v) in flow.data.iter_mut().enumerate() {
            for c in 0..3 {
                v[c] *= mask_at(i);
            }
        }
    }
    let pixelated = if use_flow {
        block(
            &flow,
            p.f("Block Size X") * ctx.scale[0],
            p.f("Block Size Y") * ctx.scale[1],
        )
    } else {
        flow.clone()
    };
    let mut coords = if use_flow {
        flow::inverse(&held, src, &pixelated)
    } else {
        Image::filled(w, h, [0.0, 0.0, 0.0, 1.0])
    };
    for v in &mut coords.data {
        v[0] *= p.f("Flow Warp Scale");
        v[1] *= p.f("Flow Warp Scale");
    }
    let mut warp_source = previous.clone();
    if flow_mask {
        for (i, v) in warp_source.data.iter_mut().enumerate() {
            for c in 0..4 {
                v[c] = src.data[i][c] + (v[c] - src.data[i][c]) * mask_at(i);
            }
        }
    }
    let mut warped = src.clone();
    let mut holes = Image::filled(w, h, [0.0, 0.0, 0.0, 1.0]);
    if use_flow {
        let white = Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| {
                    let m = if flow_mask { mask_at(i) } else { 1.0 };
                    [m, m, m, 1.0]
                })
                .collect(),
        };
        for i in 0..w * h {
            let (x, y) = (
                (i % w) as f32 + coords.data[i][0] * w as f32 * 0.5,
                (i / w) as f32 - coords.data[i][1] * w as f32 * 0.5,
            );
            warped.data[i] = unfiltered(&warp_source, x, y);
            let hole = 1.0 - unfiltered(&white, x, y)[0];
            holes.data[i] = [hole, hole, hole, 1.0];
        }
    }
    let mut comp = warped.clone();
    for i in 0..w * h {
        for c in 0..4 {
            comp.data[i][c] += (src.data[i][c] - comp.data[i][c]) * holes.data[i][0];
        }
    }
    let noised = if pulse_on {
        if p.b("Pulse Before Flow") {
            if use_flow {
                comp.clone()
            } else {
                previous
            }
        } else {
            combine(
                &pulse,
                &comp,
                intensity,
                p.f("Pulse Bg Brightness"),
                p.f("Pulse Combine") as i32,
            )
        }
    } else {
        comp
    };
    let mut result = match p.f("Show") as i32 {
        2 => src.clone(),
        3 => flow.clone(),
        4 => {
            let scale =
                (50.0 * (1.0 - p.f("Flow View Scale")) + 1.0).clamp(0.01, 100.0) * w as f32 / 720.0;
            let mut a = src.clone();
            for (i, v) in a.data.iter_mut().enumerate() {
                let (x, y) = (-flow.data[i][0] / scale, flow.data[i][1] / scale);
                let h = (y.atan2(x) / std::f32::consts::TAU + 0.5).rem_euclid(1.0);
                let s = (x * x + y * y).sqrt();
                let rgb = crate::colorops::hsv_to_rgb(h, s, 1.0);
                for c in 0..3 {
                    v[c] = rgb[c] + (src.data[i][c] - rgb[c]) * 0.15;
                }
            }
            a
        }
        5 => pixelated,
        6 => coords,
        7 => warped,
        8 => holes,
        _ => noised,
    };
    if !flow_mask {
        for (i, v) in result.data.iter_mut().enumerate() {
            for c in 0..4 {
                v[c] = src.data[i][c] + (v[c] - src.data[i][c]) * mask_at(i);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_fractional_holds_checkout_both_neighbours() {
        let mut p = super::super::defaults(super::super::Kind::PixelMosh);
        for (k, v) in [
            ("Vary Hold Frame", 2.0),
            ("Flow Warp Hold Frame", 8.25),
            ("Add Pulse Frame", 1.0),
            ("Pulse Hold Frame", 3.5),
        ] {
            p.values.insert(k, [v, 0.0, 0.0, 0.0]);
        }
        assert_eq!(needs(&p, 19.0, 100.0), vec![2, 3, 4, 7, 8, 9]);
        p.values
            .insert("Flow Warp Hold Frame", [200.0, 0.0, 0.0, 0.0]);
        assert_eq!(needs(&p, 19.0, 10.0), vec![2, 3, 4, 9]);
    }
    #[test]
    fn pulse_average_keeps_native_fractional_alpha() {
        let fg = Image::filled(3, 3, [0.2, 0.0, 0.0, 1.0]);
        let bg = Image::filled(3, 3, [0.5, 0.5, 0.5, 1.0]);
        let a = combine(&fg, &bg, 0.14, 0.8, 5);
        assert!((a.data[0][3] - 0.57).abs() < 1e-6);
        assert!((a.data[0][0] - 0.214).abs() < 1e-6);
    }
    #[test]
    fn pulse_threshold_uses_integer_gain() {
        assert!((threshold_abs(0.8, 0.6) - 0.4).abs() < 1e-6);
        assert!((threshold_abs(-0.9, 0.7) + 0.6).abs() < 1e-6);
        assert_eq!(threshold_abs(0.6, 0.6), 0.0);
    }
    #[test]
    fn odd_block_boundaries_follow_normalized_y_up_graph() {
        let mut src = Image::filled(123, 77, [0.0; 4]);
        for y in 0..77 { for x in 0..123 { src.data[y * 123 + x] = [x as f32, y as f32, 0.0, 1.0]; } }
        let a = block(&src, 32.0, 32.0);
        assert!((a.at(77, 54)[0] - 61.0).abs() < 0.01);
        assert!((a.at(78, 54)[0] - 93.0).abs() < 0.01);
        assert!((a.at(77, 54)[1] - 38.0).abs() < 0.01);
        assert!((a.at(77, 55)[1] - 70.0).abs() < 0.01);
    }
}
