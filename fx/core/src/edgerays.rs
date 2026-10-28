//! Edge rays: light streaming from the edges of the source, radially away
//! from a centre. Measured in AE (spec/effects/glow-observations.md, "EdgeRays"):
//!   E     = |(gx, gy)|, g = 2·central differences per channel, each spread by a
//!           triple box of radius Thickness/2 (sub-pixel) before the magnitude;
//!           × (1 + Thickness/10) × Edge Brightness, capped at 1
//!   ray_c = weighted mean of E_c along the segment from the pixel towards the
//!           centre, length Rays Length · Length_c · distance to the centre
//!   out   = Screen of Rays Brightness · Rays Color · ray over the source

use crate::blur::Edge;
use crate::boxes::box3;
use crate::composite::Combine;
use crate::glow::{mix, source_rgb, Mix};
use crate::image::{Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Result,
    Edges,
    Rays,
}

#[derive(Clone, Debug)]
pub struct EdgeRaysParams {
    /// Centre in pixels (layer space of the image passed in).
    pub center: [f32; 2],
    pub length: f32,
    pub length_rgb: [f32; 3],
    pub reverse: f32,
    pub brightness: f32,
    pub color: [f32; 3],
    /// Pixels.
    pub blur: f32,
    pub blur_rel: [f32; 2],
    pub dark_rays: bool,
    pub bias_outer: f32,
    pub show: Show,
    /// Pixels.
    pub edge_thickness: f32,
    pub edge_brightness: f32,
    pub edge_subpixel: bool,
    /// Noise texture modulating the edges (our own noise: the reference's
    /// pattern is not reproduced, only its controls).
    pub shimmer_amp: f32,
    /// Cycles across the layer width.
    pub shimmer_freq: f32,
    pub shimmer_seed: f32,
    /// In noise cells, and cells per second.
    pub shimmer_shift: [f32; 2],
    pub shimmer_speed: [f32; 2],
    /// Seconds.
    pub time: f32,
    /// Layer width in pixels at the current resolution.
    pub layer_width: f32,
    /// Rays Res: 1 full, 2 half, 4 quarter.
    pub res: usize,
    pub from_alpha: f32,
    pub invert_matte: bool,
    pub mix: Mix,
}

impl EdgeRaysParams {
    /// Rays pull light from anywhere between a pixel and the centre.
    pub fn reach(&self) -> (f32, f32) {
        (1.0e6, 1.0e6)
    }
}

