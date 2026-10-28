//! Glow: threshold -> triple-box spread per channel -> brightness -> combine.
//! Behaviour follows spec/effects/glow-observations.md (measured in AE).

use crate::blur::Edge;
use crate::boxes::box3;
use crate::composite::Combine;
use crate::image::{Image, Plane};
use rayon::prelude::*;

/// How the source alpha is treated (the effects' "Opacity" menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    AllOpaque,
    Normal,
    AsPremult,
}

#[derive(Clone, Debug)]
pub struct GlowParams {
    pub brightness: f32,
    pub color: [f32; 3],
    pub threshold: f32,
    pub threshold_add: [f32; 3],
    /// Glow Width in pixels (already scaled for downsampled renders).
    pub width: f32,
    pub width_xy: [f32; 2],
    pub width_rgb: [f32; 3],
    pub subpixel: bool,
    pub show_threshold: bool,
    pub combine: Combine,
    pub edge: Edge,
    pub affect_alpha: f32,
    pub from_alpha: f32,
    pub under_source: f32,
    pub light_background: f32,
    pub source_opacity: f32,
    pub bg_brightness: f32,
    pub invert_matte: bool,
    pub alpha_mode: AlphaMode,
}

impl Default for GlowParams {
    fn default() -> Self {
        Self {
            brightness: 2.0,
            color: [1.0; 3],
            threshold: 0.5,
            threshold_add: [0.0; 3],
            width: 96.0,
            width_xy: [1.0, 1.0],
            width_rgb: [1.0, 1.2, 1.4],
            subpixel: true,
            show_threshold: false,
            combine: Combine::Screen,
            edge: Edge::Reflect,
            affect_alpha: 1.0,
            from_alpha: 0.0,
            under_source: 0.0,
            light_background: 0.0,
            source_opacity: 1.0,
            bg_brightness: 1.0,
            invert_matte: false,
            alpha_mode: AlphaMode::Normal,
        }
    }
}

impl GlowParams {
    /// Largest spread in pixels along x and y (for requesting extra input).
    pub fn reach(&self) -> (f32, f32) {
        let wc = self.width_rgb.iter().cloned().fold(0.0f32, f32::max);
        let r = self.width.max(0.0) * wc;
        (
            (r * self.width_xy[0].max(0.0)).ceil() + 1.0,
            (r * self.width_xy[1].max(0.0)).ceil() + 1.0,
        )
    }
}

fn glow_source(src: [f32; 4], matte: Option<[f32; 4]>, p: &GlowParams) -> [f32; 3] {
    source_rgb(src, matte, p.alpha_mode, p.from_alpha, p.invert_matte)
}

/// Source colour as the effect sees it: premultiplied (straight colour of
/// transparent pixels must not glow), optionally replaced by alpha, scaled by the matte.
pub(crate) fn source_rgb(
    src: [f32; 4],
    matte: Option<[f32; 4]>,
    mode: AlphaMode,
    from_alpha: f32,
    invert_matte: bool,
) -> [f32; 3] {
    let a = match mode {
        AlphaMode::AllOpaque => 1.0,
        _ => src[3],
    };
    let mut c = match mode {
        AlphaMode::AsPremult | AlphaMode::AllOpaque => [src[0], src[1], src[2]],
        AlphaMode::Normal => [src[0] * a, src[1] * a, src[2] * a],
    };
    if from_alpha > 0.0 {
        for v in &mut c {
            *v += (a - *v) * from_alpha;
        }
    }
    if let Some(m) = matte {
        for (k, v) in c.iter_mut().enumerate() {
            let mv = (m[k] * m[3]).clamp(0.0, 1.0);
            *v *= if invert_matte { 1.0 - mv } else { mv };
        }
    }
    c
}

/// Premultiplied source scaled by Source Opacity, and its alpha.
pub(crate) fn source_layer(s: [f32; 4], mode: AlphaMode, opacity: f32) -> ([f32; 3], f32) {
    let sa = if mode == AlphaMode::AllOpaque {
        1.0
    } else {
        s[3]
    };
    let sp = if mode == AlphaMode::AsPremult {
        [s[0], s[1], s[2]]
    } else {
        [s[0] * sa, s[1] * sa, s[2] * sa]
    };
    (
        [sp[0] * opacity, sp[1] * opacity, sp[2] * opacity],
        sa * opacity,
    )
}

