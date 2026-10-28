//! SmartFX controls, full-frame input and temporal checkout for batch five.
use after_effects as ae;
pub use batch5::Kind;
use facet_ae_util as u;
use facet_core::{
    batch5::{self, Context, ControlType, Settings},
    shake::Frame,
    Image,
};
mod params;
pub use params::Params;
pub type P<'a> = ae::Parameters<'a, Params>;

pub fn params_setup(p: &mut P, kind: Kind) -> Result<(), ae::Error> {
    p.add(
        Params::Mask,
        if kind == Kind::PixelSort {
            "Matte"
        } else {
            "Mask"
        },
        ae::LayerDef::new(),
    )?;
    if kind == Kind::PixelMosh {
        p.add(Params::AltFlowData, "AltFlowData", ae::LayerDef::new())?;
    }
    for c in batch5::controls::controls(kind) {
        let id = params::id(c.name);
        match c.ty {
            ControlType::Float => p.add(
                id,
                c.name,
                u::slider(
                    (c.min, c.max),
                    (c.min.max(-360.0), c.slider_max.min(c.max)),
                    c.default[0] as f64,
                    3,
                ),
            )?,
            ControlType::Bool => p.add(id, c.name, u::checkbox(c.default[0] > 0.0))?,
            ControlType::Popup => p.add(id, c.name, u::popup(c.options, c.default[0] as i32))?,
            ControlType::Color => p.add(
                id,
                c.name,
                u::color([0, 1, 2].map(|i| (c.default[i] * 255.0).round().clamp(0.0, 255.0) as u8)),
            )?,
            ControlType::Point => p.add(
                id,
                c.name,
                ae::PointDef::setup(|d| {
                    d.set_default((c.default[0], c.default[1]));
                }),
            )?,
        };
    }
    Ok(())
}
fn settings(p: &P, kind: Kind, input: &ae::InData) -> Settings {
    let mut out = batch5::defaults(kind);
    let (dx, dy) = u::downsample(input);
    for c in batch5::controls::controls(kind) {
        let id = params::id(c.name);
        let v = match c.ty {
            ControlType::Float => [facet_core::shake::fixed(u::get_float(p, id)), 0.0, 0.0, 0.0],
            ControlType::Bool => [if u::get_bool(p, id) { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            ControlType::Popup => [u::get_popup(p, id) as f32, 0.0, 0.0, 0.0],
            ControlType::Color => {
                let rgb = u::get_color(p, id);
                [rgb[0], rgb[1], rgb[2], 1.0]
            }
            ControlType::Point => {
                let xy = p
                    .get(id)
                    .and_then(|v| v.as_point().map(|d| d.value()))
                    .unwrap_or((
                        c.default[0] * input.width() as f32 / 100.0,
                        c.default[1] * input.height() as f32 / 100.0,
                    ));
                [xy.0 * dx, xy.1 * dy, 0.0, 0.0]
            }
        };
        out.values.insert(c.name, v);
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
    let settings = settings(p, kind, input);
    let (n, _) = u::frame_number(input);
    let (dx, dy) = u::downsample(input);
    let ctx = Context {
        frame: Frame {
            w: input.width() as f32 * dx,
            h: input.height() as f32 * dy,
            src_x: src.x,
            src_y: src.y,
        },
        frame_number: n,
        total_frames: u::total_frames(input),
        scale: [dx, dy],
        mask: side[u::ID_MATTE as usize].as_ref(),
        alt_flow: side[u::ID_BG as usize].as_ref(),
        frames: &set.frames,
    };
    batch5::render(kind, &src.img, &settings, &ctx)
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
        ae::Command::SmartPreRender { mut extra } => u::guarded("batch5 pre_render", || {
            let settings = settings(p, kind, &input);
            let (n, _) = u::frame_number(&input);
            let need = batch5::needs(kind, &settings, n, u::total_frames(&input));
            let mut sides = vec![(u::layer_param_index(p, Params::Mask), u::ID_MATTE)];
            if kind == Kind::PixelMosh {
                sides.push((u::layer_param_index(p, Params::AltFlowData), u::ID_BG));
            }
            u::pre_render_frames(&input, &mut extra, (1e6, 1e6), &sides, &need)
        })?,
        ae::Command::SmartRender { extra } => u::guarded("batch5 smart_render", || {
            u::smart_render_frames(&extra, |src, side, set| {
                render(p, &input, kind, src, side, set)
            })
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("batch5 legacy", || {
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