/// 2·central differences along x and y (edge pixels repeated at the border).
fn gradients(p: &Plane) -> (Plane, Plane) {
    let (w, h) = (p.w, p.h);
    let at = |x: isize, y: isize| -> f32 {
        p.data[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let mut gx = Plane::new(w, h);
    let mut gy = Plane::new(w, h);
    gx.data
        .par_chunks_mut(w)
        .zip(gy.data.par_chunks_mut(w))
        .enumerate()
        .for_each(|(y, (rx, ry))| {
            let y = y as isize;
            for x in 0..w as isize {
                rx[x as usize] = 2.0 * (at(x + 1, y) - at(x - 1, y));
                ry[x as usize] = 2.0 * (at(x, y + 1) - at(x, y - 1));
            }
        });
    (gx, gy)
}

#[inline]
fn bilinear(p: &Plane, x: f32, y: f32) -> f32 {
    let (w, h) = (p.w as isize, p.h as isize);
    let (x0, y0) = (x.floor() as isize, y.floor() as isize);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |xx: isize, yy: isize| {
        if xx >= 0 && yy >= 0 && xx < w && yy < h {
            p.data[(yy * w + xx) as usize]
        } else {
            0.0
        }
    };
    let a = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * fx;
    let b = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * fx;
    a + (b - a) * fy
}

/// Sum of weight·e and of the weights along c + (p − c)·s for s from 1 to
/// `s_end`. Weight runs from 1 − bias at the pixel to bias at the far end, so
/// bias 0 fades the rays out, 0.5 keeps them even, 1 brightens their ends.
fn ray_sum(
    e: &Plane,
    px: f32,
    py: f32,
    c: [f32; 2],
    s_end: f32,
    bias: f32,
    skip_first: bool,
) -> (f32, f32) {
    let (dx, dy) = (px - c[0], py - c[1]);
    let dist = (dx * dx + dy * dy).sqrt();
    let span = (1.0 - s_end).abs() * dist;
    let n = span.ceil().max(1.0) as usize;
    let (mut acc, mut wsum) = (0.0f32, 0.0f32);
    for k in (skip_first as usize)..=n {
        let t = k as f32 / n as f32;
        let s = 1.0 + (s_end - 1.0) * t;
        let wgt = (1.0 - bias) * (1.0 - t) + bias * t;
        acc += wgt * bilinear(e, c[0] + dx * s, c[1] + dy * s);
        wsum += wgt;
    }
    (acc, wsum)
}

/// Ray value at a pixel: one segment from 1 + len·reverse (away from the
/// centre) to 1 − len (towards it), averaged over its whole length.
fn ray_at(e: &Plane, px: f32, py: f32, c: [f32; 2], len: f32, reverse: f32, bias: f32) -> f32 {
    let (mut acc, mut wsum) = ray_sum(e, px, py, c, 1.0 - len, bias, false);
    if reverse > 0.0 {
        let (a, w) = ray_sum(e, px, py, c, 1.0 + len * reverse, bias, true);
        acc += a;
        wsum += w;
    }
    if wsum > 0.0 {
        acc / wsum
    } else {
        0.0
    }
}

pub fn render(src: &Image, bg: Option<&Image>, matte: Option<&Image>, p: &EdgeRaysParams) -> Image {
    let (w, h) = (src.w, src.h);
    let n = w * h;
    let mode = p.mix.alpha_mode;
    let cols: Vec<[f32; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            source_rgb(
                src.data[i],
                matte.map(|m| m.data[i]),
                mode,
                p.from_alpha,
                p.invert_matte,
            )
        })
        .collect();
    let gain = (1.0 + p.edge_thickness.max(0.0) / 10.0) * p.edge_brightness;
    let r_edge = if p.edge_subpixel {
        p.edge_thickness.max(0.0) / 2.0
    } else {
        (p.edge_thickness.max(0.0) / 2.0).round()
    };
    let e: Vec<Plane> = (0..3)
        .map(|k| {
            // Thickness spreads the signed gradients, the magnitude comes after
            // (a 1-px line keeps a dark core; box corners stay dimmer than sides).
            let (mut gx, mut gy) = gradients(&Plane {
                w,
                h,
                data: cols.iter().map(|c| c[k]).collect(),
            });
            if r_edge > 0.0 {
                box3(&mut gx, r_edge, r_edge, Edge::Reflect, true);
                box3(&mut gy, r_edge, r_edge, Edge::Reflect, true);
            }
            // Edges are capped at 1 before they become rays.
            let data = gx
                .data
                .iter()
                .zip(&gy.data)
                .map(|(a, b)| ((a * a + b * b).sqrt() * gain).min(1.0))
                .collect();
            Plane { w, h, data }
        })
        .collect();
    let mut e = e;
    if p.shimmer_amp > 0.0 {
        shimmer(&mut e, p);
    }
    if p.show == Show::Edges {
        return Image {
            w,
            h,
            data: (0..n)
                .map(|i| [e[0].data[i], e[1].data[i], e[2].data[i], 1.0])
                .collect(),
        };
    }

    // rays, at a reduced resolution when Rays Res asks for it
    let f = p.res.max(1);
    let (e_small, ws, hs) = if f > 1 {
        (
            e.iter().map(|pl| shrink(pl, f)).collect::<Vec<_>>(),
            w.div_ceil(f),
            h.div_ceil(f),
        )
    } else {
        (e, w, h)
    };
    let c = [
        (p.center[0] + 0.5) / f as f32 - 0.5,
        (p.center[1] + 0.5) / f as f32 - 0.5,
    ];
    let same = p.length_rgb[0] == p.length_rgb[1] && p.length_rgb[1] == p.length_rgb[2];
    let mut rays: Vec<Plane> = (0..3).map(|_| Plane::new(ws, hs)).collect();
    for k in 0..3 {
        if same && k > 0 {
            break;
        }
        let len = p.length * p.length_rgb[k];
        let planes: Vec<&Plane> = if same {
            e_small.iter().collect()
        } else {
            vec![&e_small[k]]
        };
        let rows: Vec<Vec<[f32; 3]>> = (0..hs)
            .into_par_iter()
            .map(|y| {
                (0..ws)
                    .map(|x| {
                        ray_at_multi(&planes, x as f32, y as f32, c, len, p.reverse, p.bias_outer)
                    })
                    .collect()
            })
            .collect();
        for (y, row) in rows.into_iter().enumerate() {
            for (x, v) in row.into_iter().enumerate() {
                if same {
                    for (ch, r) in rays.iter_mut().enumerate() {
                        r.data[y * ws + x] = v[ch];
                    }
                } else {
                    rays[k].data[y * ws + x] = v[0];
                }
            }
        }
    }
    let mut rays: Vec<Plane> = if f > 1 {
        rays.iter().map(|pl| grow(pl, f, w, h)).collect()
    } else {
        rays
    };
    if p.blur > 0.0 {
        for pl in rays.iter_mut() {
            box3(
                pl,
                p.blur * p.blur_rel[0],
                p.blur * p.blur_rel[1],
                Edge::Reflect,
                true,
            );
        }
    }

    let gains = [
        p.brightness * p.color[0],
        p.brightness * p.color[1],
        p.brightness * p.color[2],
    ];
    let data = (0..n)
        .into_par_iter()
        .map(|i| {
            let r = [rays[0].data[i], rays[1].data[i], rays[2].data[i]];
            if p.show == Show::Rays {
                let g = [r[0] * gains[0], r[1] * gains[1], r[2] * gains[2]];
                let a = g[0].max(g[1]).max(g[2]).clamp(0.0, 1.0);
                return if a > 0.0 {
                    [g[0] / a, g[1] / a, g[2] / a, a]
                } else {
                    [0.0; 4]
                };
            }
            if p.dark_rays {
                // Rays pull the picture towards Rays Color: dark colours darken,
                // bright ones brighten (not verified against the reference).
                let s = src.data[i];
                let (sa, sp) = (s[3], [s[0] * s[3], s[1] * s[3], s[2] * s[3]]);
                let mut o = sp;
                for ch in 0..3 {
                    let k = (r[ch] * p.brightness).clamp(0.0, 1.0);
                    o[ch] += (p.color[ch] * sa - sp[ch]) * k;
                }
                return if sa > 1e-6 {
                    [o[0] / sa, o[1] / sa, o[2] / sa, sa]
                } else {
                    [0.0; 4]
                };
            }
            let g = [r[0] * gains[0], r[1] * gains[1], r[2] * gains[2]];
            mix(src.data[i], bg.map(|b| b.data[i]), g, &p.mix)
        })
        .collect();
    Image { w, h, data }
}

/// Ray values for several planes along the same segment (one sampling pass).
fn ray_at_multi(
    planes: &[&Plane],
    px: f32,
    py: f32,
    c: [f32; 2],
    len: f32,
    reverse: f32,
    bias: f32,
) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (k, pl) in planes.iter().enumerate().take(3) {
        out[k] = ray_at(pl, px, py, c, len, reverse, bias);
    }
    out
}

