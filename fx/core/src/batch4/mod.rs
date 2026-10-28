//! Film and damage graphs reconstructed from DLL analysis. spec/effects/batch4.md.
pub mod controls;
pub mod digital;
pub mod film;
pub mod flare;
mod jpeg;
mod jpeg_dct;
pub mod noise;
mod scratches;
#[cfg(test)]
mod tests;
use crate::glow::AlphaMode;
use crate::shake::Frame;
use crate::{Image, Plane};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    FilmDamage,
    DigitalDamage,
    FilmEffect,
    LensFlare,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlType {
    Float,
    Bool,
    Popup,
    Color,
    Point,
}
pub struct Control {
    pub name: &'static str,
    pub ty: ControlType,
    pub default: [f32; 4],
    pub options: &'static [&'static str],
    pub min: f32,
    pub max: f32,
    pub slider_max: f32,
}
#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub values: HashMap<&'static str, [f32; 4]>,
}
impl Settings {
    pub fn defaults(kind: Kind) -> Self {
        Self {
            values: controls::controls(kind)
                .iter()
                .map(|c| (c.name, c.default))
                .collect(),
        }
    }
    pub fn f(&self, n: &str) -> f32 {
        self.values.get(n).map(|v| v[0]).unwrap_or(0.0)
    }
    pub fn b(&self, n: &str) -> bool {
        self.f(n) > 0.0
    }
    pub fn c(&self, n: &str) -> [f32; 3] {
        let v = self.values.get(n).copied().unwrap_or([1.0; 4]);
        [v[0], v[1], v[2]]
    }
    /// Points are layer pixel coordinates. Defaults are converted by each host.
    pub fn xy(&self, n: &str) -> [f32; 2] {
        let v = self.values.get(n).copied().unwrap_or([0.0; 4]);
        [v[0], v[1]]
    }
    pub fn mode(&self) -> AlphaMode {
        match self.f("Opacity") as i32 {
            1 => AlphaMode::AllOpaque,
            3 => AlphaMode::AsPremult,
            _ => AlphaMode::Normal,
        }
    }
}
pub struct Context<'a> {
    pub frame: Frame,
    pub time: f32,
    pub fps: f32,
    pub frame_number: f32,
    pub total_frames: f32,
    pub old: Option<&'a Image>,
    pub previous: Option<&'a Image>,
    pub next: Option<&'a Image>,
    pub mask: Option<&'a Image>,
    pub background: Option<&'a Image>,
    /// Pixels per full-resolution pixel, for image-space controls.
    pub scale: [f32; 2],
}
pub fn old_frame_number(p: &Settings, n: f32, total: f32) -> i32 {
    crate::temporal::round_step(n, p.f("Freeze Frames").max(1.0)).clamp(0.0, (total - 1.0).max(0.0))
        as i32
}
pub fn needs(kind: Kind, p: &Settings, n: f32, total: f32) -> Vec<i32> {
    match kind {
        Kind::DigitalDamage if p.b("Freeze") || p.b("Flow") => vec![old_frame_number(p, n, total)],
        Kind::FilmEffect if p.f("Fields") >= 5.0 => vec![
            (n as i32 - 1).max(0),
            (n as i32 + 1).min((total - 1.0).max(0.0) as i32),
        ],
        _ => vec![],
    }
}
pub fn render(kind: Kind, src: &Image, p: &Settings, ctx: &Context) -> Image {
    if kind == Kind::LensFlare {
        return flare::render(src, p, ctx);
    }
    let mode = p.mode();
    let pre = crate::temporal::to_working(src.clone(), mode);
    let mut out = match kind {
        Kind::DigitalDamage => digital::render(&pre, p, ctx),
        Kind::FilmDamage => film::damage(&pre, p, ctx),
        Kind::FilmEffect => film::effect(&pre, p, ctx),
        Kind::LensFlare => unreachable!(),
    };
    let mask: Option<Plane> = crate::warp::warp_mask(
        ctx.mask,
        src.w,
        src.h,
        (p.f("Mask Use") == 2.0).then_some(if mode == AlphaMode::AllOpaque { 0 } else { 3 }),
        p.b("Invert Mask"),
        p.f("Blur Mask") * ctx.scale[0],
        mode,
    );
    if let Some(m) = mask {
        for (i, o) in out.data.iter_mut().enumerate() {
            for c in 0..4 {
                o[c] = pre.data[i][c] + (o[c] - pre.data[i][c]) * m.data[i];
            }
        }
    }
    crate::temporal::from_working(out, mode)
}
pub fn blur(src: &Image, r: [f32; 2], passes: usize) -> Image {
    let mut out = src.clone();
    for c in 0..4 {
        let mut p = src.channel(c);
        crate::boxes::box_passes(&mut p, r[0], r[1], crate::Edge::Reflect, passes);
        out.set_channel(c, &p);
    }
    out
}
pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn rescale(x: f32, lo: f32, hi: f32) -> f32 {
    if lo == hi {
        if x > lo {
            1.0
        } else {
            0.0
        }
    } else {
        ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}
