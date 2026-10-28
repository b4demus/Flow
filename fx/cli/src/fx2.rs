//! Offline side of the second batch of effects (spec/effects/batch2.md) for
//! `facet frames`: parameters by their AE names, points and colours from the
//! probe's "exprs".

use crate::timefx::{Layers, P};
use facet_core::composite::Combine;
use facet_core::glow::AlphaMode;
use facet_core::glow::Mix;
use facet_core::glowedges::{self, GlowEdgesParams};
use facet_core::shake::{Frame, Wrap};
use facet_core::stylize::{self, DuoToneParams, VignetteCombine, VignetteParams};
use facet_core::warp::{self, DistortRgbParams, WarpChromaParams, WarpTransformParams};
use facet_core::Image;
use serde_json::Value;

/// A point or colour parameter given as an expression "[a,b,...]" in the probe.
pub fn expr_vec(run: &Value, name: &str) -> Option<Vec<f64>> {
    let s = run.get("exprs")?.get(name)?.as_str()?;
    let v: Vec<f64> = s
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    (!v.is_empty()).then_some(v)
}

/// Colour parameter: 8-bit like the original; `d` is its default (0..255).
pub fn color(p: &P, name: &str, d: [u8; 3]) -> [f32; 3] {
    match expr_vec(p.run, name) {
        Some(v) => [0, 1, 2].map(|i| {
            ((v.get(i).copied().unwrap_or(0.0) * 255.0)
                .round()
                .clamp(0.0, 255.0)) as f32
                / 255.0
        }),
        None => d.map(|c| c as f32 / 255.0),
    }
}

/// AE point (layer pixels) -> normalised coordinates (x −1..1 across the width, y up).
pub fn point_norm(p: &P, name: &str, w: f64, h: f64) -> (f32, f32) {
    let (px, py) = match expr_vec(p.run, name) {
        Some(v) if v.len() >= 2 => (v[0], v[1]),
        _ => (w * 0.5, h * 0.5),
    };
    (
        ((px - 0.5 * w) * 2.0 / w) as f32,
        ((0.5 * h - py) * 2.0 / w) as f32,
    )
}

