//! AE glue for Sharpen, BlurMoCurves and Rays. spec/effects/batch3.md.
use after_effects as ae;
use facet_ae_util as u;
use facet_core::{
    glow::{AlphaMode, Mix},
    motion::{CurvesParams, Transform},
    rays::RaysParams,
    shake::{Frame, Wrap},
    sharpen::SharpenParams,
    Image,
};
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sharpen,
    BlurMoCurves,
    Rays,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Params {
    Matte,
    Background,
    Effect,
    Quality,
    Center,
    Opacity,
    MaskUse,
    BlurMask,
    InvertMask,
    Amount,
    Width,
    DetailSize,
    EdgeThreshold,
    Tiny,
    Small,
    Medium,
    Large,
    Luma,
    Chroma,
    Red,
    Green,
    Blue,
    Z,
    Rotate,
    ShiftX,
    ShiftY,
    Shutter,
    ShutterShift,
    Exposure,
    Brightness,
    WrapX,
    WrapY,
    Subpixel,
    Res,
    Length,
    LengthR,
    LengthG,
    LengthB,
    Reverse,
    Darkness,
    Color,
    DarkColor,
    Bias,
    Blur,
    BlurX,
    BlurY,
    Threshold,
    ThresholdAdd,
    Shimmer,
    ShimmerFreq,
    Seed,
    ShimmerX,
    ShimmerY,
    SpeedX,
    SpeedY,
    AffectAlpha,
    FromAlpha,
    UnderSource,
    SourceOpacity,
    BgBrightness,
    SourceChroma,
}
pub type P<'a> = ae::Parameters<'a, Params>;
const BIG: f32 = 100000.0;
pub fn params_setup(p: &mut P, kind: Kind) -> Result<(), ae::Error> {
    use Params::*;
    p.add(
        Matte,
        if kind == Kind::BlurMoCurves {
            "Mask"
        } else {
            "Matte"
        },
        ae::LayerDef::new(),
    )?;
    let slider = |lo, hi, v| u::slider((lo, BIG), (lo, hi), v, 3);
    let signed = |lo, hi, v| u::slider((-BIG, BIG), (lo, hi), v, 3);
    if kind == Kind::Sharpen {
        p.add(Quality, "Quality", u::popup(&["Best", "Fast"], 1))?;
        p.add(Amount, "Sharpen Amp", signed(0.0, 10.0, 1.0))?;
        p.add(EdgeThreshold, "Edge Threshold", slider(0.0, 1.0, 0.15))?;
        p.add(DetailSize, "Small Detail Size", slider(0.0, 24.0, 2.4))?;
        for (id, name, v) in [
            (Tiny, "Scale Tiny Details", 3.0),
            (Small, "Scale Small Details", 1.5),
            (Medium, "Scale Medium Details", 1.0),
            (Large, "Scale Large Details", 2.0),
        ] {
            p.add(id, name, slider(0.0, 5.0, v))?;
        }
        p.add(Width, "Sharpen Width", slider(0.0, 480.0, 26.88))?;
        for (id, name) in [
            (Luma, "Sharpen Luma"),
            (Chroma, "Sharpen Chroma"),
            (Red, "Sharpen Red"),
            (Green, "Sharpen Green"),
            (Blue, "Sharpen Blue"),
        ] {
            p.add(id, name, signed(0.0, 5.0, 1.0))?;
        }
    } else {
        p.add(
            Effect,
            "Effect",
            if kind == Kind::Rays {
                u::popup(&["Light Rays", "Dark Rays"], 1)
            } else {
                u::popup(&["Transform and Blur", "Blur Only"], 1)
            },
        )?;
        p.add(
            Center,
            "Center XY",
            ae::PointDef::setup(|d| {
                d.set_default((50.0, 50.0));
            }),
        )?;
        if kind == Kind::BlurMoCurves {
            p.add(Z, "Z Dist", slider(0.001, 5.0, 1.0))?;
            p.add(Rotate, "Rotate", signed(-180.0, 180.0, 0.0))?;
            p.add(ShiftX, "Shift X", signed(-960.0, 960.0, 0.0))?;
            p.add(ShiftY, "Shift Y", signed(-960.0, 960.0, 0.0))?;
            p.add(Shutter, "Shutter Duration", slider(0.0, 2.0, 1.0))?;
            p.add(ShutterShift, "Shutter Shift", signed(-1.0, 1.0, 0.0))?;
            p.add(
                Exposure,
                "Exposure Bias",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.5, 3),
            )?;
            p.add(Brightness, "Brightness", slider(0.0, 2.0, 1.0))?;
            p.add(WrapX, "Wrap X", u::popup(&["No", "Tile", "Reflect"], 1))?;
            p.add(WrapY, "Wrap Y", u::popup(&["No", "Tile", "Reflect"], 1))?;
            p.add(Res, "Blur Res", u::popup(&["Full", "Half", "Quarter"], 1))?;
            p.add(Subpixel, "Subpixel", u::checkbox(true))?;
        } else {
            p.add(Background, "Background", ae::LayerDef::new())?;
            p.add(
                Length,
                "Rays Length",
                u::slider((-5.0, 1.0), (-1.0, 1.0), 0.25, 3),
            )?;
            for (id, name) in [
                (LengthR, "Length Red"),
                (LengthG, "Length Green"),
                (LengthB, "Length Blue"),
            ] {
                p.add(id, name, slider(0.0, 2.0, 1.0))?;
            }
            p.add(Reverse, "Reverse Rays", slider(0.0, 2.0, 0.0))?;
            p.add(Brightness, "Rays Brightness", slider(0.0, 20.0, 3.0))?;
            p.add(Darkness, "Rays Darkness", slider(0.0, 20.0, 3.0))?;
            p.add(Color, "Rays Color", u::color([255; 3]))?;
            p.add(DarkColor, "Dark Rays Color", u::color([0; 3]))?;
            p.add(Blur, "Blur Rays", slider(0.0, 480.0, 0.0))?;
            p.add(BlurX, "Blur Rays Rel X", slider(0.0, 2.0, 1.0))?;
            p.add(BlurY, "Blur Rays Rel Y", slider(0.0, 2.0, 1.0))?;
            p.add(
                Bias,
                "Bias Outer Bright",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
            )?;
            p.add(Res, "Rays Res", u::popup(&["Full", "Half", "Quarter"], 1))?;
            p.add(Threshold, "Threshold", slider(0.0, 1.0, 0.5))?;
            p.add(ThresholdAdd, "Threshold Add Color", u::color([0; 3]))?;
            p.add(Shimmer, "Shimmer Amp", slider(0.0, 5.0, 0.5))?;
            p.add(ShimmerFreq, "Shimmer Freq", slider(0.1, 50.0, 40.0))?;
            p.add(Seed, "Shimmer Seed", slider(0.0, 1.0, 0.123))?;
            for (id, name) in [
                (ShimmerX, "Shimmer Shift X"),
                (ShimmerY, "Shimmer Shift Y"),
                (SpeedX, "Shimmer Speed X"),
                (SpeedY, "Shimmer Speed Y"),
            ] {
                p.add(id, name, signed(-1.0, 1.0, 0.0))?;
            }
            p.add(AffectAlpha, "Affect Alpha", slider(0.0, 1.0, 1.0))?;
            p.add(
                FromAlpha,
                "Rays From Alpha",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
            )?;
            p.add(
                UnderSource,
                "Rays Under Source",
                u::slider((0.0, 1.0), (0.0, 1.0), 0.0, 3),
            )?;
            p.add(
                SourceOpacity,
                "Source Opacity",
                u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
            )?;
            p.add(BgBrightness, "Bg Brightness", slider(0.0, 2.0, 1.0))?;
            p.add(SourceChroma, "Use Source Chroma", slider(0.0, 2.0, 1.0))?;
        }
    }
    p.add(
        MaskUse,
        if kind == Kind::Rays {
            "Matte Type"
        } else {
            "Mask Use"
        },
        if kind == Kind::Rays {
            u::popup(&["Luma", "Color", "Alpha"], 1)
        } else {
            u::popup(&["Luma", "Alpha"], 1)
        },
    )?;
    p.add(
        BlurMask,
        if kind == Kind::Rays {
            "Blur Matte"
        } else {
            "Blur Mask"
        },
        slider(0.0, 480.0, if kind == Kind::Rays { 0.0 } else { 12.0 }),
    )?;
    p.add(
        InvertMask,
        if kind == Kind::Rays {
            "Invert Matte"
        } else {
            "Invert Mask"
        },
        u::checkbox(false),
    )?;
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
    let pop = |id| u::get_popup(p, id);
    let opacity = match pop(Opacity) {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    };
    let (dx, dy) = u::downsample(input);
    let fr = Frame {
        w: input.width() as f32 * dx,
        h: input.height() as f32 * dy,
        src_x: src.x,
        src_y: src.y,
    };
    let matte = side[u::ID_MATTE as usize].as_ref();
    let center = || {
        let v = p
            .get(Center)
            .and_then(|p| p.as_point().map(|d| d.value()))
            .unwrap_or((input.width() as f32 * 0.5, input.height() as f32 * 0.5));
        [v.0 * dx, v.1 * dy]
    };
    let mask = || {
        facet_core::warp::warp_mask(
            matte,
            src.img.w,
            src.img.h,
            (pop(MaskUse) == 2).then_some(
                if kind == Kind::Sharpen || opacity == AlphaMode::AllOpaque {
                    0
                } else {
                    3
                },
            ),
            u::get_bool(p, InvertMask),
            f(BlurMask) * dx,
            opacity,
        )
    };
    match kind {
        Kind::Sharpen => {
            let sp = SharpenParams {
                best: pop(Quality) == 1,
                amount: f(Amount),
                luma: f(Luma),
                chroma: f(Chroma),
                rgb: [f(Red), f(Green), f(Blue)],
                width: f(Width) * dx,
                detail_size: f(DetailSize) * dx,
                threshold: f(EdgeThreshold),
                detail_scale: [f(Tiny), f(Small), f(Medium), f(Large)],
                opacity,
            };
            facet_core::sharpen::render(&src.img, mask().as_ref(), &sp)
        }
        Kind::BlurMoCurves => {
            let wrap = |id| match pop(id) {
                2 => Wrap::Tile,
                3 => Wrap::Reflect,
                _ => Wrap::No,
            };
            let tr = |offset: f32| {
                let scale = 1000000u32;
                let seconds = (input.current_time() as f64
                    + offset as f64 * input.time_step() as f64)
                    / input.time_scale() as f64;
                let t = (seconds * scale as f64).round() as i32;
                let at = |id| {
                    p.get_at(id, Some(t), None, Some(scale))
                        .and_then(|p| {
                            p.as_float_slider()
                                .map(|d| facet_core::shake::fixed(d.value() as f32))
                        })
                        .unwrap_or_else(|_| f(id))
                };
                Transform {
                    rotate: at(Rotate),
                    z: at(Z),
                    shift: [at(ShiftX) * dx * 2.0 / fr.w, -at(ShiftY) * dy * 2.0 / fr.w],
                }
            };
            let c = center();
            let cp = CurvesParams {
                transform: pop(Effect) == 1,
                current: tr(0.0),
                from: tr(f(ShutterShift) + f(Shutter) * 0.5),
                to: tr(f(ShutterShift) - f(Shutter) * 0.5),
                center: [
                    (c[0] - fr.w * 0.5) * 2.0 / fr.w,
                    (fr.h * 0.5 - c[1]) * 2.0 / fr.w,
                ],
                bias: f(Exposure),
                brightness: f(Brightness),
                subpixel: u::get_bool(p, Subpixel),
                res: 1usize << ((pop(Res) - 1).clamp(0, 2)),
                wrap: [wrap(WrapX), wrap(WrapY)],
                opacity,
            };
            facet_core::motion::curves(&src.img, mask().as_ref(), &fr, &cp)
        }
        Kind::Rays => {
            let dark = pop(Effect) == 2;
            let c = center();
            let time = input.current_time() as f32 / input.time_scale() as f32;
            let mix = Mix {
                combine: facet_core::composite::Combine::Screen,
                affect_alpha: f(AffectAlpha),
                under_source: f(UnderSource),
                light_background: 0.0,
                source_opacity: f(SourceOpacity),
                bg_brightness: f(BgBrightness),
                alpha_mode: opacity,
            };
            let rp = RaysParams {
                center: [c[0] - 0.5, c[1] - 0.5],
                length: f(Length),
                length_rgb: [f(LengthR), f(LengthG), f(LengthB)],
                reverse: f(Reverse),
                brightness: f(if dark { Darkness } else { Brightness }),
                color: u::get_color(p, if dark { DarkColor } else { Color }),
                dark,
                bias: f(Bias),
                res: 1usize << ((pop(Res) - 1).clamp(0, 2)),
                blur: f(Blur) * dx,
                blur_rel: [f(BlurX), f(BlurY)],
                threshold: f(Threshold),
                threshold_add: u::get_color(p, ThresholdAdd),
                chroma: f(SourceChroma),
                from_alpha: f(FromAlpha),
                matte_type: pop(MaskUse),
                invert_matte: u::get_bool(p, InvertMask),
                blur_matte: f(BlurMask) * dx,
                shimmer: f(Shimmer),
                shimmer_freq: f(ShimmerFreq),
                shimmer_seed: f(Seed),
                shimmer_shift: [
                    f(ShimmerX) + f(SpeedX) * time,
                    f(ShimmerY) + f(SpeedY) * time,
                ],
                time,
                mix,
            };
            facet_core::rays::render(&src.img, side[u::ID_BG as usize].as_ref(), matte, &fr, &rp)
        }
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
            if kind == Kind::Rays {
                side.push((u::layer_param_index(p, Params::Background), u::ID_BG));
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
