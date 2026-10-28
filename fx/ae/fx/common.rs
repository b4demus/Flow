//! Shared body of the second batch of facet effects (F_DuoTone, F_Vignette,
//! F_WarpTransform, ...). Every plugin (ae/<effect>/src/lib.rs) includes this
//! file and picks its `Kind`. Behaviour: spec/effects/batch2.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::glow::AlphaMode;
use facet_core::shake::{self, Frame, Wrap};
use facet_core::stylize::{self, DuoToneParams, VignetteCombine, VignetteParams};
use facet_core::warp::{self, WarpTransformParams};
use facet_core::Image;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    DuoTone,
    Vignette,
    WarpTransform,
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    // Duo Tone
    Color1,
    Color0,
    Invert,
    Threshold,
    Softness,
    MixWithSource,
    // Vignette
    CenterXY,
    Squareness,
    Radius,
    RelHeight,
    RelWidth,
    Rotate,
    EdgeSoftness,
    SmoothCurve,
    Color,
    VigOpacity,
    SourceBrightness,
    Combine,
    // Warp Transform
    ScaleX,
    ScaleY,
    ShiftXY,
    ZDist,
    Swivel,
    Tilt,
    PerspectiveAmount,
    ShearX,
    ShearY,
    WrapX,
    WrapY,
    Filter,
    // common
    Opacity,
}

pub type P<'a> = ae::Parameters<'a, Params>;

const BIG: f32 = 100000.0;
const OPACITY: [&str; 3] = ["All Opaque", "Normal", "As Premult"];
const WRAP: [&str; 3] = ["No", "Tile", "Reflect"];

fn point(default_pct: (f32, f32)) -> ae::PointDef<'static> {
    ae::PointDef::setup(|d| {
        d.set_default(default_pct);
    })
}

pub fn params_setup(params: &mut P, kind: Kind) -> Result<(), ae::Error> {
    use Params::*;
    match kind {
        Kind::DuoTone => {
            params.add(Color1, "Color1", u::color([255, 230, 204]))?;
            params.add(Color0, "Color0", u::color([0, 0, 51]))?;
            params.add(Invert, "Invert", u::checkbox(false))?;
            params.add(
                Threshold,
                "Threshold",
                u::slider((-BIG, BIG), (0.0, 1.0), 0.5, 3),
            )?;
            params.add(
                Softness,
                "Softness",
                u::slider((0.001, BIG), (0.001, 1.0), 1.0, 3),
            )?;
            params.add(
                MixWithSource,
                "Mix With Source",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
            )?;
        }
        Kind::Vignette => {
            params.add(CenterXY, "Center XY", point((50.0, 50.0)))?;
            params.add(
                Squareness,
                "Squareness",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
            )?;
            params.add(Radius, "Radius", u::slider((0.0, BIG), (0.0, 2.0), 0.9, 3))?;
            params.add(
                RelHeight,
                "Rel Height",
                u::slider((0.05, BIG), (0.05, 2.0), 0.75, 3),
            )?;
            params.add(
                RelWidth,
                "Rel Width",
                u::slider((0.05, BIG), (0.05, 2.0), 1.0, 3),
            )?;
            params.add(
                Rotate,
                "Rotate",
                u::slider((-BIG, BIG), (-180.0, 180.0), 0.0, 2),
            )?;
            params.add(
                EdgeSoftness,
                "Edge Softness",
                u::slider((0.0, BIG), (0.0, 1.0), 1.0, 3),
            )?;
            params.add(
                SmoothCurve,
                "Smooth Curve",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.4, 3),
            )?;
            params.add(Color, "Color", u::color([0, 0, 0]))?;
            params.add(
                VigOpacity,
                "Opacity",
                u::slider((0.0, BIG), (0.0, 1.0), 1.0, 3),
            )?;
            params.add(
                SourceBrightness,
                "Source Brightness",
                u::slider((0.0, BIG), (0.0, 2.0), 1.0, 3),
            )?;
            params.add(
                Combine,
                "Combine",
                u::popup(
                    &[
                        "Composite",
                        "Mult",
                        "Add",
                        "Screen",
                        "Subtract Inv",
                        "Vignette Only",
                        "Vignette Only Inv",
                    ],
                    1,
                ),
            )?;
        }
        Kind::WarpTransform => {
            params.add(
                ScaleX,
                "Scale X",
                u::slider((-BIG, BIG), (-2.0, 2.0), 1.0, 3),
            )?;
            params.add(
                ScaleY,
                "Scale Y",
                u::slider((-BIG, BIG), (-2.0, 2.0), 1.0, 3),
            )?;
            params.add(ShiftXY, "Shift XY", point((50.0, 50.0)))?;
            params.add(ZDist, "Z Dist", u::slider((0.001, BIG), (0.1, 4.0), 1.0, 3))?;
            params.add(
                Rotate,
                "Rotate",
                u::slider((-BIG, BIG), (-180.0, 180.0), 0.0, 2),
            )?;
            params.add(
                Swivel,
                "Swivel",
                u::slider((-BIG, BIG), (-90.0, 90.0), 0.0, 2),
            )?;
            params.add(Tilt, "Tilt", u::slider((-BIG, BIG), (-90.0, 90.0), 0.0, 2))?;
            params.add(
                PerspectiveAmount,
                "Perspective Amount",
                u::slider((0.25, 4.0), (0.25, 4.0), 1.0, 3),
            )?;
            params.add(
                ShearX,
                "Shear X",
                u::slider((-BIG, BIG), (-2.0, 2.0), 0.0, 3),
            )?;
            params.add(
                ShearY,
                "Shear Y",
                u::slider((-BIG, BIG), (-2.0, 2.0), 0.0, 3),
            )?;
            params.add(WrapX, "Wrap X", u::popup(&WRAP, 1))?;
            params.add(WrapY, "Wrap Y", u::popup(&WRAP, 1))?;
            params.add(Filter, "Filter", u::checkbox(true))?;
            params.add(Opacity, "Opacity", u::popup(&OPACITY, 2))?;
        }
    }
    Ok(())
}