/// Box-average down by `f`.
fn shrink(p: &Plane, f: usize) -> Plane {
    let (ws, hs) = (p.w.div_ceil(f), p.h.div_ceil(f));
    let mut out = Plane::new(ws, hs);
    out.data
        .par_chunks_mut(ws)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let (mut s, mut cnt) = (0.0f32, 0.0f32);
                for yy in y * f..((y + 1) * f).min(p.h) {
                    for xx in x * f..((x + 1) * f).min(p.w) {
                        s += p.data[yy * p.w + xx];
                        cnt += 1.0;
                    }
                }
                *o = s / cnt.max(1.0);
            }
        });
    out
}

/// Bilinear back up by `f` to `w`×`h`.
fn grow(p: &Plane, f: usize, w: usize, h: usize) -> Plane {
    let mut out = Plane::new(w, h);
    let k = 1.0 / f as f32;
    out.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let sx = ((x as f32 + 0.5) * k - 0.5).clamp(0.0, (p.w - 1) as f32);
            let sy = ((y as f32 + 0.5) * k - 0.5).clamp(0.0, (p.h - 1) as f32);
            *o = bilinear(p, sx, sy);
        }
    });
    out
}

fn hash3(x: i32, y: i32, s: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6b343)
        ^ (y as u32).wrapping_mul(0xd8163841)
        ^ s.wrapping_mul(0xcb1ab31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 16;
    (h & 0xffffff) as f32 / 16777215.0
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let sm = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (sm(fx), sm(fy));
    let a = hash3(xi, yi, seed) + (hash3(xi + 1, yi, seed) - hash3(xi, yi, seed)) * u;
    let b = hash3(xi, yi + 1, seed) + (hash3(xi + 1, yi + 1, seed) - hash3(xi, yi + 1, seed)) * u;
    a + (b - a) * v
}

