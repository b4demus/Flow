//! F_GlowEdges. Recovered gradient and graph: spec/effects/batch2.md.
use after_effects as ae;
use facet_ae_util as u;
use facet_core::composite::Combine as Blend;
use facet_core::glow::{AlphaMode, Mix};
use facet_core::glowedges::{self, GlowEdgesParams};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Brightness,
    Color,
    GlowWidth,
    WidthX,
    WidthY,
    WidthRed,
    WidthGreen,
    WidthBlue,
    Subpixel,
    EdgesSmooth,
    EdgesBrightness,
    EdgesThreshold,
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
type P<'a> = ae::Parameters<'a, Params>;
#[derive(Default)]
struct Plugin {}
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, p: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        use Params::*;
        p.add(Background, "Background", ae::LayerDef::new())?;
        p.add(Matte, "Matte", ae::LayerDef::new())?;
        p.add(
            Brightness,
            "Glow Brightness",
            u::slider((0.0, 100000.0), (0.0, 10.0), 2.0, 3),
        )?;
        p.add(Color, "Color", u::color([255, 255, 255]))?;
        p.add(
            GlowWidth,
            "Glow Width",
            u::slider((0.0, 100000.0), (0.0, 480.0), 53.76, 2),
        )?;
        for (id, name, default) in [
            (WidthX, "Width X", 1.0),
            (WidthY, "Width Y", 1.0),
            (WidthRed, "Width Red", 1.0),
            (WidthGreen, "Width Green", 1.2),
            (WidthBlue, "Width Blue", 1.4),
        ] {
            p.add(id, name, u::slider((0.0, 100000.0), (0.0, 4.0), default, 3))?;
        }
        p.add(Subpixel, "Subpixel", u::checkbox(true))?;
        p.add(
            EdgesSmooth,
            "Edges Smooth",
            u::slider((0.0, 100000.0), (0.0, 48.0), 0.0, 2),
        )?;
        p.add(
            EdgesBrightness,
            "Edges Brightness",
            u::slider((0.0, 100000.0), (0.0, 10.0), 1.0, 3),
        )?;
        p.add(
            EdgesThreshold,
            "Edges Threshold",
            u::slider((0.0, 100000.0), (0.0, 1.0), 0.5, 3),
        )?;
        p.add(Show, "Show", u::popup(&["Result", "Edges"], 1))?;
        p.add(
            Combine,
            "Combine",
            u::popup(&["Mult", "Add", "Screen", "Difference", "Overlay"], 3),
        )?;
        for (id, name, default) in [
            (AffectAlpha, "Affect Alpha", 1.0),
            (FromAlpha, "Glow From Alpha", 0.0),
            (UnderSource, "Glow Under Source", 0.0),
            (LightBackground, "Light Background", 0.0),
            (SourceOpacity, "Source Opacity", 1.0),
            (BgBrightness, "Bg Brightness", 1.0),
        ] {
            p.add(
                id,
                name,
                u::slider(
                    (
                        0.0,
                        if matches!(id, AffectAlpha | BgBrightness) {
                            100000.0
                        } else {
                            1.0
                        },
                    ),
                    (0.0, 1.0),
                    default,
                    3,
                ),
            )?;
        }
        p.add(InvertMatte, "Invert Matte", u::checkbox(false))?;
        p.add(
            Opacity,
            "Opacity",
            u::popup(&["All Opaque", "Normal", "As Premult"], 2),
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        input: InData,
        mut output: OutData,
        params: &mut P,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => output
                .set_return_msg("F_GlowEdges v0.1\rGlow from brightness or alpha edges.\rb4demus"),
            ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
                let side = [
                    (u::layer_param_index(params, Params::Background), u::ID_BG),
                    (u::layer_param_index(params, Params::Matte), u::ID_MATTE),
                ];
                u::pre_render(&input, &mut extra, (1e6, 1e6), &side)
            })?,
            ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
                let p = read_params(params, &input);
                let (dx, _) = u::downsample(&input);
                u::smart_render(&extra, |src, side| {
                    glowedges::render(
                        &src.img,
                        side[u::ID_BG as usize].as_ref(),
                        side[u::ID_MATTE as usize].as_ref(),
                        input.width() as f32 * dx,
                        &p,
                    )
                })
            })?,
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => u::guarded("render", || {
                let p = read_params(params, &input);
                let (dx, _) = u::downsample(&input);
                u::legacy_render(&in_layer, &mut out_layer, |src| {
                    glowedges::render(&src.img, None, None, input.width() as f32 * dx, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

fn read_params(p: &P, input: &InData) -> GlowEdgesParams {
    use Params::*;
    let f = |id| facet_core::shake::fixed(u::get_float(p, id));
    let (dx, dy) = u::downsample(input);
    GlowEdgesParams {
        glow_brightness: f(Brightness),
        color: u::get_color(p, Color),
        width: f(GlowWidth),
        width_xy: [f(WidthX) * dx, f(WidthY) * dy],
        width_rgb: [f(WidthRed), f(WidthGreen), f(WidthBlue)],
        subpixel: u::get_bool(p, Subpixel),
        edges_smooth: f(EdgesSmooth) * dx / 240.0,
        edges_brightness: f(EdgesBrightness),
        edges_threshold: f(EdgesThreshold),
        show_edges: u::get_popup(p, Show) == 2,
        from_alpha: f(FromAlpha),
        invert_matte: u::get_bool(p, InvertMatte),
        mix: Mix {
            combine: Blend::from_popup(u::get_popup(p, Combine)),
            affect_alpha: f(AffectAlpha),
            under_source: f(UnderSource),
            light_background: f(LightBackground),
            source_opacity: f(SourceOpacity),
            bg_brightness: f(BgBrightness),
            alpha_mode: match u::get_popup(p, Opacity) {
                1 => AlphaMode::AllOpaque,
                3 => AlphaMode::AsPremult,
                _ => AlphaMode::Normal,
            },
        },
    }
}
