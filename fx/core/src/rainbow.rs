//! Rainbow glow: colour bands by distance from the bright areas.
//! Measured in AE (spec/effects/glow-observations.md, "GlowRainbow"):
//!   bright  = (5R + 8G + 3B)/16 > Threshold          (no sub-pixel edges)
//!   dist    = Euclidean distance to the nearest bright pixel (0 inside)
//!   env     = max(0, 1 − dist / Glow Width)
//!   col_c   = ½·(1 − cos 2π(Φ + Φ_c + speed·t − F·F_c · dist / (W/2)))   W = layer width
//!   glow_c  = Brightness · Color_c · lerp(grey, env·col, Saturation)

use crate::darks::GRAY;
use crate::edt;
use crate::glow::{mix, source_rgb, AlphaMode, Mix};
use crate::image::Image;
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct RainbowParams {
    pub brightness: f32,
    pub color: [f32; 3],
    pub saturation: f32,
    pub threshold: f32,
    pub threshold_add: [f32; 3],
    /// Glow Width in pixels.
    pub width: f32,
    pub frequency: f32,
    pub frequency_rgb: [f32; 3],
    pub phase: f32,
    pub phase_speed: f32,
    pub phase_rgb: [f32; 3],
    /// Seconds, for Phase Speed.
    pub time: f32,
    /// Half the layer width in pixels: the distance one Frequency unit spans.
    pub half_width: f32,
    pub show_threshold: bool,
    pub from_alpha: f32,
    pub invert_matte: bool,
    pub mix: Mix,
}

impl RainbowParams {
    pub fn reach(&self) -> (f32, f32) {
        let r = self.width.max(0.0).ceil() + 1.0;
        (r, r)
    }
}

pub fn render(src: &Image, bg: Option<&Image>, matte: Option<&Image>, p: &RainbowParams) -> Image {
    let (w, h) = (src.w, src.h);
    let n = w * h;
    let mode: AlphaMode = p.mix.alpha_mode;
    let level: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| {
            let c = source_rgb(
                src.data[i],
                matte.map(|m| m.data[i]),
                mode,
                p.from_alpha,
                p.invert_matte,
            );
            (0..3)
                .map(|k| GRAY[k] * (c[k] - p.threshold_add[k]))
                .sum::<f32>()
                - p.threshold
        })
        .collect();
    if p.show_threshold {
        return Image {
            w,
            h,
            data: level.par_iter().map(|&v| [v, v, v, 1.0]).collect(),
        };
    }
    let mask: Vec<bool> = level.iter().map(|&v| v > 0.0).collect();
    let d2 = edt::squared(&mask, w, h);

    let base_phase = p.phase + p.phase_speed * p.time;
    let tau = std::f32::consts::TAU;
    let inv_r = if p.width > 0.0 {
        1.0 / p.width
    } else {
        f32::INFINITY
    };
    let per_px = if p.half_width > 0.0 {
        p.frequency / p.half_width
    } else {
        0.0
    };
    let data = (0..n)
        .into_par_iter()
        .map(|i| {
            let dist = d2[i].sqrt();
            let env = if dist == 0.0 {
                1.0
            } else {
                (1.0 - dist * inv_r).max(0.0)
            };
            let mut g = [0.0f32; 3];
            if env > 0.0 {
                for c in 0..3 {
                    let ph = base_phase + p.phase_rgb[c] - per_px * p.frequency_rgb[c] * dist;
                    g[c] = env * 0.5 * (1.0 - (tau * ph).cos());
                }
                let gray = GRAY[0] * g[0] + GRAY[1] * g[1] + GRAY[2] * g[2];
                for c in 0..3 {
                    g[c] = (gray + (g[c] - gray) * p.saturation) * p.brightness * p.color[c];
                }
            }
            mix(src.data[i], bg.map(|b| b.data[i]), g, &p.mix)
        })
        .collect();
    Image { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Combine;

    fn params() -> RainbowParams {
        RainbowParams {
            brightness: 1.0,
            color: [1.0; 3],
            saturation: 1.0,
            threshold: 0.5,
            threshold_add: [0.0; 3],
            width: 30.0,
            frequency: 1.0,
            frequency_rgb: [1.0, 0.9, 0.8],
            phase: 0.1,
            phase_speed: 0.0,
            phase_rgb: [0.0; 3],
            time: 0.0,
            half_width: 200.0,
            show_threshold: false,
            from_alpha: 0.0,
            invert_matte: false,
            mix: Mix {
                combine: Combine::Add,
                affect_alpha: 1.0,
                under_source: 0.0,
                light_background: 0.0,
                source_opacity: 1.0,
                bg_brightness: 1.0,
                alpha_mode: AlphaMode::Normal,
            },
        }
    }

    #[test]
    fn step_like_ae() {
        // work/probe/r3.json step_gray_f1: 400x200, white over x < 200; x = 200, 201, 210
        let (w, h) = (400, 20);
        let src = Image {
            w,
            h,
            data: (0..w * h)
                .map(|i| {
                    if i % w < 200 {
                        [1.0; 4]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    }
                })
                .collect(),
        };
        let out = render(&src, None, None, &params());
        let row = 10 * w;
        for (x, want) in [
            (200, [0.0836, 0.0844, 0.0853]),
            (201, [0.0727, 0.0742, 0.0758]),
            (210, [0.0126, 0.0158, 0.0194]),
        ] {
            let got = out.data[row + x];
            for c in 0..3 {
                assert!(
                    (got[c] - want[c]).abs() < 1e-4,
                    "x {x}: {got:?} vs {want:?}"
                );
            }
        }
    }
}
