//! F_GlowRainbow: rainbow bands by distance from bright areas.
//! Behaviour: core/src/rainbow.rs, spec/effects/glow-observations.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::composite::Combine;
use facet_core::glow::{AlphaMode, Mix};
use facet_core::rainbow::{self, RainbowParams};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Brightness,
    Color,
    Saturation,
    Threshold,
    ThresholdAdd,
    GlowWidth,
    Frequency,
    FrequencyRed,
    FrequencyGreen,
    FrequencyBlue,
    Phase,
    PhaseSpeed,
    PhaseRed,
    PhaseGreen,
    PhaseBlue,
    Show,
    Combine,
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
            u::slider((0.0, 1000.0), (0.0, 4.0), 0.8, 3),
        )?;
        params.add(Params::Color, "Color", u::color([255, 255, 255]))?;
        params.add(
            Params::Saturation,
            "Glow Saturation",
            u::slider((-100.0, 100.0), (-2.0, 4.0), 1.0, 3),
        )?;
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
            u::slider((0.0, 10000.0), (0.0, 1000.0), 403.2, 2),
        )?;
        params.add(
            Params::Frequency,
            "Frequency",
            u::slider((0.0, 1000.0), (0.0, 50.0), 12.0, 3),
        )?;
        params.add(
            Params::FrequencyRed,
            "Frequency Red",
            u::slider((0.0, 100.0), (0.0, 2.0), 1.0, 3),
        )?;
        params.add(
            Params::FrequencyGreen,
            "Frequency Green",
            u::slider((0.0, 100.0), (0.0, 2.0), 0.9, 3),
        )?;
        params.add(
            Params::FrequencyBlue,
            "Frequency Blue",
            u::slider((0.0, 100.0), (0.0, 2.0), 0.8, 3),
        )?;
        params.add(
            Params::Phase,
            "Phase",
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::PhaseSpeed,
            "Phase Speed",
            u::slider((-1000.0, 1000.0), (-4.0, 4.0), 1.0, 3),
        )?;
        params.add(
            Params::PhaseRed,
            "Phase Red",
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::PhaseGreen,
            "Phase Green",
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::PhaseBlue,
            "Phase Blue",
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(Params::Show, "Show", u::popup(&["Result", "Threshold"], 1))?;
        params.add(
            Params::Combine,
            "Combine",
            u::popup(&["Mult", "Add", "Screen", "Difference", "Overlay"], 3),
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
                out_data.set_return_msg(
                    "F_GlowRainbow v0.1\rRainbow glows around bright areas.\rb4demus",
                );
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
                    rainbow::render(
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
                    rainbow::render(&src.img, None, None, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

fn read_params(params: &ae::Parameters<Params>, in_data: &InData) -> RainbowParams {
    let (dx, _) = u::downsample(in_data);
    let f = |p| u::get_float(params, p);
    RainbowParams {
        brightness: f(Params::Brightness),
        color: u::get_color(params, Params::Color),
        saturation: f(Params::Saturation),
        threshold: f(Params::Threshold),
        threshold_add: u::get_color(params, Params::ThresholdAdd),
        width: f(Params::GlowWidth) * dx,
        frequency: f(Params::Frequency),
        frequency_rgb: [
            f(Params::FrequencyRed),
            f(Params::FrequencyGreen),
            f(Params::FrequencyBlue),
        ],
        phase: f(Params::Phase),
        phase_speed: f(Params::PhaseSpeed),
        phase_rgb: [
            f(Params::PhaseRed),
            f(Params::PhaseGreen),
            f(Params::PhaseBlue),
        ],
        time: if in_data.time_scale() > 0 {
            in_data.current_time() as f32 / in_data.time_scale() as f32
        } else {
            0.0
        },
        // in_data.width is the full-resolution layer width.
        half_width: in_data.width() as f32 * dx * 0.5,
        show_threshold: u::get_popup(params, Params::Show) == 2,
        from_alpha: f(Params::FromAlpha),
        invert_matte: u::get_bool(params, Params::InvertMatte),
        mix: Mix {
            combine: Combine::from_popup(u::get_popup(params, Params::Combine)),
            affect_alpha: f(Params::AffectAlpha),
            under_source: f(Params::UnderSource),
            light_background: f(Params::LightBackground),
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
