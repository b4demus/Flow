//! Dark glow: areas darker than the threshold spread darkness around them.
//! Measured in AE (spec/effects/glow-observations.md, "GlowDarks"):
//!   d_c   = max(t − c_c, 0) / t            per channel, no redistribution
//!   G     = triple-box spread of d, radius Glow Width (no per-channel widths)
//!   G'    = lerp(gray(G), G, saturation),  gray = (5R + 8G + 3B) / 16
//!   out_c = base_c · (1 − 2 · Darkness · G'_c)

use crate::blur::Edge;
use crate::boxes::box3;
use crate::glow::{bg_layer, finish, over, source_layer, source_rgb, AlphaMode};
use crate::image::{Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct DarksParams {
    pub darkness: f32,
    pub threshold: f32,
    pub saturation: f32,
    /// Glow Width in pixels.
    pub width: f32,
    pub width_xy: [f32; 2],
    pub subpixel: bool,
    pub from_alpha: f32,
    pub under_source: f32,
    pub source_opacity: f32,
    pub bg_brightness: f32,
    pub show_threshold: bool,
    pub invert_matte: bool,
    pub alpha_mode: AlphaMode,
    pub edge: Edge,
}

impl Default for DarksParams {
    fn default() -> Self {
        Self {
            darkness: 0.5,
            threshold: 0.5,
            saturation: 1.0,
            width: 240.0,
            width_xy: [1.0, 1.0],
            subpixel: true,
            from_alpha: 0.0,
            under_source: 0.0,
            source_opacity: 1.0,
            bg_brightness: 1.0,
            show_threshold: false,
            invert_matte: false,
            alpha_mode: AlphaMode::Normal,
            edge: Edge::Reflect,
        }
    }
}

impl DarksParams {
    pub fn reach(&self) -> (f32, f32) {
        let r = self.width.max(0.0);
        (
            (r * self.width_xy[0].max(0.0)).ceil() + 1.0,
            (r * self.width_xy[1].max(0.0)).ceil() + 1.0,
        )
    }
}

/// Weights of the grey that Glow Saturation pulls towards.
pub const GRAY: [f32; 3] = [5.0 / 16.0, 8.0 / 16.0, 3.0 / 16.0];

#[inline]
pub fn dark_cut(v: f32, t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else {
        ((t - v) / t).max(0.0)
    }
}

pub fn render(src: &Image, bg: Option<&Image>, matte: Option<&Image>, p: &DarksParams) -> Image {
    let (w, h) = (src.w, src.h);
    let n = w * h;
    let thr: Vec<[f32; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            let c = source_rgb(
                src.data[i],
                matte.map(|m| m.data[i]),
                p.alpha_mode,
                p.from_alpha,
                p.invert_matte,
            );
            [
                dark_cut(c[0], p.threshold),
                dark_cut(c[1], p.threshold),
                dark_cut(c[2], p.threshold),
            ]
        })
        .collect();
    if p.show_threshold {
        return Image {
            w,
            h,
            data: thr.par_iter().map(|c| [c[0], c[1], c[2], 1.0]).collect(),
        };
    }

    let mut planes: Vec<Plane> = (0..3)
        .map(|k| Plane {
            w,
            h,
            data: thr.iter().map(|c| c[k]).collect(),
        })
        .collect();
    drop(thr);
    let r = p.width.max(0.0);
    for pl in planes.iter_mut() {
        box3(
            pl,
            r * p.width_xy[0].max(0.0),
            r * p.width_xy[1].max(0.0),
            p.edge,
            p.subpixel,
        );
    }

    let k = 2.0 * p.darkness;
    let data = (0..n)
        .into_par_iter()
        .map(|i| {
            let g = [planes[0].data[i], planes[1].data[i], planes[2].data[i]];
            let gray = GRAY[0] * g[0] + GRAY[1] * g[1] + GRAY[2] * g[2];
            let m = [0, 1, 2].map(|c| 1.0 - k * (gray + (g[c] - gray) * p.saturation));
            let (src_p, src_a) = source_layer(src.data[i], p.alpha_mode, p.source_opacity);
            let (bg_p, bg_a) = bg_layer(bg.map(|b| b.data[i]), p.bg_brightness);
            let dim = |b: [f32; 3]| [b[0] * m[0], b[1] * m[1], b[2] * m[2]];
            let (base, base_a) = over(src_p, src_a, bg_p, bg_a);
            let mut rgb = dim(base);
            let mut a = base_a;
            // Glow Under Source: the source goes over the darkened background instead.
            let u = p.under_source;
            if u > 0.0 {
                if bg.is_some() {
                    let (o, oa) = over(src_p, src_a, dim(bg_p), bg_a);
                    for c in 0..3 {
                        rgb[c] += (o[c] - rgb[c]) * u;
                    }
                    a += (oa - a) * u;
                } else {
                    // Measured (k3.json): colour = lerp(darkened, plain source) scaled
                    // by Source Opacity, alpha = the source's own, not faded.
                    for c in 0..3 {
                        rgb[c] += (src_p[c] - rgb[c]) * u;
                    }
                    a = if p.source_opacity > 0.0 {
                        src_a / p.source_opacity
                    } else {
                        0.0
                    };
                }
            }
            finish(rgb, a, p.alpha_mode)
        })
        .collect();
    Image { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(c: [f32; 3], p: &DarksParams) -> [f32; 4] {
        render(&Image::filled(4, 4, [c[0], c[1], c[2], 1.0]), None, None, p).data[5]
    }

    #[test]
    fn pointwise_like_ae() {
        // work/probe/k1.json, k2.json: Glow Width 0, Darkness 1, Threshold 0.5
        let p = DarksParams {
            width: 0.0,
            darkness: 1.0,
            ..Default::default()
        };
        let f = 0.1998;
        let o = flat([f, 0.5 * f, 0.25 * f], &p);
        assert!(
            (o[0] + 0.04).abs() < 2e-3 && (o[1] + 0.06).abs() < 2e-3,
            "{o:?}"
        );
        let o = flat(
            [f, 0.5 * f, 0.25 * f],
            &DarksParams {
                saturation: 0.0,
                ..p.clone()
            },
        );
        assert!((o[0] + 0.102495).abs() < 1e-4, "{o:?}");
        let o = flat(
            [0.0999; 3],
            &DarksParams {
                threshold: 0.3,
                ..p.clone()
            },
        );
        assert!((o[0] + 0.033367).abs() < 1e-4, "{o:?}");
    }
}