/// Slider value as the original stores it (16.16 fixed).
fn f(params: &P, p: Params) -> f32 {
    shake::fixed(u::get_float(params, p))
}

fn alpha_mode(params: &P) -> AlphaMode {
    match u::get_popup(params, Params::Opacity) {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    }
}

fn wrap(params: &P, p: Params) -> Wrap {
    match u::get_popup(params, p) {
        2 => Wrap::Tile,
        3 => Wrap::Reflect,
        _ => Wrap::No,
    }
}

/// The layer at the current resolution, and where the buffer's (0,0) sits in it.
fn frame(in_data: &ae::InData, src: &u::Placed) -> Frame {
    let (dx, dy) = u::downsample(in_data);
    Frame {
        w: in_data.width() as f32 * dx,
        h: in_data.height() as f32 * dy,
        src_x: src.x,
        src_y: src.y,
    }
}

/// AE point (layer pixels at the current resolution) -> normalised coordinates.
fn point_norm(params: &P, p: Params, fr: &Frame) -> (f32, f32) {
    let (x, y) = params
        .get(p)
        .ok()
        .and_then(|d| d.as_point().ok().map(|v| v.value()))
        .unwrap_or((0.5 * fr.w, 0.5 * fr.h));
    ((x - 0.5 * fr.w) * 2.0 / fr.w, (0.5 * fr.h - y) * 2.0 / fr.w)
}

fn render(params: &P, in_data: &ae::InData, kind: Kind, src: &u::Placed) -> Image {
    use Params::*;
    let fr = frame(in_data, src);
    match kind {
        Kind::DuoTone => stylize::duotone(
            &src.img,
            &DuoToneParams {
                color0: u::get_color(params, Color0),
                color1: u::get_color(params, Color1),
                invert: u::get_bool(params, Invert),
                threshold: f(params, Threshold),
                softness: f(params, Softness),
                mix: f(params, MixWithSource),
            },
        ),
        Kind::Vignette => {
            let vp = VignetteParams {
                center: point_norm(params, CenterXY, &fr),
                squareness: f(params, Squareness),
                radius: f(params, Radius),
                rel_height: f(params, RelHeight),
                rel_width: f(params, RelWidth),
                rotate: f(params, Rotate),
                edge_softness: f(params, EdgeSoftness),
                smooth_curve: f(params, SmoothCurve),
                color: u::get_color(params, Color),
                opacity: f(params, VigOpacity),
                source_brightness: f(params, SourceBrightness),
                combine: match u::get_popup(params, Combine) {
                    2 => VignetteCombine::Mult,
                    3 => VignetteCombine::Add,
                    4 => VignetteCombine::Screen,
                    5 => VignetteCombine::SubtractInv,
                    6 => VignetteCombine::VignetteOnly,
                    7 => VignetteCombine::VignetteOnlyInv,
                    _ => VignetteCombine::Composite,
                },
            };
            stylize::vignette(&src.img, &vp, fr.w, fr.h, src.x, src.y)
        }
        Kind::WarpTransform => {
            let wp = WarpTransformParams {
                scale: (f(params, ScaleX), f(params, ScaleY)),
                shift: point_norm(params, ShiftXY, &fr),
                z_dist: f(params, ZDist),
                rotate: f(params, Rotate),
                swivel: f(params, Swivel),
                tilt: f(params, Tilt),
                perspective: f(params, PerspectiveAmount),
                shear: (f(params, ShearX), f(params, ShearY)),
                wrap: [wrap(params, WrapX), wrap(params, WrapY)],
                filter: u::get_bool(params, Filter),
                alpha_mode: alpha_mode(params),
            };
            warp::warp_transform(&src.img, &fr, &wp)
        }
    }
}

/// How far outside the output the effect reads (pixels).
fn reach(kind: Kind) -> (f32, f32) {
    match kind {
        Kind::WarpTransform => (1e6, 1e6),
        _ => (0.0, 0.0),
    }
}

pub fn handle_command(
    cmd: ae::Command,
    in_data: ae::InData,
    mut out_data: ae::OutData,
    params: &mut P,
    kind: Kind,
    about: &str,
) -> Result<(), ae::Error> {
    match cmd {
        ae::Command::About => out_data.set_return_msg(about),
        ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
            u::pre_render(&in_data, &mut extra, reach(kind), &[])
        })?,
        ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
            u::smart_render(&extra, |src, _| render(params, &in_data, kind, src))
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("render", || {
            u::legacy_render(&in_layer, &mut out_layer, |src| {
                render(params, &in_data, kind, src)
            })
        })?,
        _ => {}
    }
    Ok(())
}
