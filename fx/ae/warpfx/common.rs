//! DistortRGB and WarpChroma host glue. spec/effects/warp-rgb.md.
use after_effects as ae;
use facet_ae_util as u;
use facet_core::glow::AlphaMode;
use facet_core::shake::{Frame, Wrap};
use facet_core::warp::{self, DistortRgbParams, WarpChromaParams};
use facet_core::Image;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DistortRgb,
    WarpChroma,
}
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    Lens,
    Matte,
    Amount,
    BlurLens,
    RotateDir,
    WarpRed,
    WarpGreen,
    WarpBlue,
    AmountX,
    AmountY,
    WrapX,
    WrapY,
    Filter,
    BlurMatte,
    InvertMatte,
    MatteUse,
    Opacity,
    Steps,
    Center,
    FromZ,
    FromRotate,
    FromX,
    FromY,
    ToZ,
    ToRotate,
    ToX,
    ToY,
    WarpAmount,
    Brightness,
    Color1,
    Color2,
    Color3,
    WhiteBalance,
}
pub type P<'a> = ae::Parameters<'a, Params>;
const BIG: f32 = 100000.0;
pub fn params_setup(p: &mut P, kind: Kind) -> Result<(), ae::Error> {
    use Params::*;
    if kind == Kind::DistortRgb {
        p.add(Lens, "Lens", ae::LayerDef::new())?;
    }
    p.add(Matte, "Matte", ae::LayerDef::new())?;
    let s = |min, max, def| u::slider((min, BIG), (min, max), def, 3);
    if kind == Kind::DistortRgb {
        p.add(
            Amount,
            "Amount",
            u::slider((-BIG, BIG), (-2.0, 2.0), 1.0, 3),
        )?;
        p.add(BlurLens, "Blur Lens", s(0.0, 480.0, 96.0))?;
        p.add(
            RotateDir,
            "Rotate Warp Dir",
            u::slider((-BIG, BIG), (-180.0, 180.0), 0.0, 2),
        )?;
        for (id, name, v) in [
            (WarpRed, "Warp Red", 0.5),
            (WarpGreen, "Warp Green", 0.75),
            (WarpBlue, "Warp Blue", 1.0),
        ] {
            p.add(id, name, u::slider((-BIG, BIG), (-2.0, 2.0), v, 3))?;
        }
        p.add(AmountX, "Amount Rel X", s(0.0, 4.0, 1.0))?;
        p.add(AmountY, "Amount Rel Y", s(0.0, 4.0, 1.0))?;
    } else {
        p.add(
            Steps,
            "Steps",
            u::slider((3.0, 100.0), (3.0, 20.0), 15.0, 0),
        )?;
        p.add(
            Center,
            "Center XY",
            ae::PointDef::setup(|d| {
                d.set_default((50.0, 50.0));
            }),
        )?;
        p.add(FromZ, "From Z Dist", s(0.001, 10.0, 0.85))?;
        p.add(
            FromRotate,
            "From Rotate",
            u::slider((-BIG, BIG), (-90.0, 90.0), 0.0, 2),
        )?;
        p.add(
            FromX,
            "From Shift X",
            u::slider((-BIG, BIG), (-960.0, 960.0), 0.0, 2),
        )?;
        p.add(
            FromY,
            "From Shift Y",
            u::slider((-BIG, BIG), (-960.0, 960.0), 0.0, 2),
        )?;
        p.add(ToZ, "To Z Dist", s(0.001, 10.0, 1.0))?;
        p.add(
            ToRotate,
            "To Rotate",
            u::slider((-BIG, BIG), (-90.0, 90.0), 0.0, 2),
        )?;
        p.add(
            ToX,
            "To Shift X",
            u::slider((-BIG, BIG), (-960.0, 960.0), 0.0, 2),
        )?;
        p.add(
            ToY,
            "To Shift Y",
            u::slider((-BIG, BIG), (-960.0, 960.0), 0.0, 2),
        )?;
        p.add(WarpAmount, "Warp Amount", s(0.0, 2.0, 0.5))?;
        p.add(Brightness, "Brightness", s(0.0, 2.0, 1.0))?;
        p.add(Color1, "Color1", u::color([255, 0, 0]))?;
        p.add(Color2, "Color2", u::color([0, 255, 0]))?;
        p.add(Color3, "Color3", u::color([0, 0, 255]))?;
        p.add(WhiteBalance, "White Balance", u::checkbox(false))?;
    }
    let wrap = if kind == Kind::DistortRgb { 3 } else { 1 };
    p.add(WrapX, "Wrap X", u::popup(&["No", "Tile", "Reflect"], wrap))?;
    p.add(WrapY, "Wrap Y", u::popup(&["No", "Tile", "Reflect"], wrap))?;
    p.add(Filter, "Filter", u::checkbox(kind == Kind::DistortRgb))?;
    p.add(BlurMatte, "Blur Matte", s(0.0, 48.0, 0.0))?;
    p.add(InvertMatte, "Invert Matte", u::checkbox(false))?;
    p.add(MatteUse, "Matte Use", u::popup(&["Luma", "Alpha"], 1))?;
    p.add(
        Opacity,
        "Opacity",
        u::popup(&["All Opaque", "Normal", "As Premult"], 2),
    )?;
    Ok(())
}
fn render(
    p: &P,
    input: &ae::InData,
    kind: Kind,
    src: &u::Placed,
    side: &[Option<Image>; 4],
) -> Image {
    use Params::*;
    let f = |id| facet_core::shake::fixed(u::get_float(p, id));
    let (dx, dy) = u::downsample(input);
    let fr = Frame {
        w: input.width() as f32 * dx,
        h: input.height() as f32 * dy,
        src_x: src.x,
        src_y: src.y,
    };
    let wrap = |id| match u::get_popup(p, id) {
        2 => Wrap::Tile,
        3 => Wrap::Reflect,
        _ => Wrap::No,
    };
    let alpha_mode = match u::get_popup(p, Opacity) {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    };
    let mask = warp::warp_mask(
        side[u::ID_MATTE as usize].as_ref(),
        src.img.w,
        src.img.h,
        (u::get_popup(p, MatteUse) == 2).then_some(
            if kind == Kind::DistortRgb || alpha_mode == AlphaMode::AllOpaque {
                0
            } else {
                3
            },
        ),
        u::get_bool(p, InvertMatte),
        f(BlurMatte) * dx,
        alpha_mode,
    );
    if kind == Kind::DistortRgb {
        let params = DistortRgbParams {
            amount: f(Amount),
            amount_rel: (f(AmountX), f(AmountY)),
            blur_lens: f(BlurLens) * dx / 240.0,
            rotate_dir: f(RotateDir),
            warp: [f(WarpRed), f(WarpGreen), f(WarpBlue), 0.75],
            wrap: [wrap(WrapX), wrap(WrapY)],
            filter: u::get_bool(p, Filter),
            alpha_mode,
        };
        warp::distort_rgb(
            &src.img,
            side[3].as_ref().unwrap_or(&src.img),
            mask.as_ref(),
            &fr,
            &params,
        )
    } else {
        let (cx, cy) = p
            .get(Center)
            .ok()
            .and_then(|p| p.as_point().ok().map(|v| v.value()))
            .unwrap_or((input.width() as f32 * 0.5, input.height() as f32 * 0.5));
        let sh = |x, y| (f(x) * dx * 2.0 / fr.w, -f(y) * dy * 2.0 / fr.w);
        let params = WarpChromaParams {
            steps: f(Steps).round() as i32,
            center: (
                (cx * dx - fr.w * 0.5) * 2.0 / fr.w,
                (fr.h * 0.5 - cy * dy) * 2.0 / fr.w,
            ),
            from_z: f(FromZ),
            from_rotate: f(FromRotate),
            from_shift: sh(FromX, FromY),
            to_z: f(ToZ),
            to_rotate: f(ToRotate),
            to_shift: sh(ToX, ToY),
            warp_amount: f(WarpAmount),
            brightness: f(Brightness),
            colors: [
                u::get_color(p, Color1),
                u::get_color(p, Color2),
                u::get_color(p, Color3),
            ],
            white_balance: u::get_bool(p, WhiteBalance),
            wrap: [wrap(WrapX), wrap(WrapY)],
            filter: u::get_bool(p, Filter),
            alpha_mode,
        };
        warp::warp_chroma(&src.img, mask.as_ref(), &fr, &params)
    }
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
            let mut side = vec![(u::layer_param_index(p, Params::Matte), u::ID_MATTE)];
            if kind == Kind::DistortRgb {
                side.push((u::layer_param_index(p, Params::Lens), 3));
            }
            u::pre_render(&input, &mut extra, (1e6, 1e6), &side)
        })?,
        ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
            u::smart_render(&extra, |src, side| render(p, &input, kind, src, side))
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("render", || {
            u::legacy_render(&in_layer, &mut out_layer, |src| {
                render(p, &input, kind, src, &Default::default())
            })
        })?,
        _ => {}
    }
    Ok(())
}
