//! Procedural scratch segments recovered from native lifetime, weave and
//! scan-conversion maths. No image or stamp resources are used.
use super::{Context, Settings};
use crate::{
    shake::{hash4, linear_noise1, quadratic_noise1},
    Image,
};

fn unit(h: u32) -> f64 {
    (h & 0x3fff_ffff) as f64 / 1073741824.0
}
fn ranged(seed: u32, at: f32, octaves: usize, lo: f32, hi: f32) -> f32 {
    lo + ((linear_noise1(seed, at, octaves) - -1.0) / 2.0) * (hi - lo)
}
struct Segment {
    start: [f32; 2],
    end: [f32; 2],
    time: f32,
    width: [[f32; 2]; 2],
    frequency: [f32; 2],
    width_seed: u32,
    gate_seed: u32,
    taper: f32,
    luma: [f32; 2],
    luma_seed: u32,
    opacity: f32,
}

fn make_segment(
    p: &Settings,
    ctx: &Context,
    seed: u32,
    series: u32,
    time: f32,
    begin: f32,
    end: f32,
    gate_seed: u32,
    color: f32,
) -> Segment {
    let w = ctx.frame.w;
    let h = ctx.frame.h;
    let x_seed = hash4(seed.wrapping_add(series + 20));
    let width_seed = hash4(seed.wrapping_add(series + 30));
    let area_lo = (p.f("Scratch Area Center") - p.f("Scratch Area Width") + 1.0) * (w * 0.5);
    let area_hi = (p.f("Scratch Area Center") + p.f("Scratch Area Width") + 1.0) * (w * 0.5);
    let mut r = hash4(x_seed.wrapping_add(10101));
    let base = area_lo + (area_hi - area_lo) * unit(r) as f32;
    r = hash4(r);
    let weave = (unit(r) * p.f("Weave Amount") as f64 * (w * 0.5) as f64) as f32;
    let phase = unit(hash4(x_seed.wrapping_add(77))) as f32;
    let frequency = p.f("Weave Frequency") * phase * phase;
    let x = |t| {
        base - weave
            + ((quadratic_noise1(x_seed, t * frequency, 10) + 1.0) / 2.0)
                * ((base + weave) - (base - weave))
    };
    let start = [x(begin), (begin - time) * h];
    let mut finish = [x(end), (end - time) * h];
    let dx = finish[0] - start[0];
    let dy = finish[1] - start[1];
    let length = (dx * dx + dy * dy).sqrt();
    let cosine = if length == 0.0 { 0.0 } else { dy / length };
    let secant = if dy == 0.0 { 0.0 } else { length / dy };
    let scale = secant.min(5.0) * (h / 486.0 + cosine * (w / 720.0 - h / 486.0));
    finish[0] = start[0] + cosine * dx;
    finish[1] = start[1] + cosine * dy;
    let width = p.f("Scratch Width");
    let inv_width = 1.0 / (width * 0.5).max(1.0);
    r = hash4(width_seed.wrapping_add(1));
    let mut frequencies = [0.0; 2];
    for f in &mut frequencies {
        let scale = (unit(r) * (1.0 - 0.01f32 as f64) + 0.01f32 as f64) as f32 * inv_width;
        *f = p.f("Scratch Rough Freq") * scale;
        r = hash4(r);
    }
    r = hash4(width_seed.wrapping_sub(1));
    let mut widths = [[0.0; 2]; 2];
    for v in &mut widths {
        let half = width
            * (p.f("Vary Scratches Width")
                * (unit(r) * (1.0 - 0.02f32 as f64) + 0.02f32 as f64) as f32
                + 1.0)
            * scale;
        *v = [
            half * (1.0 - p.f("Scratch Roughness")),
            half * (1.0 + p.f("Scratch Roughness")),
        ];
        r = hash4(r);
    }
    let taper = p.f("Scratches Taper")
        * cosine
        * (widths[0][1] + widths[1][1]).min(1.0)
        * (h + cosine * (w - h));
    // The AE wrapper overrides the graph's hidden zero defaults: luminance
    // varies by 0.1 per strand and by 0.2 along it, opacity by 0.1 per strand.
    let luma_seed = hash4(seed.wrapping_add(series + 50));
    let luma = color + 0.1 * (unit(hash4(hash4(luma_seed.wrapping_sub(1)))) * 2.0 - 1.0) as f32;
    let opacity_seed = hash4(seed.wrapping_add(series + 40));
    let opacity = p.f("Scratch Opacity")
        * (1.0 + 0.1 * (unit(hash4(hash4(opacity_seed.wrapping_sub(1)))) * 2.0 - 1.0) as f32);
    Segment {
        start,
        end: finish,
        time: begin,
        width: widths,
        frequency: frequencies,
        width_seed,
        gate_seed,
        taper,
        luma: [luma - 0.2, luma + 0.2],
        luma_seed,
        opacity,
    }
}