/// Premultiplied Background input scaled by Bg Brightness (transparent when absent).
pub(crate) fn bg_layer(b: Option<[f32; 4]>, brightness: f32) -> ([f32; 3], f32) {
    match b {
        Some(b) => {
            let k = brightness * b[3];
            ([b[0] * k, b[1] * k, b[2] * k], b[3])
        }
        None => ([0.0; 3], 0.0),
    }
}

/// Premultiplied "top over bottom".
#[inline]
pub(crate) fn over(t: [f32; 3], ta: f32, b: [f32; 3], ba: f32) -> ([f32; 3], f32) {
    (
        [
            t[0] + b[0] * (1.0 - ta),
            t[1] + b[1] * (1.0 - ta),
            t[2] + b[2] * (1.0 - ta),
        ],
        ta + ba * (1.0 - ta),
    )
}

/// Premultiplied result to the output pixel (straight unless As Premult).
#[inline]
pub(crate) fn finish(rgb: [f32; 3], a: f32, mode: AlphaMode) -> [f32; 4] {
    let a = a.clamp(0.0, 1.0);
    if a <= 1e-6 {
        [0.0; 4]
    } else if mode == AlphaMode::AsPremult {
        // Results stay premultiplied in this mode.
        [rgb[0], rgb[1], rgb[2], a]
    } else {
        [rgb[0] / a, rgb[1] / a, rgb[2] / a, a]
    }
}

/// One channel through the threshold: below `t` nothing; up to 1 stretched so
/// 1 stays 1; above 1 (HDR) passed through unchanged.
#[inline]
fn cut(v: f32, t: f32) -> f32 {
    if v < t || v <= 0.0 {
        0.0
    } else if v > 1.0 || t >= 1.0 {
        v
    } else {
        (v - t) / (1.0 - t)
    }
}

/// Per-channel cut, summed, redistributed in the pixel's own colour:
/// out_c = c_c / Σc · Σ cut(c_k, t_k).
pub fn threshold(c: [f32; 3], t: f32, add: [f32; 3]) -> [f32; 3] {
    let total: f32 = c.iter().map(|v| v.max(0.0)).sum();
    if total <= 0.0 {
        return [0.0; 3];
    }
    let s: f32 = (0..3).map(|k| cut(c[k], t + add[k])).sum();
    let f = s / total;
    [c[0].max(0.0) * f, c[1].max(0.0) * f, c[2].max(0.0) * f]
}

/// Renders the glow over `src` (straight colour). `bg` and `matte` are optional
/// layer inputs of the same size.
pub fn render(src: &Image, bg: Option<&Image>, matte: Option<&Image>, p: &GlowParams) -> Image {
    let (w, h) = (src.w, src.h);
    let n = w * h;

    // 1. thresholded source
    let thr: Vec<[f32; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            let m = matte.map(|m| m.data[i]);
            threshold(glow_source(src.data[i], m, p), p.threshold, p.threshold_add)
        })
        .collect();
    if p.show_threshold {
        let data = thr.par_iter().map(|c| [c[0], c[1], c[2], 1.0]).collect();
        return Image { w, h, data };
    }

    // 2. spread, one plane per channel
    let mut planes: Vec<Plane> = (0..3)
        .map(|k| Plane {
            w,
            h,
            data: thr.iter().map(|c| c[k]).collect(),
        })
        .collect();
    drop(thr);
    for (k, pl) in planes.iter_mut().enumerate() {
        let r = p.width.max(0.0) * p.width_rgb[k].max(0.0);
        box3(
            pl,
            r * p.width_xy[0].max(0.0),
            r * p.width_xy[1].max(0.0),
            p.edge,
            p.subpixel,
        );
    }

    // 3. composite
    let gain = [
        p.brightness * p.color[0],
        p.brightness * p.color[1],
        p.brightness * p.color[2],
    ];
    let m = p.mix();
    let data = (0..n)
        .into_par_iter()
        .map(|i| {
            let g = [
                planes[0].data[i] * gain[0],
                planes[1].data[i] * gain[1],
                planes[2].data[i] * gain[2],
            ];
            mix(src.data[i], bg.map(|b| b.data[i]), g, &m)
        })
        .collect();
    Image { w, h, data }
}

