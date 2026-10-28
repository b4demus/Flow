use super::{noise, Context, Settings};
use crate::{shake::hash4, Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Sample {
    px: [f32; 4],
    key: f32,
    mask: f32,
    noise: f32,
    index: Option<usize>,
}
fn index(x: f32, y: f32, w: usize, h: usize, round: bool) -> Option<usize> {
    if !(x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32) {
        return None;
    }
    let (x, y) = if round {
        ((x + 0.5).floor() as usize, (y + 0.5).floor() as usize)
    } else {
        (x as usize, y as usize)
    };
    (x < w && y < h).then(|| y * w + x)
}
fn selection(a: &mut [Sample], reverse: bool) {
    for i in 0..a.len() {
        let mut best = i;
        for j in i + 1..a.len() {
            if if reverse {
                a[j].key > a[best].key
            } else {
                a[j].key < a[best].key
            } {
                best = j;
            }
        }
        if best != i {
            let p = a[i].px;
            let k = a[i].key;
            a[i].px = a[best].px;
            a[i].key = a[best].key;
            a[best].px = p;
            a[best].key = k;
        }
    }
}
fn sort_path(a: &mut [Sample], reverse: bool, maxlen: f32, soft: bool) {
    if !soft {
        let mut i = 0;
        while i < a.len() {
            if a[i].mask < 0.01 {
                a[i].mask = 0.0;
            } else if a[i].mask > 0.99 {
                a[i].mask = 1.0;
            } else if a[i].mask > 0.01 && a[i].mask < 0.99 {
                let end = a.len() - 1;
                let threshold = dither_threshold(a[end].noise);
                for p in &mut a[i..end] {
                    p.mask = if p.mask > threshold { 1.0 } else { 0.0 };
                }
                i = end;
            }
            i += 1;
        }
    }
    let (mut start, mut count) = (None, 0);
    for i in 0..a.len() {
        let active = a[i].mask > 0.5;
        let limit = (maxlen + a[i].noise * maxlen).max(0.0);
        if let Some(s) = start {
            count += 1;
            if i == a.len() - 1 || count as f32 >= limit || !active {
                selection(&mut a[s..i], reverse);
                start = None;
                count = 0;
            }
        } else if active {
            start = Some(i);
            count = 1;
        }
    }
}
fn dither_threshold(v: f32) -> f32 {
    let mut u = v.to_bits().wrapping_mul(0x1001);
    u = ((u >> 22) ^ u).wrapping_mul(0x11);
    u = ((u >> 9) ^ u).wrapping_mul(0x401);
    u = ((u >> 2) ^ u).wrapping_mul(0x81);
    u = (u >> 12) ^ u;
    u = u.wrapping_mul(0x10001);
    u = ((u >> 13) ^ u).wrapping_mul(0x11);
    u = ((u >> 7) ^ u).wrapping_mul(0x401);
    u = ((u >> 5) ^ u).wrapping_mul(0x101);
    u = (u >> 16) ^ u;
    u as f32 * (1.0 / 4294967296.0)
}
fn key(px: [f32; 4], ty: i32) -> f32 {
    let [r, g, b, _] = px;
    match ty {
        2 => (r + g + b) / 3.0,
        3 => r.min(g).min(b),
        4 => r.max(g).max(b),
        5 => r,
        6 => g,
        7 => b,
        8 => crate::colorops::rgb_to_hsv([r, g, b]).0,
        9 => crate::colorops::rgb_to_hsv([r, g, b]).1,
        10 => crate::colorops::rgb_to_hsv([r, g, b]).2,
        _ => (r * 0.3125 + g * 0.5) + b * 0.1875,
    }
}
fn angle(x: f32, y: f32) -> f32 {
    (((y as f64).atan2(x as f64) * 180.0 / std::f64::consts::PI) as f32).rem_euclid(360.0)
}
fn circle(
    r: f32,
    start: f32,
    c: [f32; 2],
    w: usize,
    h: usize,
    cap: usize,
) -> Vec<(Option<usize>, f32)> {
    let mut a = start.rem_euclid(360.0);
    let rad = a * 0.017453292;
    let (mut x, mut y) = (r * rad.cos(), r * rad.sin());
    let mut swept = 0.0;
    let mut out = Vec::new();
    for _ in 0..cap {
        out.push((index(c[0] + x, c[1] + y, w, h, true), swept));
        if a <= 45.0 || a >= 315.0 {
            y -= 1.0;
            x = (r * r - y * y).sqrt();
        } else if a <= 135.0 {
            x += 1.0;
            y = (r * r - x * x).sqrt();
        } else if a <= 225.0 {
            y += 1.0;
            x = -(r * r - y * y).sqrt();
        } else {
            x -= 1.0;
            y = -(r * r - x * x).sqrt();
        }
        let next = angle(x, y);
        swept += if next <= a {
            a - next
        } else {
            360.0 - next + a
        };
        if swept > 360.0 {
            break;
        }
        a = next;
    }
    out
}
pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let (w, h) = (src.w, src.h);
    if w == 0 || h == 0 {
        return src.clone();
    }
    let above = p.f("Sort Direction") == 2.0;
    let keys: Vec<_> = src
        .data
        .par_iter()
        .map(|v| key(*v, p.f("Sort Type") as i32))
        .collect();
    let mut threshold = Plane {
        w,
        h,
        data: keys
            .iter()
            .map(|v| {
                if (*v >= p.f("Threshold")) == above {
                    1.0
                } else {
                    0.0
                }
            })
            .collect(),
    };
    crate::boxes::box_passes(
        &mut threshold,
        p.f("Soften Threshold Mask") * ctx.scale[0],
        p.f("Soften Threshold Mask") * ctx.scale[1],
        crate::Edge::Reflect,
        3,
    );
    let n = noise::field(w, h, p.f("Randomly Restart Sort"), 2, p.f("Seed"));
    let dots = noise::dots(&n, w, h);
    let matte = crate::warp::warp_mask(
        ctx.mask,
        w,
        h,
        (p.f("Matte Use") == 2.0).then_some(if p.mode() == crate::glow::AlphaMode::AllOpaque {
            0
        } else {
            3
        }),
        p.b("Invert Matte"),
        p.f("Blur Matte") * ctx.scale[0],
        p.mode(),
    );
    let masks: Vec<_> = (0..w * h)
        .map(|i| {
            threshold.data[i] * matte.as_ref().map(|m| m.data[i]).unwrap_or(1.0) * (1.0 - dots[i])
        })
        .collect();
    let show = p.f("Show") as i32;
    let mut out = src.clone();
    if (2..=5).contains(&show) {
        let field = match show {
            2 => &keys,
            3 => &threshold.data,
            4 => &dots,
            _ => &masks,
        };
        for (i, v) in out.data.iter_mut().enumerate() {
            *v = [field[i], field[i], field[i], src.data[i][3]];
        }
    } else {
        let len_noise = noise::field(w, h, 100.0, 5, p.f("Seed"));
        let samples: Vec<_> = (0..w * h)
            .map(|i| Sample {
                px: src.data[i],
                key: if n[i] < 0.1 {
                    if above {
                        0.0
                    } else {
                        1.0
                    }
                } else {
                    keys[i]
                },
                mask: masks[i],
                noise: len_noise[i],
                index: Some(i),
            })
            .collect();
        let empty = Sample {
            px: [0.0; 4],
            key: 0.0,
            mask: 0.0,
            noise: 0.0,
            index: None,
        };
        let mode = p.f("Effect") as i32;
        let user_reverse = p.b("Reverse Sort Direction");
        let mut paths: Vec<Vec<(Option<usize>, bool)>> = Vec::new();
        let mut reverse = user_reverse;
        if mode == 1 {
            let a = p.f("Sort Angle").rem_euclid(360.0);
            if a == 0.0 || a == 180.0 {
                for y in 0..h {
                    let mut row: Vec<_> = (0..w).map(|x| (Some(y * w + x), true)).collect();
                    if a == 180.0 {
                        row.reverse();
                    }
                    paths.push(row);
                }
            } else if a == 90.0 || a == 270.0 {
                for x in 0..w {
                    let mut col: Vec<_> = (0..h).map(|y| (Some(y * w + x), true)).collect();
                    if a == 270.0 {
                        col.reverse();
                    }
                    paths.push(col);
                }
            } else {
                if a > 180.0 {
                    reverse = !reverse;
                }
                let m = (a * 0.017453292).tan();
                let (dx, dy) = if m > 1.0 {
                    (1.0 / m, 1.0)
                } else if m > 0.0 {
                    (1.0, m)
                } else if m > -1.0 {
                    (-1.0, -m)
                } else {
                    (1.0 / m, 1.0)
                };
                for j in 0..w + h - 1 {
                    let (mut x, mut y) = if m <= 0.0 {
                        let q = j as i32 - (w as i32 - 1);
                        ((w as i32 - 1 + q.min(0)) as f32, q.max(0) as f32)
                    } else {
                        let q = j as i32 - (h as i32 - 1);
                        (q.max(0) as f32, (-q).max(0) as f32)
                    };
                    let mut path = Vec::new();
                    for _ in 0..w.max(h) {
                        if let Some(i) = index(x, y, w, h, false) {
                            path.push((Some(i), true));
                        } else {
                            break;
                        }
                        x += dx;
                        y += dy;
                        if index(x, y, w, h, false).is_none() {
                            path.pop();
                            break;
                        }
                    }
                    paths.push(path);
                }
            }
        } else if mode == 2 {
            let c = p.xy("Center");
            let center = c;
            let mut seed = (p.f("Seed") * 4096.0) as i32 as u32;
            for j in 0..2 * (w + h) - 4 {
                let endpoint = if j < w - 1 {
                    [j as f32, 0.0]
                } else if j < w + h - 2 {
                    [(w - 1) as f32, (j - (w - 1)) as f32]
                } else if j < 2 * w + h - 3 {
                    [(w - 1 - (j - (w + h - 2))) as f32, (h - 1) as f32]
                } else {
                    [0.0, (h - 1 - (j - (2 * w + h - 3))) as f32]
                };
                let (dx, dy) = (endpoint[0] - center[0], endpoint[1] - center[1]);
                let span = dx.abs().max(dy.abs());
                let off = 180.0
                    * (((hash4(seed) & 0x3fffffff) as f64 / 1073741824.0 * 2.0 - 1.0)
                        * p.f("Vary Radius") as f64) as f32;
                seed = hash4(seed);
                let inner = 2.0 * p.f(" Inner Radius") * ctx.scale[0] + off;
                let outer = inner + 2.0 * p.f("Ray Length") * ctx.scale[0];
                let a = angle(dx, -dy);
                let start = p.f(" Start Angle").rem_euclid(360.0);
                let end = (start + p.f(" Degrees Sorted")).rem_euclid(360.0);
                let allow = p.f(" Degrees Sorted") >= 360.0
                    || if end >= start {
                        a >= start && a <= end
                    } else {
                        a >= start || a <= end
                    };
                let (mut x, mut y) = (center[0], center[1]);
                let mut path = Vec::new();
                if span >= 0.01 {
                    let (sx, sy) = (dx / span, dy / span);
                    for _ in 0..(w + h) * 4 {
                        let inside = index(x, y, w, h, false);
                        if let Some(i) = inside {
                            let r2 = (x - center[0]).powi(2) + (y - center[1]).powi(2);
                            path.push((
                                Some(i),
                                allow && r2 >= inner * inner && r2 <= outer * outer,
                            ));
                            if path.len() >= w + h {
                                break;
                            }
                        } else if !path.is_empty()
                            || (x < 0.0 && sx <= 0.0)
                            || (x >= w as f32 && sx >= 0.0)
                            || (y < 0.0 && sy <= 0.0)
                            || (y >= h as f32 && sy >= 0.0)
                        {
                            break;
                        }
                        x += sx;
                        y += sy;
                    }
                }
                path.pop();
                paths.push(path);
            }
        } else {
            let c = p.xy("Circle Center");
            let center = c;
            let mut seed = (p.f("Seed") * 4096.0) as i32 as u32;
            for r in 0..w + h {
                let allow = r as f32 >= 2.0 * p.f("Inner Radius") * ctx.scale[0]
                    && r as f32 <= 2.0 * (p.f("Inner Radius") + p.f("Thickness")) * ctx.scale[0];
                let off = 180.0
                    * (((hash4(seed) & 0x3fffffff) as f64 / 1073741824.0 * 2.0 - 1.0)
                        * p.f("Vary Start") as f64) as f32;
                seed = hash4(seed);
                paths.push(
                    circle(
                        r as f32,
                        p.f("Start Angle") - off,
                        center,
                        w,
                        h,
                        2 * (w + h),
                    )
                    .into_iter()
                    .map(|(i, sweep)| (i, allow && sweep <= p.f("Degrees Sorted")))
                    .collect(),
                );
            }
        }
        // Native paths are gathered before any writes; overlap is resolved in path order.
        let result: Vec<_> = paths
            .par_iter()
            .map(|path| {
                let mut a: Vec<_> = path
                    .iter()
                    .map(|(i, allowed)| {
                        let mut s = i.map(|j| samples[j]).unwrap_or(empty);
                        if !allowed {
                            s.mask = 0.0;
                        }
                        s
                    })
                    .collect();
                sort_path(&mut a, reverse, w.max(h) as f32, p.b("Soft Borders"));
                a
            })
            .collect();
        for a in result {
            for s in a {
                if let Some(i) = s.index {
                    out.data[i] = s.px;
                }
            }
        }
    }
    let mix = p.f("Mix With Source");
    for (i, v) in out.data.iter_mut().enumerate() {
        for c in 0..4 {
            v[c] += (src.data[i][c] - v[c]) * mix;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_selection_swaps_tied_colours() {
        let mut a: Vec<_> = [2.0, 2.0, 1.0]
            .into_iter()
            .enumerate()
            .map(|(i, k)| Sample {
                px: [i as f32; 4],
                key: k,
                mask: 1.0,
                noise: 0.5,
                index: Some(i),
            })
            .collect();
        selection(&mut a, false);
        assert_eq!(
            a.iter().map(|s| s.px[0]).collect::<Vec<_>>(),
            vec![2.0, 1.0, 0.0]
        );
    }
    #[test]
    fn circular_path_matches_native_cardinal_samples() {
        let a = circle(10.0, 0.0, [32.0, 24.0], 64, 48, 224);
        assert_eq!(a[0].0, Some(24 * 64 + 42));
        assert_eq!(a[14].0, Some(14 * 64 + 32));
        assert_eq!(a[28].0, Some(24 * 64 + 22));
        assert_eq!(a[42].0, Some(34 * 64 + 32));
    }
}