fn segments(
    p: &Settings,
    ctx: &Context,
    seed: u32,
    count: f32,
    length: f32,
    color: f32,
) -> Vec<Segment> {
    let time = ctx.frame_number as i32 as f32;
    let mut result = Vec::new();
    if count <= 0.0 || length <= 0.0 || p.f("Scratch Width") <= 0.0 || p.f("Scratch Opacity") <= 0.0
    {
        return result;
    }
    for series in 0..(count.ceil() as u32).min(100000) {
        let gate_seed = hash4(seed.wrapping_add(series + 10));
        let mut t = -0.2f32;
        let mut active = false;
        let mut begin = 0.0;
        let mut previous = 0.0;
        while t <= 1.2001001 {
            let n = linear_noise1(gate_seed, (1.0 / length) * (time + t), 8);
            if n > 0.3 && !active {
                begin = if t == -0.2 {
                    t
                } else {
                    t - 0.025 + (0.025 * (0.3 - previous)) / (n - previous)
                };
                active = true;
            } else if n <= 0.3 && active {
                let end = t - 0.025 + (0.025 * (0.3 - previous)) / (n - previous);
                result.push(make_segment(
                    p,
                    ctx,
                    seed,
                    series,
                    time,
                    time + begin,
                    time + end,
                    gate_seed,
                    color,
                ));
                active = false;
            }
            previous = n;
            t += 0.025;
        }
        if active {
            result.push(make_segment(
                p,
                ctx,
                seed,
                series,
                time,
                time + begin,
                time + t,
                gate_seed,
                color,
            ));
        }
    }
    result.sort_by(|a, b| {
        a.start[1]
            .total_cmp(&b.start[1])
            .then(a.start[0].total_cmp(&b.start[0]))
    });
    result
}

// Integral over a unit pixel of a moving step edge. The iterative form keeps
// the native floating-point accumulation for wide and sloping scratches.
fn edge(row: &mut [f32], a: f32, b: f32, subtract: bool, clipped: bool) {
    let lo = a.min(b);
    let hi = a.max(b);
    let first = lo.floor() as i32;
    let last = hi.floor() as i32;
    let mut put = |i: i32, v: f32| {
        if i >= if clipped { 1 } else { 0 } && i < row.len() as i32 {
            if subtract {
                row[i as usize] -= v;
            } else {
                row[i as usize] = v;
            }
        }
    };
    if first == last {
        put(
            first,
            1.0 - ((lo - first as f32) + (hi - last as f32)) * 0.5,
        );
    } else {
        let inv = (1.0f64 / (hi - lo) as f64) as f32;
        let rest = 1.0 - (lo - first as f32);
        let mut ramp = rest * inv;
        put(first, rest * ramp * 0.5);
        for i in first + 1..last {
            put(i, ramp + inv * 0.5);
            ramp += inv;
        }
        put(last, 1.0 - (1.0 - ramp) * (hi - last as f32) * 0.5);
    }
}

