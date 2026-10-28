//! Glow Edges: glow from the edges (brightness gradient) of the source,
//! composited like Glow. spec/effects/batch2.md.

use crate::blur::Edge;
use crate::boxes::box3;
use crate::glow::{self, AlphaMode, Mix};
use crate::image::{Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct GlowEdgesParams {
    pub glow_brightness: f32,
    pub color: [f32; 3],
    /// Glow Width in pixels.
    pub width: f32,
    pub width_xy: [f32; 2],
    pub width_rgb: [f32; 3],
    pub subpixel: bool,
    /// Normalised (AE pixels / 240).
    pub edges_smooth: f32,
    pub edges_brightness: f32,
    pub edges_threshold: f32,
    pub show_edges: bool,
    pub from_alpha: f32,
    pub invert_matte: bool,
    pub mix: Mix,
}

/// Gradient magnitude of `m` blurred by `blur` (AE pixels / 240), times `scale`
/// (the original's edge_detect); `fw` = layer width in pixels.
pub fn edge_detect(m: &Plane, blur: f32, scale: f32, fw: f32, subpixel: bool) -> Plane {
    let mut b = m.clone();
    let r = blur * 240.0;
    box3(&mut b, r, r, Edge::Reflect, subpixel);
    let gain = scale * fw * 0.05;
    let (w, h) = (m.w, m.h);
    let mut out = Plane {
        w,
        h,
        data: vec![0.0; w * h],
    };
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            let at = |x: usize, y: usize| b.data[y * w + x];
            for (i, o) in row.iter_mut().enumerate() {
                let gx = if w < 2 {
                    0.0
                } else if i == 0 {
                    (at(1, j) - at(0, j)) * 2.0
                } else if i == w - 1 {
                    (at(w - 1, j) - at(w - 2, j)) * 2.0
                } else {
                    at(i + 1, j) - at(i - 1, j)
                };
                let gy = if h < 2 {
                    0.0
                } else if j == 0 {
                    (at(i, 1) - at(i, 0)) * 2.0
                } else if j == h - 1 {
                    (at(i, h - 1) - at(i, h - 2)) * 2.0
                } else {
                    at(i, j + 1) - at(i, j - 1)
                };
                *o = (gx * gx + gy * gy).sqrt() * gain;
            }
        });
    out
}

pub fn render(
    src: &Image,
    bg: Option<&Image>,
    matte: Option<&Image>,
    fw: f32,
    p: &GlowEdgesParams,
) -> Image {
    let (w, h) = (src.w, src.h);
    let mono: Vec<f32> = src
        .data
        .par_iter()
        .map(|px| {
            let c = glow::source_rgb(*px, None, p.mix.alpha_mode, 0.0, false);
            let a = if p.mix.alpha_mode == AlphaMode::AllOpaque {
                1.0
            } else {
                px[3]
            };
            let m = crate::flicker::mono([c[0], c[1], c[2], 0.0]);
            m + (a - m) * p.from_alpha
        })
        .collect();
    // normalize_scale=4 becomes 8 * AE_pixels / layer_width inside the DLL.
    let smooth_normalized = p.edges_smooth * 1920.0 / fw;
    let e_scale = p.edges_brightness * (4.0 * (smooth_normalized + 0.1));
    let ed = edge_detect(
        &Plane { w, h, data: mono },
        p.edges_smooth,
        e_scale,
        fw,
        p.subpixel,
    );
    let edges: Vec<f32> = ed
        .data
        .iter()
        .map(|v| (v - p.edges_threshold).clamp(0.0, 1.0))
        .collect();
    let mut planes: Vec<Plane> = (0..3)
        .map(|c| Plane {
            w,
            h,
            data: edges
                .iter()
                .enumerate()
                .map(|(i, &e)| {
                    let m = matte
                        .map(|m| (m.data[i][c] * m.data[i][3]).clamp(0.0, 1.0))
                        .unwrap_or(1.0);
                    e * if p.invert_matte { 1.0 - m } else { m }
                })
                .collect(),
        })
        .collect();
    if p.show_edges {
        return Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| [planes[0].data[i], planes[1].data[i], planes[2].data[i], 1.0])
                .collect(),
        };
    }
    for (k, pl) in planes.iter_mut().enumerate() {
        let r = p.width.max(0.0) * p.width_rgb[k].max(0.0);
        box3(
            pl,
            r * p.width_xy[0].max(0.0),
            r * p.width_xy[1].max(0.0),
            Edge::Reflect,
            p.subpixel,
        );
    }
    let gain = [
        p.glow_brightness * p.color[0],
        p.glow_brightness * p.color[1],
        p.glow_brightness * p.color[2],
    ];
    let data = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let g = [
                planes[0].data[i] * gain[0],
                planes[1].data[i] * gain[1],
                planes[2].data[i] * gain[2],
            ];
            glow::mix(src.data[i], bg.map(|b| b.data[i]), g, &p.mix)
        })
        .collect();
    Image { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Combine;

    #[test]
    fn gradient_tracks_normalized_resolution() {
        // The same ramp and proportional smooth radius have the same edge
        // intensity at both resolutions. The old gain table violated this.
        let mut values = Vec::new();
        for w in [320, 640] {
            let src = Image {
                w,
                h: 3,
                data: (0..w * 3)
                    .map(|i| {
                        let v = (i % w) as f32 / w as f32;
                        [v, v, v, 1.0]
                    })
                    .collect(),
            };
            let p = GlowEdgesParams {
                glow_brightness: 2.0,
                color: [1.0; 3],
                width: 0.0,
                width_xy: [1.0; 2],
                width_rgb: [1.0; 3],
                subpixel: true,
                edges_smooth: (w as f32 / 32.0) / 240.0,
                edges_brightness: 1.0,
                edges_threshold: 0.0,
                show_edges: true,
                from_alpha: 0.0,
                invert_matte: false,
                mix: Mix {
                    combine: Combine::Screen,
                    affect_alpha: 1.0,
                    under_source: 0.0,
                    light_background: 0.0,
                    source_opacity: 1.0,
                    bg_brightness: 1.0,
                    alpha_mode: AlphaMode::Normal,
                },
            };
            let out = render(&src, None, None, w as f32, &p);
            let v = out.at(w / 2, 1)[0];
            assert!((v - 0.14).abs() < 2e-5, "width {w}: {v}");
            values.push(v);
        }
        assert!((values[0] - values[1]).abs() < 2e-5);
    }
}
