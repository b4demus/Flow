//! F_GlowDarks: dark glows spreading from areas darker than a threshold.
//! Behaviour: core/src/darks.rs, spec/effects/glow-observations.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::darks::{self, DarksParams};
use facet_core::glow::AlphaMode;
use facet_core::Edge;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Darkness,
    Threshold,
    Saturation,
    GlowWidth,
    WidthX,
    WidthY,
    Subpixel,
    FromAlpha,
    UnderSource,
    SourceOpacity,
    BgBrightness,
    Show,
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
            Params::Darkness,
            "Darkness",
            u::slider((0.0, 100.0), (0.0, 2.0), 0.5, 3),
        )?;
        params.add(
            Params::Threshold,
            "Threshold",
            u::slider((0.0, 100.0), (0.0, 1.0), 0.5, 3),
        )?;
        params.add(
            Params::Saturation,
            "Glow Saturation",
            u::slider((-2.0, 8.0), (-2.0, 8.0), 1.0, 3),
        )?;
        params.add(
            Params::GlowWidth,
            "Glow Width",
            u::slider((0.0, 10000.0), (0.0, 1000.0), 240.0, 2),
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
        params.add(Params::Subpixel, "Subpixel", u::checkbox(true))?;
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
            Params::SourceOpacity,
            "Source Opacity",
            u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
        )?;
        params.add(
            Params::BgBrightness,
            "Bg Brightness",
            u::slider((0.0, 100.0), (0.0, 4.0), 1.0, 3),
        )?;
        params.add(Params::Show, "Show", u::popup(&["Result", "Threshold"], 1))?;
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
                out_data.set_return_msg("F_GlowDarks v0.1\rDark glows from dark areas.\rb4demus");
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
                    darks::render(
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
                    darks::render(&src.img, None, None, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

fn read_params(params: &ae::Parameters<Params>, in_data: &InData) -> DarksParams {
    let (dx, dy) = u::downsample(in_data);
    DarksParams {
        darkness: u::get_float(params, Params::Darkness),
        threshold: u::get_float(params, Params::Threshold),
        saturation: u::get_float(params, Params::Saturation),
        width: u::get_float(params, Params::GlowWidth),
        width_xy: [
            u::get_float(params, Params::WidthX) * dx,
            u::get_float(params, Params::WidthY) * dy,
        ],
        subpixel: u::get_bool(params, Params::Subpixel),
        from_alpha: u::get_float(params, Params::FromAlpha),
        under_source: u::get_float(params, Params::UnderSource),
        source_opacity: u::get_float(params, Params::SourceOpacity),
        bg_brightness: u::get_float(params, Params::BgBrightness),
        show_threshold: u::get_popup(params, Params::Show) == 2,
        invert_matte: u::get_bool(params, Params::InvertMatte),
        alpha_mode: match u::get_popup(params, Params::Opacity) {
            1 => AlphaMode::AllOpaque,
            3 => AlphaMode::AsPremult,
            _ => AlphaMode::Normal,
        },
        edge: Edge::Reflect,
    }
}
