use crate::timefx::{Layers, Source, P};
use facet_core::{
    batch5::{self, Context, ControlType, Kind},
    shake::Frame,
    Image,
};
pub fn render(
    effect: &str,
    clip: &Source,
    p: &P,
    layers: &Layers,
    n: i32,
) -> Option<Result<Image, String>> {
    let kind = match effect {
        "F_Hotspots" => Kind::Hotspots,
        "F_PixelSort" => Kind::PixelSort,
        "F_PixelMosh" => Kind::PixelMosh,
        _ => return None,
    };
    let src = clip.frame(n);
    let mut settings = batch5::defaults(kind);
    for c in batch5::controls::controls(kind) {
        let d = c.default;
        let v = match c.ty {
            ControlType::Float => [facet_core::shake::fixed(p.f(c.name, d[0])), 0.0, 0.0, 0.0],
            ControlType::Bool | ControlType::Popup => {
                [p.i(c.name, d[0] as i32) as f32, 0.0, 0.0, 0.0]
            }
            ControlType::Color => {
                let rgb = if let Some(v) = p
                    .run
                    .get("params")
                    .and_then(|v| v.get(c.name))
                    .and_then(|v| v.as_array())
                {
                    [0, 1, 2].map(|i| {
                        ((v.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() / 255.0)
                            as f32
                    })
                } else {
                    crate::fx2::color(p, c.name, [0, 1, 2].map(|i| (d[i] * 255.0).round() as u8))
                };
                [rgb[0], rgb[1], rgb[2], 1.0]
            }
            ControlType::Point => match crate::fx2::expr_vec(p.run, c.name) {
                Some(v) if v.len() >= 2 => [v[0] as f32, v[1] as f32, 0.0, 0.0],
                _ => [
                    d[0] * src.w as f32 / 100.0,
                    d[1] * src.h as f32 / 100.0,
                    0.0,
                    0.0,
                ],
            },
        };
        settings.values.insert(c.name, v);
    }
    let mask = layers
        .get(if kind == Kind::PixelSort {
            "Matte"
        } else {
            "Mask"
        })
        .map(|l| l.frame(n));
    let alt = layers.get("AltFlowData").map(|l| l.frame(n));
    let frames: Vec<_> = batch5::needs(kind, &settings, n as f32, clip.frames_total as f32)
        .into_iter()
        .map(|k| (k, clip.frame(k)))
        .collect();
    let ctx = Context {
        frame: Frame {
            w: src.w as f32,
            h: src.h as f32,
            src_x: 0,
            src_y: 0,
        },
        frame_number: n as f32,
        total_frames: clip.frames_total as f32,
        scale: [1.0; 2],
        mask: mask.as_ref(),
        alt_flow: alt.as_ref(),
        frames: &frames,
    };
    Some(Ok(batch5::render(kind, &src, &settings, &ctx)))
}
