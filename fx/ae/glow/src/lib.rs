//! F_Glow: glow from areas brighter than a threshold.
//! Parameters and behaviour: spec/effects/glow-family.md, glow-observations.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::glow::{self, AlphaMode, GlowParams};
use facet_core::{composite::Combine, Edge};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Brightness,
    Color,
    Threshold,
    ThresholdAdd,
    GlowWidth,
    WidthX,
    WidthY,
    WidthRed,
    WidthGreen,
    WidthBlue,
    Subpixel,
    Show,
    Combine,
    EdgeMode,
    AffectAlpha,
    FromAlpha,
    UnderSource,
    LightBackground,
    SourceOpacity,
    BgBrightness,
    InvertMatte,
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
        params.add(Params::Background, "Background", ae::LayerDef::new())?;
        params.add(Params::Matte, "Matte", ae::LayerDef::new())?;
        params.add(
            Params::Brightness,
            "Brightness",
            u::slider((0.0, 1000.0), (0.0, 10.0), 2.0, 3),
        )?;
        params.add(Params::Color, "Color", u::color([255, 255, 255]))?;
        params.add(
            Params::Threshold,
            "Threshold",
            u::slider((0.0, 100.0), (0.0, 1.0), 0.5, 3),
        )?;
        params.add(
            Params::ThresholdAdd,
            "Threshold Add Color",
            u::color([0, 0, 0]),
        )?;
        params.add(
            Params::GlowWidth,
            "Glow Width",
            u::slider((0.0, 10000.0), (0.0, 500.0), 96.0, 2),
        )?;
        params.add(
            Params::WidthX,
            "Width X",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::WidthY,
            "Width Y",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::WidthRed,
            "Width Red",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::WidthGreen,
            "Width Green",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.2, 3),
        )?;
        params.add(
            Params::WidthBlue,
            "Width Blue",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.4, 3),
        )?;
        params.add(Params::Subpixel, "Subpixel", u::checkbox(true))?;
        params.add(Params::Show, "Show", u::popup(&["Result", "Threshold"], 1))?;
        params.add(
            Params::Combine,
            "Combine",
            u::popup(&["Mult", "Add", "Screen", "Difference", "Overlay"], 3),
        )?;
        params.add(
            Params::EdgeMode,
            "Edge Mode",
            u::popup(&["Transparent", "Reflect"], 2),
        )?;
        params.add(
            Params::AffectAlpha,
            "Affect Alpha",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::FromAlpha,
            "Glow From Alpha",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::UnderSource,
            "Glow Under Source",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::LightBackground,
            "Light Background",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::SourceOpacity,
            "Source Opacity",
            u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
        )?;
        params.add(
            Params::BgBrightness,
            "Bg Brightness",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(Params::InvertMatte, "Invert Matte", u::checkbox(false))?;
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
                out_data.set_return_msg("F_Glow v0.1\rGlow from bright areas.\rb4demus");
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
                    glow::render(
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
                    glow::render(&src.img, None, None, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

fn read_params(params: &ae::Parameters<Params>, in_data: &InData) -> GlowParams {
    let (dx, dy) = u::downsample(in_data);
    let width = u::get_float(params, Params::GlowWidth);
    GlowParams {
        brightness: u::get_float(params, Params::Brightness),
        color: u::get_color(params, Params::Color),
        threshold: u::get_float(params, Params::Threshold),
        threshold_add: u::get_color(params, Params::ThresholdAdd),
        width,
        // Downsampled previews shrink the spread with the image.
        width_xy: [
            u::get_float(params, Params::WidthX) * dx,
            u::get_float(params, Params::WidthY) * dy,
        ],
        width_rgb: [
            u::get_float(params, Params::WidthRed),
            u::get_float(params, Params::WidthGreen),
            u::get_float(params, Params::WidthBlue),
        ],
        subpixel: u::get_bool(params, Params::Subpixel),
        show_threshold: u::get_popup(params, Params::Show) == 2,
        combine: Combine::from_popup(u::get_popup(params, Params::Combine)),
        edge: if u::get_popup(params, Params::EdgeMode) == 1 {
            Edge::Transparent
        } else {
            Edge::Reflect
        },
        affect_alpha: u::get_float(params, Params::AffectAlpha),
        from_alpha: u::get_float(params, Params::FromAlpha),
        under_source: u::get_float(params, Params::UnderSource),
        light_background: u::get_float(params, Params::LightBackground),
        source_opacity: u::get_float(params, Params::SourceOpacity),
        bg_brightness: u::get_float(params, Params::BgBrightness),
        invert_matte: u::get_bool(params, Params::InvertMatte),
        alpha_mode: match u::get_popup(params, Params::Opacity) {
            1 => AlphaMode::AllOpaque,
            3 => AlphaMode::AsPremult,
            _ => AlphaMode::Normal,
        },
    }
}
