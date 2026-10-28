//! F_EdgeRays: light beams streaming from the edges of the layer.
//! Behaviour: core/src/edgerays.rs, spec/effects/glow-observations.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::composite::Combine;
use facet_core::edgerays::{self, EdgeRaysParams, Show};
use facet_core::glint::MatteType;
use facet_core::glow::{AlphaMode, Mix};
use facet_core::{Edge, Image};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Matte,
    Center,
    RaysLength,
    LengthRed,
    LengthGreen,
    LengthBlue,
    ReverseRays,
    RaysBrightness,
    BlurRays,
    BlurRaysRelX,
    BlurRaysRelY,
    RaysColor,
    DarkRays,
    BiasOuter,
    RaysRes,
    Show,
    EdgeThickness,
    EdgeBrightness,
    EdgeSubpixel,
    ShimmerAmp,
    ShimmerFreq,
    ShimmerSeed,
    ShimmerShift,
    ShimmerSpeed,
    AffectAlpha,
    FromAlpha,
    UnderSource,
    SourceOpacity,
    BgBrightness,
    MatteType,
    BlurMatte,
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
        let rel = |d: f64| u::slider((0.0, 100.0), (0.0, 4.0), d, 3);
        let point = |x: f32, y: f32| {
            ae::PointDef::setup(move |d| {
                d.set_default((x, y));
            })
        };
        params.add(Params::Background, "Background", ae::LayerDef::new())?;
        params.add(Params::Matte, "Matte", ae::LayerDef::new())?;
        params.add(Params::Center, "Center", point(50.0, 50.0))?;
        params.add(
            Params::RaysLength,
            "Rays Length",
            u::slider((-5.0, 1.0), (-1.0, 1.0), 0.25, 3),
        )?;
        params.add(Params::LengthRed, "Length Red", rel(1.0))?;
        params.add(Params::LengthGreen, "Length Green", rel(1.0))?;
        params.add(Params::LengthBlue, "Length Blue", rel(1.0))?;
        params.add(Params::ReverseRays, "Reverse Rays", rel(0.0))?;
        params.add(
            Params::RaysBrightness,
            "Rays Brightness",
            u::slider((0.0, 1000.0), (0.0, 8.0), 2.0, 3),
        )?;
        params.add(
            Params::BlurRays,
            "Blur Rays",
            u::slider((0.0, 1000.0), (0.0, 50.0), 0.0, 2),
        )?;
        params.add(Params::BlurRaysRelX, "Blur Rays Rel X", rel(1.0))?;
        params.add(Params::BlurRaysRelY, "Blur Rays Rel Y", rel(1.0))?;
        params.add(Params::RaysColor, "Rays Color", u::color([255, 255, 255]))?;
        params.add(Params::DarkRays, "Enable Dark Rays", u::checkbox(false))?;
        params.add(
            Params::BiasOuter,
            "Bias Outer Bright",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::RaysRes,
            "Rays Res",
            u::popup(&["Full", "Half", "Quarter"], 1),
        )?;
        params.add(
            Params::Show,
            "Show",
            u::popup(&["Result", "Edges", "Rays"], 1),
        )?;
        params.add(
            Params::EdgeThickness,
            "Edge Thickness",
            u::slider((0.0, 1000.0), (0.0, 50.0), 10.56, 2),
        )?;
        params.add(Params::EdgeBrightness, "Edge Brightness", rel(1.0))?;
        params.add(Params::EdgeSubpixel, "Edge Subpixel", u::checkbox(true))?;
        params.add(Params::ShimmerAmp, "Shimmer Amp", rel(0.5))?;
        params.add(
            Params::ShimmerFreq,
            "Shimmer Freq",
            u::slider((0.1, 10000.0), (0.1, 200.0), 40.0, 2),
        )?;
        params.add(
            Params::ShimmerSeed,
            "Shimmer Seed",
            u::slider((0.0, 1000.0), (0.0, 1.0), 0.123, 3),
        )?;
        params.add(Params::ShimmerShift, "Shimmer Shift", point(0.0, 0.0))?;
        params.add(Params::ShimmerSpeed, "Shimmer Speed", point(0.0, 0.0))?;
        params.add(Params::AffectAlpha, "Affect Alpha", rel(1.0))?;
        params.add(
            Params::FromAlpha,
            "Rays From Alpha",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::UnderSource,
            "Rays Under Source",
            u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Params::SourceOpacity,
            "Source Opacity",
            u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
        )?;
        params.add(Params::BgBrightness, "Bg Brightness", rel(1.0))?;
        params.add(
            Params::MatteType,
            "Matte Type",
            u::popup(&["Luma", "Color", "Alpha"], 1),
        )?;
        params.add(
            Params::BlurMatte,
            "Blur Matte",
            u::slider((0.0, 1000.0), (0.0, 50.0), 0.0, 2),
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
                out_data.set_return_msg("F_EdgeRays v0.1\rLight beams from the edges.\rb4demus");
            }
            ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
                let side = [
                    (u::layer_param_index(params, Params::Background), u::ID_BG),
                    (u::layer_param_index(params, Params::Matte), u::ID_MATTE),
                ];
                // Rays pull light from anywhere between a pixel and the centre: whole layer.
                u::pre_render(&in_data, &mut extra, (1.0e6, 1.0e6), &side)
            })?,
            ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
                let (mut p, blur_matte, matte_type) = read_params(params, &in_data);
                u::smart_render(&extra, |src, side| {
                    // centre from layer space into the input world, pixel centres on integers
                    p.center = [
                        p.center[0] - src.x as f32 - 0.5,
                        p.center[1] - src.y as f32 - 0.5,
                    ];
                    let matte = side[u::ID_MATTE as usize]
                        .as_ref()
                        .map(|m| prepare_matte(m, matte_type, blur_matte));
                    edgerays::render(
                        &src.img,
                        side[u::ID_BG as usize].as_ref(),
                        matte.as_ref(),
                        &p,
                    )
                })
            })?,
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => u::guarded("render", || {
                let (mut p, _, _) = read_params(params, &in_data);
                u::legacy_render(&in_layer, &mut out_layer, |src| {
                    p.center = [
                        p.center[0] - src.x as f32 - 0.5,
                        p.center[1] - src.y as f32 - 0.5,
                    ];
                    edgerays::render(&src.img, None, None, &p)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}

/// Matte as an RGB multiplier per Matte Type, blurred by Blur Matte.
fn prepare_matte(m: &Image, t: MatteType, blur: f32) -> Image {
    let mut out = m.clone();
    for px in out.data.iter_mut() {
        *px = match t {
            MatteType::Color => *px,
            MatteType::Alpha => [1.0, 1.0, 1.0, px[3]],
            MatteType::Luma => {
                let l = facet_core::image::luma(*px);
                [l, l, l, px[3]]
            }
        };
    }
    if blur > 0.0 {
        for c in 0..4 {
            let mut pl = out.channel(c);
            facet_core::boxes::box3(&mut pl, blur, blur, Edge::Reflect, true);
            out.set_channel(c, &pl);
        }
    }
    out
}

fn point(params: &ae::Parameters<Params>, p: Params) -> (f32, f32) {
    params
        .get(p)
        .and_then(|v| v.as_point().map(|d| d.value()))
        .unwrap_or((0.0, 0.0))
}

fn read_params(
    params: &ae::Parameters<Params>,
    in_data: &InData,
) -> (EdgeRaysParams, f32, MatteType) {
    let (dx, dy) = u::downsample(in_data);
    let f = |p| u::get_float(params, p);
    let c = point(params, Params::Center);
    let shift = point(params, Params::ShimmerShift);
    let speed = point(params, Params::ShimmerSpeed);
    let p = EdgeRaysParams {
        center: [c.0 * dx, c.1 * dy],
        length: f(Params::RaysLength),
        length_rgb: [
            f(Params::LengthRed),
            f(Params::LengthGreen),
            f(Params::LengthBlue),
        ],
        reverse: f(Params::ReverseRays),
        brightness: f(Params::RaysBrightness),
        color: u::get_color(params, Params::RaysColor),
        blur: f(Params::BlurRays) * dx,
        blur_rel: [f(Params::BlurRaysRelX), f(Params::BlurRaysRelY)],
        dark_rays: u::get_bool(params, Params::DarkRays),
        bias_outer: f(Params::BiasOuter),
        show: match u::get_popup(params, Params::Show) {
            2 => Show::Edges,
            3 => Show::Rays,
            _ => Show::Result,
        },
        edge_thickness: f(Params::EdgeThickness) * dx,
        edge_brightness: f(Params::EdgeBrightness),
        edge_subpixel: u::get_bool(params, Params::EdgeSubpixel),
        shimmer_amp: f(Params::ShimmerAmp),
        shimmer_freq: f(Params::ShimmerFreq),
        shimmer_seed: f(Params::ShimmerSeed),
        shimmer_shift: [shift.0, shift.1],
        shimmer_speed: [speed.0, speed.1],
        time: if in_data.time_scale() > 0 {
            in_data.current_time() as f32 / in_data.time_scale() as f32
        } else {
            0.0
        },
        layer_width: in_data.width() as f32 * dx,
        res: match u::get_popup(params, Params::RaysRes) {
            2 => 2,
            3 => 4,
            _ => 1,
        },
        from_alpha: f(Params::FromAlpha),
        invert_matte: u::get_bool(params, Params::InvertMatte),
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
    };
    let matte_type = match u::get_popup(params, Params::MatteType) {
        2 => MatteType::Color,
        3 => MatteType::Alpha,
        _ => MatteType::Luma,
    };
    (p, f(Params::BlurMatte) * dx, matte_type)
}
