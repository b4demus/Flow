//! Shared AE parameter and frame checkout glue for the remaining four effects.
use after_effects as ae;
pub use batch4::Kind;
use facet_ae_util as u;
use facet_core::{
    batch4::{self, Context, ControlType, Settings},
    shake::Frame,
    Image,
};
mod params;
pub use params::Params;
pub type P<'a> = ae::Parameters<'a, Params>;

pub fn params_setup(p: &mut P, kind: Kind) -> Result<(), ae::Error> {
    p.add(
        Params::Mask,
        if kind == Kind::LensFlare {
            "Occlusion from Layer"
        } else {
            "Mask"
        },
        ae::LayerDef::new(),
    )?;
    for control in batch4::controls::controls(kind) {
        let id = params::id(control.name);
        match control.ty {
            ControlType::Float => {
                p.add(
                    id,
                    control.name,
                    u::slider(
                        (control.min, control.max),
                        (control.min.max(-10.0), control.slider_max.min(control.max)),
                        control.default[0] as f64,
                        if control.name == "Freeze Frames" {
                            0
                        } else {
                            3
                        },
                    ),
                )?;
            }
            ControlType::Bool => {
                p.add(id, control.name, u::checkbox(control.default[0] > 0.0))?;
            }
            ControlType::Popup => {
                p.add(
                    id,
                    control.name,
                    u::popup(control.options, control.default[0] as i32),
                )?;
            }
            ControlType::Color => {
                p.add(
                    id,
                    control.name,
                    u::color(
                        [0, 1, 2]
                            .map(|i| (control.default[i] * 255.0).round().clamp(0.0, 255.0) as u8),
                    ),
                )?;
            }
            ControlType::Point => {
                p.add(
                    id,
                    control.name,
                    ae::PointDef::setup(|d| {
                        d.set_default((control.default[0], control.default[1]));
                    }),
                )?;
            }
        }
    }
    Ok(())
}
fn settings(p: &P, kind: Kind, input: &ae::InData) -> Settings {
    let mut out = Settings::defaults(kind);
    let (dx, dy) = u::downsample(input);
    for control in batch4::controls::controls(kind) {
        let id = params::id(control.name);
        let v = match control.ty {
            ControlType::Float => [facet_core::shake::fixed(u::get_float(p, id)), 0.0, 0.0, 0.0],
            ControlType::Bool => [if u::get_bool(p, id) { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            ControlType::Popup => [u::get_popup(p, id) as f32, 0.0, 0.0, 0.0],
            ControlType::Color => {
                let c = u::get_color(p, id);
                [c[0], c[1], c[2], 1.0]
            }
            ControlType::Point => {
                let d = p
                    .get(id)
                    .and_then(|v| v.as_point().map(|d| d.value()))
                    .unwrap_or((
                        control.default[0] * input.width() as f32 / 100.0,
                        control.default[1] * input.height() as f32 / 100.0,
                    ));
                [d.0 * dx, d.1 * dy, 0.0, 0.0]
            }
        };
        out.values.insert(control.name, v);
    }
    out
}
fn render(
    p: &P,
    input: &ae::InData,
    kind: Kind,
    src: &u::Placed,
    side: &[Option<Image>; 4],
    set: &u::FrameSet,
) -> Image {
    let p = settings(p, kind, input);
    let (n, _) = u::frame_number(input);
    let (dx, dy) = u::downsample(input);
    let total = u::total_frames(input);
    let ctx = Context {
        frame: Frame {
            w: input.width() as f32 * dx,
            h: input.height() as f32 * dy,
            src_x: src.x,
            src_y: src.y,
        },
        time: input.current_time() as f32 / input.time_scale() as f32,
        fps: input.time_scale() as f32 / input.time_step().max(1) as f32,
        frame_number: n,
        total_frames: total,
        old: set.get(batch4::old_frame_number(&p, n, total)),
        previous: set.get((n as i32 - 1).max(0)),
        next: set.get((n as i32 + 1).min((total - 1.0).max(0.0) as i32)),
        mask: side[u::ID_MATTE as usize].as_ref(),
        background: None,
        scale: [dx, dy],
    };
    batch4::render(kind, &src.img, &p, &ctx)
}
pub fn handle_command(
    cmd: ae::Command,
    input: ae::InData,
    mut output: ae::OutData,
    p: &mut P,
    kind: Kind,
    about: &str,
) -> Result<(), ae::Error> {
    match cmd {
        ae::Command::About => output.set_return_msg(about),
        ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
            let settings = settings(p, kind, &input);
            let (n, _) = u::frame_number(&input);
            let need = batch4::needs(kind, &settings, n, u::total_frames(&input));
            u::pre_render_frames(
                &input,
                &mut extra,
                (1e6, 1e6),
                &[(u::layer_param_index(p, Params::Mask), u::ID_MATTE)],
                &need,
            )
        })?,
        ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
            u::smart_render_frames(&extra, |src, side, set| {
                render(p, &input, kind, src, side, set)
            })
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("render", || {
            u::legacy_render(&in_layer, &mut out_layer, |src| {
                render(
                    p,
                    &input,
                    kind,
                    src,
                    &Default::default(),
                    &u::FrameSet { frames: vec![] },
                )
            })
        })?,
        _ => {}
    }
    Ok(())
}
