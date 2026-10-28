//! Host-independent renderer for the fourth batch and its temporal fixtures.
use crate::timefx::{Layers, Source, P};
use facet_core::{
    batch4::{self, Context, ControlType, Kind, Settings},
    shake::Frame,
    Image,
};
pub fn render(
    effect: &str,
    clip: &Source,
    p: &P,
    layers: &Layers,
    frame_n: i32,
) -> Option<Result<Image, String>> {
    let kind = match effect {
        "F_FilmDamage" => Kind::FilmDamage,
        "F_DigitalDamage" => Kind::DigitalDamage,
        "F_FilmEffect" => Kind::FilmEffect,
        "F_LensFlare" => Kind::LensFlare,
        _ => return None,
    };
    let src = clip.frame(frame_n);
    let mut settings = Settings::defaults(kind);
    for control in batch4::controls::controls(kind) {
        let d = control.default;
        let v = match control.ty {
            ControlType::Float => [p.f(control.name, d[0]), 0.0, 0.0, 0.0],
            ControlType::Bool | ControlType::Popup => {
                [p.i(control.name, d[0] as i32) as f32, 0.0, 0.0, 0.0]
            }
            ControlType::Color => {
                let c = crate::fx2::color(
                    p,
                    control.name,
                    [0, 1, 2].map(|i| (d[i] * 255.0).round() as u8),
                );
                [c[0], c[1], c[2], 1.0]
            }
            ControlType::Point => {
                let xy = crate::fx2::expr_vec(p.run, control.name);
                match xy {
                    Some(v) if v.len() >= 2 => [v[0] as f32, v[1] as f32, 0.0, 0.0],
                    _ => [
                        d[0] * src.w as f32 / 100.0,
                        d[1] * src.h as f32 / 100.0,
                        0.0,
                        0.0,
                    ],
                }
            }
        };
        settings.values.insert(control.name, v);
    }
    let old = clip.frame(batch4::old_frame_number(
        &settings,
        frame_n as f32,
        clip.frames_total as f32,
    ));
    let prev = clip.frame((frame_n - 1).max(0));
    let next = clip.frame((frame_n + 1).min(clip.frames_total - 1));
    let mask = layers
        .get(if kind == Kind::LensFlare {
            "Occlusion from Layer"
        } else {
            "Mask"
        })
        .map(|l| l.frame(frame_n));
    let ctx = Context {
        frame: Frame {
            w: src.w as f32,
            h: src.h as f32,
            src_x: 0,
            src_y: 0,
        },
        time: frame_n as f32 / 25.0,
        fps: 25.0,
        frame_number: frame_n as f32,
        total_frames: clip.frames_total as f32,
        old: Some(&old),
        previous: Some(&prev),
        next: Some(&next),
        mask: mask.as_ref(),
        background: None,
        scale: [1.0; 2],
    };
    Some(Ok(batch4::render(kind, &src, &settings, &ctx)))
}