/// How an effect layer (glow, rays, glints) goes onto source and background;
/// the options are the same across the lighting effects.
#[derive(Clone, Debug)]
pub struct Mix {
    pub combine: Combine,
    pub affect_alpha: f32,
    pub under_source: f32,
    pub light_background: f32,
    pub source_opacity: f32,
    pub bg_brightness: f32,
    pub alpha_mode: AlphaMode,
}

impl GlowParams {
    pub fn mix(&self) -> Mix {
        Mix {
            combine: self.combine,
            affect_alpha: self.affect_alpha,
            under_source: self.under_source,
            light_background: self.light_background,
            source_opacity: self.source_opacity,
            bg_brightness: self.bg_brightness,
            alpha_mode: self.alpha_mode,
        }
    }
}

/// One output pixel from source `s`, optional Background `b` and effect light `g`.
pub fn mix(s: [f32; 4], b: Option<[f32; 4]>, g: [f32; 3], m: &Mix) -> [f32; 4] {
    let (src_p, src_a) = source_layer(s, m.alpha_mode, m.source_opacity);
    // Without a Background input only the source is there.
    let (mut bg_p, bg_a) = bg_layer(b, m.bg_brightness);
    // Light Background: the glow lights the background (b + 2bg), and
    // that much of it no longer goes through the combine. Exact at 0 and 1.
    let l = m.light_background;
    let mut gl = g;
    if l > 0.0 {
        for c in 0..3 {
            bg_p[c] += 2.0 * l * bg_p[c] * g[c];
            gl[c] *= 1.0 - l;
        }
    }
    let ga = (gl[0].max(gl[1]).max(gl[2]).max(0.0) * m.affect_alpha).min(1.0);
    // Mult multiplies the background including its alpha; the other
    // modes add the glow's opacity to it.
    let comb = |b: [f32; 3], ba: f32| {
        let rgb = [
            m.combine.apply(b[0], gl[0]),
            m.combine.apply(b[1], gl[1]),
            m.combine.apply(b[2], gl[2]),
        ];
        (
            rgb,
            if m.combine == Combine::Mult {
                ba * ga
            } else {
                ba + ga * (1.0 - ba)
            },
        )
    };
    let (base, base_a) = over(src_p, src_a, bg_p, bg_a);
    let (mut rgb, mut a) = comb(base, base_a);
    // Glow Under Source: the source goes over the glowing background instead.
    let u = m.under_source;
    if u > 0.0 {
        let (x, xa) = comb(bg_p, bg_a);
        let (o, oa) = over(src_p, src_a, x, xa);
        for c in 0..3 {
            rgb[c] += (o[c] - rgb[c]) * u;
        }
        a += (oa - a) * u;
    }
    finish(rgb, a, m.alpha_mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_matches_measured_ramps() {
        // work/probe/e5.json: ramp to (1, 0.5, 0.25), t = 0.25, f = 0.5994 and 0.999
        let f = 0.5994;
        let o = threshold([f, 0.5 * f, 0.25 * f], 0.25, [0.0; 3]);
        assert!((o[0] - 0.304076).abs() < 1e-4, "{o:?}");
        let f = 0.999;
        let o = threshold([f, 0.5 * f, 0.25 * f], 0.5, [0.0; 3]);
        assert!(
            (o[0] - 0.570286).abs() < 1e-4 && (o[2] - 0.142571).abs() < 1e-4,
            "{o:?}"
        );
        // grey ramp: (v - t) / (1 - t)
        let o = threshold([0.75; 3], 0.5, [0.0; 3]);
        assert!((o[1] - 0.5).abs() < 1e-6);
        // HDR (e7.json): above 1 unchanged; t >= 1 is a hard cut
        assert!((threshold([1.2; 3], 0.5, [0.0; 3])[0] - 1.2).abs() < 1e-6);
        assert_eq!(threshold([1.4; 3], 1.5, [0.0; 3])[0], 0.0);
        assert!((threshold([1.6; 3], 1.5, [0.0; 3])[0] - 1.6).abs() < 1e-6);
    }

    #[test]
    fn no_nan_on_noise() {
        let (w, h) = (640, 360);
        let mut seed = 12345u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 1.6 - 0.2
        };
        let src = Image {
            w,
            h,
            data: (0..w * h).map(|_| [rnd(), rnd(), rnd(), 1.0]).collect(),
        };
        for width in [20.0, 96.0, 115.2] {
            let p = GlowParams {
                width,
                ..Default::default()
            };
            let out = render(&src, None, None, &p);
            let bad = out
                .data
                .iter()
                .position(|px| px.iter().any(|v| !v.is_finite()));
            assert!(bad.is_none(), "width {width}: NaN at {bad:?}");
        }
        // AE asks for tiny tiles when probed with sampleImage: 41x41 world, width 10.
        let src = Image::filled(41, 41, [1.0; 4]);
        let p = GlowParams {
            width: 10.0,
            threshold: 0.0,
            brightness: 1.0,
            combine: Combine::Add,
            ..Default::default()
        };
        let out = render(&src, None, None, &p);
        let bad = out
            .data
            .iter()
            .position(|px| px.iter().any(|v| !v.is_finite()));
        assert!(
            bad.is_none(),
            "tile: NaN at {bad:?} {:?}",
            bad.map(|i| out.data[i])
        );
        // ... and those tiles cut across edges: 41x41 crops of a step at every offset.
        for x0 in 0..60usize {
            let src = Image {
                w: 41,
                h: 41,
                data: (0..41 * 41)
                    .map(|i| {
                        if i % 41 + x0 < 50 {
                            [1.0; 4]
                        } else {
                            [0.0, 0.0, 0.0, 1.0]
                        }
                    })
                    .collect(),
            };
            let out = render(&src, None, None, &p);
            let bad = out
                .data
                .iter()
                .position(|px| px.iter().any(|v| !v.is_finite()));
            assert!(
                bad.is_none(),
                "crop {x0}: NaN at {bad:?} {:?}",
                bad.map(|i| out.data[i])
            );
        }
        // AE case edge_reflect: 400x200, white box over x 0..59, Add, width 40.
        let (w, h) = (400, 200);
        let src = Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| {
                    if i % w < 60 {
                        [1.0; 4]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    }
                })
                .collect(),
        };
        let p = GlowParams {
            width: 40.0,
            threshold: 0.0,
            brightness: 1.0,
            combine: Combine::Add,
            ..Default::default()
        };
        let out = render(&src, None, None, &p);
        let bad = out
            .data
            .iter()
            .position(|px| px.iter().any(|v| !v.is_finite()));
        assert!(
            bad.is_none(),
            "edge case: NaN at {bad:?} {:?}",
            bad.map(|i| out.data[i])
        );
    }

    #[test]
    fn step_edge_like_ae() {
        // e3.json step_w10: Add, threshold 0, brightness 1 -> just past the edge the
        // glow is 0.451247 (red); far inside source 1 + glow 1 = 2.
        let (w, h) = (300, 40);
        let src = Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| {
                    if i % w <= 149 {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    }
                })
                .collect(),
        };
        let p = GlowParams {
            width: 10.0,
            threshold: 0.0,
            brightness: 1.0,
            combine: Combine::Add,
            ..Default::default()
        };
        let out = render(&src, None, None, &p);
        let row = 20 * w;
        assert!(
            (out.data[row + 150][0] - 0.451247).abs() < 1e-4,
            "{:?}",
            out.data[row + 150]
        );
        assert!((out.data[row + 20][0] - 2.0).abs() < 1e-4);
    }
}
