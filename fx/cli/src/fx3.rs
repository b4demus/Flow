use crate::timefx::{Layers, P};
use facet_core::{
    sharpen::{self, SharpenParams},
    warp, Image,
};

pub fn render(
    effect: &str,
    src: &Image,
    p: &P,
    layers: &Layers,
    frame_n: i32,
) -> Option<Result<Image, String>> {
    let mode = crate::fx2::alpha_mode(p.i("Opacity", 2));
    Some(match effect {
        "F_Rays" => {
            let n = crate::fx2::point_norm(p, "Center XY", src.w as f64, src.h as f64);
            let time = frame_n as f32 / 25.0;
            let dark = p.i("Effect", 1) == 2;
            let mix = facet_core::glow::Mix {
                combine: facet_core::composite::Combine::Screen,
                affect_alpha: p.f("Affect Alpha", 1.0),
                under_source: p.f("Rays Under Source", 0.0),
                light_background: 0.0,
                source_opacity: p.f("Source Opacity", 1.0),
                bg_brightness: p.f("Bg Brightness", 1.0),
                alpha_mode: mode,
            };
            let rp = facet_core::rays::RaysParams {
                center: [
                    (n.0 + 1.0) * src.w as f32 * 0.5 - 0.5,
                    (src.h as f32 - n.1 * src.w as f32) * 0.5 - 0.5,
                ],
                length: p.f("Rays Length", 0.25),
                length_rgb: [
                    p.f("Length Red", 1.0),
                    p.f("Length Green", 1.0),
                    p.f("Length Blue", 1.0),
                ],
                reverse: p.f("Reverse Rays", 0.0),
                brightness: p.f(
                    if dark {
                        "Rays Darkness"
                    } else {
                        "Rays Brightness"
                    },
                    3.0,
                ),
                color: crate::fx2::color(p, "Rays Color", if dark { [0; 3] } else { [255; 3] }),
                dark,
                bias: p.f("Bias Outer Bright", 0.0),
                res: 1usize << ((p.i("Rays Res", 1) - 1).clamp(0, 2)),
                blur: p.f("Blur Rays", 0.0),
                blur_rel: [p.f("Blur Rays Rel X", 1.0), p.f("Blur Rays Rel Y", 1.0)],
                threshold: p.f("Threshold", 0.5),
                threshold_add: crate::fx2::color(p, "Threshold Add Color", [0; 3]),
                chroma: p.f("Use Source Chroma", 1.0),
                from_alpha: p.f("Rays From Alpha", 0.0),
                matte_type: p.i("Matte Type", 1),
                invert_matte: p.i("Invert Matte", 0) != 0,
                blur_matte: p.f("Blur Matte", 0.0),
                shimmer: p.f("Shimmer Amp", 0.5),
                shimmer_freq: p.f("Shimmer Freq", 40.0),
                shimmer_seed: p.f("Shimmer Seed", 0.12298584),
                shimmer_shift: [
                    p.f("Shimmer Shift X", 0.0) + p.f("Shimmer Speed X", 0.0) * time,
                    p.f("Shimmer Shift Y", 0.0) + p.f("Shimmer Speed Y", 0.0) * time,
                ],
                time,
                mix,
            };
            let m = layers.get("Matte").map(|l| l.frame(frame_n));
            let b = layers.get("Background").map(|l| l.frame(frame_n));
            Ok(facet_core::rays::render(
                src,
                b.as_ref(),
                m.as_ref(),
                &facet_core::shake::Frame {
                    w: src.w as f32,
                    h: src.h as f32,
                    src_x: 0,
                    src_y: 0,
                },
                &rp,
            ))
        }
        "F_BlurMoCurves" => {
            let time = frame_n as f32 / 25.0;
            let value = |name: &str, d: f32, t: f32| -> f32 {
                let Some(a) = p
                    .run
                    .get("curves")
                    .and_then(|c| c.get(name))
                    .and_then(|v| v.as_array())
                else {
                    return p.f(name, d);
                };
                facet_core::shake::fixed(
                    a.iter()
                        .enumerate()
                        .map(|(i, c)| c.as_f64().unwrap_or(0.0) as f32 * t.powi(i as i32))
                        .sum(),
                )
            };
            let tr = |t: f32| facet_core::motion::Transform {
                rotate: value("Rotate", 0.0, t),
                z: value("Z Dist", 1.0, t),
                shift: [
                    value("Shift X", 0.0, t) * 2.0 / src.w as f32,
                    -value("Shift Y", 0.0, t) * 2.0 / src.w as f32,
                ],
            };
            let duration = p.f("Shutter Duration", 1.0);
            let shift = p.f("Shutter Shift", 0.0);
            let center = crate::fx2::point_norm(p, "Center XY", src.w as f64, src.h as f64);
            let params = facet_core::motion::CurvesParams {
                transform: p.i("Effect", 1) == 1,
                current: tr(time),
                from: tr(time + (shift + duration * 0.5) / 25.0),
                to: tr(time + (shift - duration * 0.5) / 25.0),
                center: [center.0, center.1],
                bias: p.f("Exposure Bias", 0.5),
                brightness: p.f("Brightness", 1.0),
                subpixel: p.i("Subpixel", 1) != 0,
                res: 1usize << ((p.i("Blur Res", 1) - 1).clamp(0, 2)),
                wrap: [
                    crate::fx2::wrap(p.i("Wrap X", 1)),
                    crate::fx2::wrap(p.i("Wrap Y", 1)),
                ],
                opacity: mode,
            };
            let m = layers.get("Mask").map(|l| l.frame(frame_n));
            let mask = warp::warp_mask(
                m.as_ref(),
                src.w,
                src.h,
                (p.i("Mask Use", 1) == 2).then_some(
                    if mode == facet_core::glow::AlphaMode::AllOpaque {
                        0
                    } else {
                        3
                    },
                ),
                p.i("Invert Mask", 0) != 0,
                p.f("Blur Mask", 12.0),
                mode,
            );
            Ok(facet_core::motion::curves(
                src,
                mask.as_ref(),
                &facet_core::shake::Frame {
                    w: src.w as f32,
                    h: src.h as f32,
                    src_x: 0,
                    src_y: 0,
                },
                &params,
            ))
        }
        "F_Sharpen" => {
            let params = SharpenParams {
                best: p.i("Quality", 1) == 1,
                amount: p.f("Sharpen Amp", 1.0),
                luma: p.f("Sharpen Luma", 1.0),
                chroma: p.f("Sharpen Chroma", 1.0),
                rgb: [
                    p.f("Sharpen Red", 1.0),
                    p.f("Sharpen Green", 1.0),
                    p.f("Sharpen Blue", 1.0),
                ],
                width: p.f("Sharpen Width", 26.87999),
                detail_size: p.f("Small Detail Size", 2.399994),
                threshold: p.f("Edge Threshold", 0.1499939),
                detail_scale: [
                    p.f("Scale Tiny Details", 3.0),
                    p.f("Scale Small Details", 1.5),
                    p.f("Scale Medium Details", 1.0),
                    p.f("Scale Large Details", 2.0),
                ],
                opacity: mode,
            };
            let m = layers.get("Matte").map(|l| l.frame(frame_n));
            let mask = warp::warp_mask(
                m.as_ref(),
                src.w,
                src.h,
                (p.i("Mask Use", 1) == 2).then_some(0),
                p.i("Invert Mask", 0) != 0,
                p.f("Blur Mask", 12.0),
                mode,
            );
            Ok(sharpen::render(src, mask.as_ref(), &params))
        }
        _ => return None,
    })
}