pub fn render(
    effect: &str,
    src: &Image,
    p: &P,
    layers: &Layers,
    frame_n: i32,
) -> Option<Result<Image, String>> {
    let (w, h) = (src.w as f64, src.h as f64);
    let r = match effect {
        "F_DuoTone" => Ok(stylize::duotone(
            src,
            &DuoToneParams {
                color0: color(p, "Color0", [0, 0, 51]),
                color1: color(p, "Color1", [255, 230, 204]),
                invert: p.i("Invert", 0) != 0,
                threshold: p.f("Threshold", 0.5),
                softness: p.f("Softness", 1.0),
                mix: p.f("Mix With Source", 0.0),
            },
        )),
        "F_Vignette" => {
            let vp = VignetteParams {
                center: point_norm(p, "Center XY", w, h),
                squareness: p.f("Squareness", 0.0),
                radius: p.f("Radius", 0.9),
                rel_height: p.f("Rel Height", 0.75),
                rel_width: p.f("Rel Width", 1.0),
                rotate: p.f("Rotate", 0.0),
                edge_softness: p.f("Edge Softness", 1.0),
                smooth_curve: p.f("Smooth Curve", 0.4),
                color: color(p, "Color", [0, 0, 0]),
                opacity: p.f("Opacity", 1.0),
                source_brightness: p.f("Source Brightness", 1.0),
                combine: match p.i("Combine", 1) {
                    2 => VignetteCombine::Mult,
                    3 => VignetteCombine::Add,
                    4 => VignetteCombine::Screen,
                    5 => VignetteCombine::SubtractInv,
                    6 => VignetteCombine::VignetteOnly,
                    7 => VignetteCombine::VignetteOnlyInv,
                    _ => VignetteCombine::Composite,
                },
            };
            Ok(stylize::vignette(src, &vp, w as f32, h as f32, 0, 0))
        }
        "F_WarpTransform" => {
            let fr = Frame {
                w: w as f32,
                h: h as f32,
                src_x: 0,
                src_y: 0,
            };
            let wp = WarpTransformParams {
                scale: (p.f("Scale X", 1.0), p.f("Scale Y", 1.0)),
                shift: point_norm(p, "Shift XY", w, h),
                z_dist: p.f("Z Dist", 1.0),
                rotate: p.f("Rotate", 0.0),
                swivel: p.f("Swivel", 0.0),
                tilt: p.f("Tilt", 0.0),
                perspective: p.f("Perspective Amount", 1.0),
                shear: (p.f("Shear X", 0.0), p.f("Shear Y", 0.0)),
                wrap: [wrap(p.i("Wrap X", 1)), wrap(p.i("Wrap Y", 1))],
                filter: p.i("Filter", 1) != 0,
                alpha_mode: alpha_mode(p.i("Opacity", 2)),
            };
            Ok(warp::warp_transform(src, &fr, &wp))
        }
        "F_DistortRGB" => {
            let fr = Frame {
                w: w as f32,
                h: h as f32,
                src_x: 0,
                src_y: 0,
            };
            let lens = layers
                .get("Lens")
                .map(|l| l.frame(frame_n))
                .unwrap_or_else(|| src.clone());
            let dp = DistortRgbParams {
                amount: p.f("Amount", 1.0),
                amount_rel: (p.f("Amount Rel X", 1.0), p.f("Amount Rel Y", 1.0)),
                blur_lens: p.f("Blur Lens", 96.0) / 240.0,
                rotate_dir: p.f("Rotate Warp Dir", 0.0),
                warp: [
                    p.f("Warp Red", 0.5),
                    p.f("Warp Green", 0.75),
                    p.f("Warp Blue", 1.0),
                    0.75,
                ],
                wrap: [wrap(p.i("Wrap X", 3)), wrap(p.i("Wrap Y", 3))],
                filter: p.i("Filter", 1) != 0,
                alpha_mode: alpha_mode(p.i("Opacity", 2)),
            };
            let m = layers.get("Matte").map(|l| l.frame(frame_n));
            let mask = warp::warp_mask(
                m.as_ref(),
                src.w,
                src.h,
                (p.i("Matte Use", 1) == 2).then_some(0),
                p.i("Invert Matte", 0) != 0,
                p.f("Blur Matte", 0.0),
                dp.alpha_mode,
            );
            Ok(warp::distort_rgb(src, &lens, mask.as_ref(), &fr, &dp))
        }
        "F_WarpChroma" => {
            let fr = Frame {
                w: w as f32,
                h: h as f32,
                src_x: 0,
                src_y: 0,
            };
            // shifts are in layer pixels (y down)
            let sh =
                |a: &str, b: &str| (p.f(a, 0.0) * 2.0 / w as f32, -p.f(b, 0.0) * 2.0 / w as f32);
            let cp = WarpChromaParams {
                steps: p.f("Steps", 15.0) as i32,
                center: point_norm(p, "Center XY", w, h),
                from_z: p.f("From Z Dist", 0.85),
                from_rotate: p.f("From Rotate", 0.0),
                from_shift: sh("From Shift X", "From Shift Y"),
                to_z: p.f("To Z Dist", 1.0),
                to_rotate: p.f("To Rotate", 0.0),
                to_shift: sh("To Shift X", "To Shift Y"),
                warp_amount: p.f("Warp Amount", 0.5),
                brightness: p.f("Brightness", 1.0),
                colors: [
                    color(p, "Color1", [255, 0, 0]),
                    color(p, "Color2", [0, 255, 0]),
                    color(p, "Color3", [0, 0, 255]),
                ],
                white_balance: p.i("White Balance", 0) != 0,
                filter: p.i("Filter", 0) != 0,
                wrap: [wrap(p.i("Wrap X", 1)), wrap(p.i("Wrap Y", 1))],
                alpha_mode: alpha_mode(p.i("Opacity", 2)),
            };
            let m = layers.get("Matte").map(|l| l.frame(frame_n));
            let mask = warp::warp_mask(
                m.as_ref(),
                src.w,
                src.h,
                (p.i("Matte Use", 1) == 2).then_some(if cp.alpha_mode == AlphaMode::AllOpaque {
                    0
                } else {
                    3
                }),
                p.i("Invert Matte", 0) != 0,
                p.f("Blur Matte", 0.0),
                cp.alpha_mode,
            );
            Ok(warp::warp_chroma(src, mask.as_ref(), &fr, &cp))
        }
        "F_DissolveBlur" => {
            let bg = layers
                .get("Background")
                .map(|l| l.frame(frame_n))
                .or_else(|| {
                    let c = p.run.get("bgLayer")?.as_array()?;
                    Some(Image::filled(
                        src.w,
                        src.h,
                        [
                            c.first()?.as_f64()? as f32,
                            c.get(1)?.as_f64()? as f32,
                            c.get(2)?.as_f64()? as f32,
                            1.0,
                        ],
                    ))
                });
            let amount = if p.i("Auto Trans", 1) == 2 {
                let duration = p.run.get("duration").and_then(Value::as_f64).unwrap_or(4.0) as f32;
                frame_n as f32 / (duration * 25.0 - 1.0).max(1.0)
            } else {
                p.f("Dissolve Percent", 0.0) / 100.0
            };
            let dp = facet_core::transition::DissolveBlurParams {
                amount,
                on_from_bg: p.i("Transition Dir", 1) == 2,
                width: p.f("Blur Amount", 480.0),
                rel_xy: [p.f("Blur Rel X", 1.0), p.f("Blur Rel Y", 0.0)],
                rel_from: p.f("Blur Rel From", 1.0),
                rel_to: p.f("Blur Rel To", 1.0),
                passes: p.i("Blur Filter", 3).clamp(1, 3) as usize,
                alpha_mode: alpha_mode(p.i("Opacity", 2)),
            };
            Ok(facet_core::transition::dissolve_blur(src, bg.as_ref(), &dp))
        }
        "F_GlowEdges" => {
            let bg = layers.get("Background").map(|l| l.frame(frame_n));
            let gp = GlowEdgesParams {
                glow_brightness: p.f("Glow Brightness", 2.0),
                color: color(p, "Color", [255, 255, 255]),
                width: p.f("Glow Width", 53.76),
                width_xy: [p.f("Width X", 1.0), p.f("Width Y", 1.0)],
                width_rgb: [
                    p.f("Width Red", 1.0),
                    p.f("Width Green", 1.2),
                    p.f("Width Blue", 1.4),
                ],
                subpixel: p.i("Subpixel", 1) != 0,
                edges_smooth: p.f("Edges Smooth", 0.0) / 240.0,
                edges_brightness: p.f("Edges Brightness", 1.0),
                edges_threshold: p.f("Edges Threshold", 0.5),
                show_edges: p.i("Show", 1) == 2,
                from_alpha: p.f("Glow From Alpha", 0.0),
                invert_matte: p.i("Invert Matte", 0) != 0,
                mix: Mix {
                    combine: Combine::from_popup(p.i("Combine", 3)),
                    affect_alpha: p.f("Affect Alpha", 1.0),
                    under_source: p.f("Glow Under Source", 0.0),
                    light_background: p.f("Light Background", 0.0),
                    source_opacity: p.f("Source Opacity", 1.0),
                    bg_brightness: p.f("Bg Brightness", 1.0),
                    alpha_mode: alpha_mode(p.i("Opacity", 2)),
                },
            };
            let matte = layers.get("Matte").map(|l| l.frame(frame_n));
            Ok(glowedges::render(
                src,
                bg.as_ref(),
                matte.as_ref(),
                w as f32,
                &gp,
            ))
        }
        _ => return None,
    };
    Some(r)
}

pub fn wrap(v: i32) -> Wrap {
    match v {
        2 => Wrap::Tile,
        3 => Wrap::Reflect,
        _ => Wrap::No,
    }
}

pub fn alpha_mode(v: i32) -> AlphaMode {
    match v {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    }
}