/// Modulates the edges by a two-octave value noise: E · (1 + amp·(2n − 1)).
fn shimmer(e: &mut [Plane], p: &EdgeRaysParams) {
    let scale = if p.layer_width > 0.0 {
        p.shimmer_freq / p.layer_width
    } else {
        0.0
    };
    let seed = (p.shimmer_seed * 1.0e6) as u32;
    let ox = p.shimmer_shift[0] + p.shimmer_speed[0] * p.time;
    let oy = p.shimmer_shift[1] + p.shimmer_speed[1] * p.time;
    let w = e[0].w;
    let amp = p.shimmer_amp;
    let noise: Vec<f32> = (0..e[0].data.len())
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as f32 * scale + ox, (i / w) as f32 * scale + oy);
            let n = 0.67 * value_noise(x, y, seed)
                + 0.33 * value_noise(x * 2.03, y * 2.03, seed ^ 0x9e37);
            (1.0 + amp * (2.0 * n - 1.0)).max(0.0)
        })
        .collect();
    for pl in e.iter_mut() {
        pl.data
            .par_iter_mut()
            .zip(noise.par_iter())
            .for_each(|(v, m)| *v *= m);
    }
}

impl Default for EdgeRaysParams {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            length: 0.25,
            length_rgb: [1.0; 3],
            reverse: 0.0,
            brightness: 2.0,
            color: [1.0; 3],
            blur: 0.0,
            blur_rel: [1.0, 1.0],
            dark_rays: false,
            bias_outer: 0.0,
            show: Show::Result,
            edge_thickness: 10.56,
            edge_brightness: 1.0,
            edge_subpixel: true,
            shimmer_amp: 0.5,
            shimmer_freq: 40.0,
            shimmer_seed: 0.123,
            shimmer_shift: [0.0; 2],
            shimmer_speed: [0.0; 2],
            time: 0.0,
            layer_width: 0.0,
            res: 1,
            from_alpha: 0.0,
            invert_matte: false,
            mix: Mix {
                combine: Combine::Screen,
                affect_alpha: 1.0,
                under_source: 0.0,
                light_background: 0.0,
                source_opacity: 1.0,
                bg_brightness: 1.0,
                alpha_mode: crate::glow::AlphaMode::Normal,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_profile_like_ae() {
        // work/probe/x1: white band from x = 300, Show = Edges, Edge Brightness 0.25
        let (w, h) = (400, 20);
        let src = Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| {
                    if (300..330).contains(&(i % w)) {
                        [1.0; 4]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    }
                })
                .collect(),
        };
        let row = |t: f32| {
            let p = EdgeRaysParams {
                show: Show::Edges,
                edge_thickness: t,
                edge_brightness: 0.25,
                shimmer_amp: 0.0,
                ..Default::default()
            };
            let out = render(&src, None, None, &p);
            (296..304)
                .map(|x| out.data[10 * w + x][0])
                .collect::<Vec<_>>()
        };
        let close = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-4);
        assert!(
            close(&row(0.0), &[0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0]),
            "{:?}",
            row(0.0)
        );
        assert!(
            close(&row(2.0), &[0.0, 0.0, 0.2, 0.4, 0.4, 0.2, 0.0, 0.0]),
            "{:?}",
            row(2.0)
        );
        let t = row(5.5);
        assert!(
            (t[3] - 0.38752).abs() < 2e-4 && (t[1] - 0.10764).abs() < 2e-4,
            "{t:?}"
        );
    }
}