fn draw(out: &mut Image, p: &Settings, ctx: &Context, segments: &[Segment]) {
    let w = ctx.frame.w as usize;
    let h = ctx.frame.h as usize;
    let step = (1.0f64 / h as f64) as f32;
    let mut coverage = vec![0.0f32; w];
    // Native workers restart the scanline accumulators in horizontal strips.
    // The reset is observable at high roughness frequencies in float32.
    let workers = std::env::var("SAPPHIRE_FORCE_NCPUS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
        .min(128)
        .min(h.max(1));
    let strip_height = h / workers;
    for s in segments.iter().rev() {
        if s.end[1] <= s.start[1] {
            continue;
        }
        for strip in 0..workers {
            let strip_first = strip * strip_height;
            let strip_end = if strip + 1 == workers {
                h
            } else {
                strip_first + strip_height
            };
            let first = (s.start[1].ceil() as i32 - 1).max(strip_first as i32);
            let last = (s.end[1].floor() as i32).min(strip_end as i32 - 1);
            if first > last {
                continue;
            }
            let top = (first as f32).max(s.start[1]);
            let mut center = s.start[0]
                + ((first as f32 - s.start[1]) / (s.end[1] - s.start[1])) * (s.end[0] - s.start[0]);
            let delta = (s.end[0] - center) / (s.end[1] - top);
            let mut row_time = strip_first as f32 * step;
            for _ in strip_first as i32..first {
                row_time += step;
            }
            let mut time = s.time + row_time;
            let taper = ((s.end[1] - s.start[1]).abs() * 0.6).min(s.taper);
            let widths = |at: f32, y: f32| {
                let mut result = [0.0; 2];
                for c in 0..2 {
                    result[c] = ranged(
                        s.width_seed,
                        at * s.frequency[c],
                        5,
                        s.width[c][0],
                        s.width[c][1],
                    );
                }
                if y < s.start[1] + taper {
                    for v in &mut result {
                        *v *= ((y - s.start[1]) / taper).max(0.0);
                    }
                }
                if y > s.end[1] - taper {
                    for v in &mut result {
                        *v *= ((s.end[1] - y) / taper).max(0.0);
                    }
                }
                result
            };
            let mut previous = None;
            for y in first..=last {
                let next_time = time + step;
                let mut next = [0.0; 2];
                let gaps = p.f("Gaps");
                let gate = if gaps < 0.999 {
                    let n = ranged(
                        hash4(s.gate_seed),
                        next_time * p.f("Gaps Freq"),
                        8,
                        0.0,
                        1.0,
                    );
                    ((n - (gaps - 0.1)) * 5.0).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let opacity = s.opacity.clamp(0.0, 1.0) * gate;
                let color =
                    ranged(s.luma_seed, next_time * 28.0, 5, s.luma[0], s.luma[1]).clamp(0.0, 1.0);
                if opacity > 0.0 {
                    let prev = previous.unwrap_or_else(|| widths(time, y as f32));
                    next = widths(next_time, (y + 1) as f32);
                    for v in &mut next {
                        *v *= gate;
                    }
                    let left = [center - prev[0], center + delta - next[0]];
                    let right = [center + prev[1], center + delta + next[1]];
                    let lo = left[0].min(left[1]).floor() as i32;
                    let hi = right[0].max(right[1]).floor() as i32;
                    if hi >= 0 && lo < w as i32 {
                        let clipped = left
                            .into_iter()
                            .chain(right)
                            .any(|x| x.floor() < 0.0 || x.floor() >= w as f32);
                        coverage.fill(0.0);
                        edge(&mut coverage, left[0], left[1], false, clipped);
                        let from = (left[0].max(left[1]).floor() as i32 + 1).max(0);
                        for x in from..=hi.min(w as i32 - 1) {
                            coverage[x as usize] = 1.0;
                        }
                        edge(&mut coverage, right[0], right[1], true, clipped);
                        let vertical = if s.start[1] > y as f32 {
                            (y + 1) as f32 - s.start[1]
                        } else if s.end[1] > y as f32 && s.end[1] < (y + 1) as f32 {
                            s.end[1] - y as f32
                        } else {
                            1.0
                        };
                        let gy = if p.b("Flip Stamps Vertically") {
                            h as i32 - y - 1
                        } else {
                            y
                        };
                        let iy = gy - ctx.frame.src_y;
                        if iy >= 0 && iy < out.h as i32 {
                            for x in lo.max(ctx.frame.src_x).max(0)
                                ..(hi + 1).min(ctx.frame.src_x + out.w as i32).min(w as i32)
                            {
                                let alpha = opacity * coverage[x as usize] * vertical;
                                let v = &mut out.data
                                    [iy as usize * out.w + (x - ctx.frame.src_x) as usize];
                                for c in 0..3 {
                                    v[c] = alpha * color + (1.0 - alpha) * v[c];
                                }
                            }
                        }
                    }
                }
                previous = Some(next);
                time = next_time;
                center += delta;
            }
        }
    }
}

pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let seed = ((p.f("Seed") + 0.3) * 4096.0) as i32 as u32;
    let mut out = src.clone();
    for (seed, count, length, color) in [
        (
            seed,
            p.f("Scratches") * p.f("Black Scratches"),
            p.f("Black Scratch Length"),
            0.0,
        ),
        (
            hash4(seed),
            p.f("Scratches") * p.f("White Scratches"),
            p.f("White Scratch Length"),
            1.0,
        ),
    ] {
        let segments = segments(p, ctx, seed, count, length, color);
        draw(&mut out, p, ctx, &segments);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{batch4::Kind, shake::Frame};

    #[test]
    fn scratch_tiles_and_vertical_flip_preserve_coverage_and_alpha() {
        let source = Image::filled(96, 64, [0.7, 0.5, 0.3, 0.6]);
        let mut p = Settings::defaults(Kind::FilmDamage);
        p.values.insert("Scratches", [100.0, 0.0, 0.0, 0.0]);
        p.values.insert("Scratch Width", [1.0, 0.0, 0.0, 0.0]);
        let mut ctx = Context {
            frame: Frame {
                w: 96.0,
                h: 64.0,
                src_x: 0,
                src_y: 0,
            },
            time: 0.6,
            fps: 25.0,
            frame_number: 15.0,
            total_frames: 100.0,
            old: None,
            previous: None,
            next: None,
            mask: None,
            background: None,
            scale: [1.0; 2],
        };
        let full = render(&source, &p, &ctx);
        assert!(full.data.iter().any(|v| v[0] != 0.7));
        assert!(full
            .data
            .iter()
            .all(|v| v[3] == 0.6 && v.iter().all(|c| c.is_finite())));
        ctx.frame.src_x = 23;
        ctx.frame.src_y = 17;
        let tile = render(&Image::filled(41, 29, [0.7, 0.5, 0.3, 0.6]), &p, &ctx);
        for y in 0..29 {
            for x in 0..41 {
                assert_eq!(tile.data[y * 41 + x], full.data[(y + 17) * 96 + x + 23]);
            }
        }
        ctx.frame.src_x = 0;
        ctx.frame.src_y = 0;
        p.values
            .insert("Flip Stamps Vertically", [1.0, 0.0, 0.0, 0.0]);
        let flipped = render(&source, &p, &ctx);
        for y in 0..64 {
            for x in 0..96 {
                assert_eq!(flipped.data[y * 96 + x], full.data[(63 - y) * 96 + x]);
            }
        }
    }
}
