//! F_Glint: star-shaped glints on bright spots.
//! Behaviour: core/src/glint.rs, spec/effects/glow-observations.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::composite::Combine;
use facet_core::glint::{self, GlintParams, MatteType};
use facet_core::glow::{AlphaMode, Mix};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Brightness,
    HueShift,
    ScaleColors,
    BrightnessX,
    BrightnessY,
    BrightnessDiag1,
    BrightnessDiag2,
    Threshold,
    ThresholdAdd,
    ThresholdBlur,
    Size,
    SizeX,
    SizeY,
    SizeDiag1,
    SizeDiag2,
    SizeRed,
    SizeGreen,
    SizeBlue,
    BlurGlint,
    AffectAlpha,
    FromAlpha,
    UnderSource,
    SourceOpacity,
    BgBrightness,
    InvertMatte,
    MatteType,
    Opacity,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let rel = |d: f64| u::slider((0.0, 100.0), (0.0, 4.0), d, 3);
        params.add(Params::Background, "Background", ae::LayerDef::new())?;
        params.add(Params::Matte, "Matte", ae::LayerDef::new())?;
        params.add(
            Params::Brightness,
            "Brightness",
            u::slider((0.0, 1000.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::HueShift,
            "Hue Shift",
            u::slider((-1.0, 1.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::ScaleColors,
            "Scale Colors",
            u::color([255, 255, 255]),
        )?;
        params.add(Params::BrightnessX, "Brightness X", rel(1.0))?;
        params.add(Params::BrightnessY, "Brightness Y", rel(1.0))?;
        params.add(Params::BrightnessDiag1, "Brightness Diag1", rel(1.0))?;
        params.add(Params::BrightnessDiag2, "Brightness Diag2", rel(1.0))?;
        params.add(
            Params::Threshold,
            "Threshold",
            u::slider((0.0, 100.0), (0.0, 1.0), 0.7, 3),
        )?;
        params.add(
            Params::ThresholdAdd,
            "Threshold Add Color",
            u::color([0, 0, 0]),
        )?;
        params.add(
            Params::ThresholdBlur,
            "Threshold Blur",
            u::slider((0.0, 1000.0), (0.0, 50.0), 5.376, 3),
        )?;
        params.add(
            Params::Size,
            "Size",
            u::slider((0.0, 10000.0), (0.0, 1000.0), 480.0, 2),
        )?;
        params.add(Params::SizeX, "Size X", rel(1.0))?;
        params.add(Params::SizeY, "Size Y", rel(1.0))?;
        params.add(Params::SizeDiag1, "Size Diag1", rel(0.75))?;
        params.add(Params::SizeDiag2, "Size Diag2", rel(0.75))?;
        params.add(Params::SizeRed, "Size Red", rel(0.5))?;
        params.add(Params::SizeGreen, "Size Green", rel(1.0))?;
        params.add(Params::SizeBlue, "Size Blue", rel(1.5))?;
        params.add(
            Params::BlurGlint,
            "Blur Glint",
            u::slider((0.0, 1000.0), (0.0, 50.0), 0.0, 3),
        )?;
        params.add(Params::AffectAlpha, "Affect Alpha", rel(1.0))?;
        params.add(
            Params::FromAlpha,
            "Glint From Alpha",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::UnderSource,
            "Glint Under Source",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::SourceOpacity,
            "Source Opacity",
            u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
        )?;
        params.add(Params::BgBrightness, "Bg Brightness", rel(1.0))?;
        params.add(Params::InvertMatte, "Invert Matte", u::checkbox(false))?;
        params.add(
            Params::MatteType,
            "Matte Type",
            u::popup(&["Luma", "Color", "Alpha"], 1),
        )?;
        params.add(
            Params::Opacity,
            "Opacity",
            u::popup(&["All Opaque", "Normal", "As Premult"], 2),
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg("F_Glint v0.1\rStar glints on bright spots.\rb4demus");
            }
            ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
                let p = read_params(params, &in_data);
                let side = [
                    (u::layer_param_index(params, Params::Background), u::ID_BG),
                    (u::layer_param_index(params, Params::Matte), u::ID_MATTE),
                ];
                u::pre_render(&in_data, &mut extra, p.reach(), &side)
            })?,
            ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
                let p = read_params(params, &in_data);
                u::smart_render(&extra, |src, side| {
                    glint::render(
                        &src.img,
                        side[u::ID_BG as usize].as_ref(),
                        side[u::ID_MATTE as usize].as_ref(),
                        &p,
                    )
                })
            })?,
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => u::guarded("render", || {
                let p = read_params(params, &in_data);
                u::legacy_render(&in_layer, &mut out_layer, |src| {
                    glint::render(&src.img, None, None, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

fn read_params(params: &ae::Parameters<Params>, in_data: &InData) -> GlintParams {
    let (dx, _) = u::downsample(in_data);
    let f = |p| u::get_float(params, p);
    GlintParams {
        brightness: f(Params::Brightness),
        hue_shift: f(Params::HueShift),
        scale_colors: u::get_color(params, Params::ScaleColors),
        brightness_dir: [
            f(Params::BrightnessX),
            f(Params::BrightnessY),
            f(Params::BrightnessDiag1),
            f(Params::BrightnessDiag2),
        ],
        threshold: f(Params::Threshold),
        threshold_add: u::get_color(params, Params::ThresholdAdd),
        threshold_blur: f(Params::ThresholdBlur) * dx,
        size: f(Params::Size) * dx,
        size_dir: [
            f(Params::SizeX),
            f(Params::SizeY),
            f(Params::SizeDiag1),
            f(Params::SizeDiag2),
        ],
        size_rgb: [
            f(Params::SizeRed),
            f(Params::SizeGreen),
            f(Params::SizeBlue),
        ],
        blur_glint: f(Params::BlurGlint) * dx,
        from_alpha: f(Params::FromAlpha),
        invert_matte: u::get_bool(params, Params::InvertMatte),
        matte_type: match u::get_popup(params, Params::MatteType) {
            2 => MatteType::Color,
            3 => MatteType::Alpha,
            _ => MatteType::Luma,
        },
        edges: glint::EDGES,
        mix: Mix {
            combine: Combine::Screen,
            affect_alpha: f(Params::AffectAlpha),
            under_source: f(Params::UnderSource),
            light_background: 0.0,
            source_opacity: f(Params::SourceOpacity),
            bg_brightness: f(Params::BgBrightness),
            alpha_mode: match u::get_popup(params, Params::Opacity) {
                1 => AlphaMode::AllOpaque,
                3 => AlphaMode::AsPremult,
                _ => AlphaMode::Normal,
            },
        },
    }
}
