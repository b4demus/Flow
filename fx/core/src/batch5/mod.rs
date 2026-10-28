//! Hotspots, PixelSort and PixelMosh: recovered native graphs and image kernels.
pub mod controls;
mod flow;
mod hotspots;
mod mosh;
mod noise;
mod sort;
pub use crate::batch4::{ControlType, Settings};
use crate::{shake::Frame, Image};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hotspots,
    PixelSort,
    PixelMosh,
}
pub fn defaults(kind: Kind) -> Settings {
    Settings {
        values: controls::controls(kind)
            .iter()
            .map(|c| (c.name, c.default))
            .collect(),
    }
}
pub struct Context<'a> {
    pub frame: Frame,
    pub frame_number: f32,
    pub total_frames: f32,
    pub scale: [f32; 2],
    pub mask: Option<&'a Image>,
    pub alt_flow: Option<&'a Image>,
    pub frames: &'a [(i32, Image)],
}
pub fn needs(kind: Kind, p: &Settings, n: f32, total: f32) -> Vec<i32> {
    if kind == Kind::PixelMosh {
        mosh::needs(p, n, total)
    } else {
        vec![]
    }
}
pub fn render(kind: Kind, src: &Image, p: &Settings, ctx: &Context) -> Image {
    let mode = p.mode();
    let input = crate::temporal::to_working(src.clone(), mode);
    let mut out = match kind {
        Kind::Hotspots => hotspots::render(&input, p, ctx),
        Kind::PixelSort => sort::render(&input, p, ctx),
        Kind::PixelMosh => mosh::render(&input, p, ctx),
    };
    if kind == Kind::Hotspots {
        let mask = crate::warp::warp_mask(
            ctx.mask,
            src.w,
            src.h,
            (p.f("Mask Use") == 2.0).then_some(if mode == crate::glow::AlphaMode::AllOpaque {
                0
            } else {
                3
            }),
            p.b("Invert Mask"),
            p.f("Blur Mask") * ctx.scale[0],
            mode,
        );
        if let Some(mask) = mask {
            for (i, v) in out.data.iter_mut().enumerate() {
                for c in 0..4 {
                    v[c] = input.data[i][c] + (v[c] - input.data[i][c]) * mask.data[i];
                }
            }
        }
    }
    crate::temporal::from_working(out, mode)
}
