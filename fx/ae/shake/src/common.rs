//! Shared by F_Shake and F_DissolveShake (ae/dissolveshake includes
//! this file): parameters, their reading, the render commands.
//! Behaviour: spec/effects/shake.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::shake::{self, AlphaMode, Axis, DissolveParams, Frame, ShakeParams, Wrap};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    // Dissolve Shake only
    Background,
    TransitionDir,
    AutoTrans,
    DissolvePercent,
    DissolveSpeed,
    // Shake only
    Style,
    Phase,
    Stillness,
    TwitchFrequency,
    Drift,
    CenterBias,
    ZDist,
    // both
    Amplitude,
    Frequency,
    MotionBlur,
    MoBlurLength,
    Seed,
    WrapX,
    WrapY,
    XStart,
    XRandAmp,
    XRandFreq,
    XWaveAmp,
    XWaveFreq,
    XPhase,
    XEnd,
    YStart,
    YRandAmp,
    YRandFreq,
    YWaveAmp,
    YWaveFreq,
    YPhase,
    YEnd,
    ZStart,
    ZRandAmp,
    ZRandFreq,
    ZWaveAmp,
    ZWaveFreq,
    ZPhase,
    ZEnd,
    TiltStart,
    TiltRandAmp,
    TiltRandFreq,
    TiltWaveAmp,
    TiltWaveFreq,
    TiltPhase,
    TiltEnd,
    ChStart,
    RedAmp,
    GreenAmp,
    BlueAmp,
    RedPhase,
    GreenPhase,
    BluePhase,
    RgbRandomness,
    RgbFrequency,
    ChEnd,
    Opacity,
}

pub type P<'a> = ae::Parameters<'a, Params>;

/// Distances are shown in pixels; 1 normalised unit = half the layer width
/// (the defaults are the original's, made for a 1920 wide frame).
const PX: f64 = 960.0;

/// Group with Rand/Wave amp, freq and phase of one axis.
fn axis(
    params: &mut P,
    ids: [Params; 7],
    name: &str,
    rand_amp: f64,
    amp_max: (f32, f32),
    prec: i16,
) -> Result<(), ae::Error> {
    params.add_group(ids[0], ids[6], &format!("{name} Shake"), true, |p| {
        p.add(
            ids[1],
            &format!("{name} Rand Amp"),
            u::slider((0.0, 100000.0), amp_max, rand_amp, prec),
        )?;
        p.add(
            ids[2],
            &format!("{name} Rand Freq"),
            u::slider((0.0, 1000.0), (0.0, 5.0), 1.0, 3),
        )?;
        p.add(
            ids[3],
            &format!("{name} Wave Amp"),
            u::slider((0.0, 100000.0), amp_max, 0.0, prec),
        )?;
        p.add(
            ids[4],
            &format!("{name} Wave Freq"),
            u::slider((0.0, 1000.0), (0.0, 5.0), 0.5, 3),
        )?;
        p.add(
            ids[5],
            &format!("{name} Phase"),
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        Ok(())
    })
}

pub fn params_setup(params: &mut P, dissolve: bool) -> Result<(), ae::Error> {
    use Params::*;
    let d = if dissolve {
        shake::dissolve_shake_defaults()
    } else {
        ShakeParams::default()
    };
    if dissolve {
        params.add(Background, "Background", ae::LayerDef::new())?;
        params.add(
            TransitionDir,
            "Transition Dir",
            u::popup(&["Dissolve Off to Bg", "Dissolve On from Bg"], 1),
        )?;
        params.add(
            AutoTrans,
            "Auto Trans",
            u::popup(&["No: use Dissolve%", "Yes: ignore Dissolve%"], 1),
        )?;
        params.add(
            DissolvePercent,
            "Dissolve Percent",
            u::slider((0.0, 100.0), (0.0, 100.0), 0.0, 2),
        )?;
        params.add(
            DissolveSpeed,
            "Dissolve Speed",
            u::slider((1.0, 10000.0), (1.0, 10.0), 3.0, 3),
        )?;
    } else {
        params.add(Style, "Style", u::popup(&["Normal", "Twitchy", "Jumpy"], 1))?;
    }
    params.add(
        Amplitude,
        "Amplitude",
        u::slider((0.0, 1000.0), (0.0, 4.0), d.amplitude as f64, 3),
    )?;
    params.add(
        Frequency,
        "Frequency",
        u::slider((0.0, 1000.0), (0.1, 30.0), d.frequency as f64, 3),
    )?;
    if !dissolve {
        params.add(
            Phase,
            "Phase",
            u::slider((-10000.0, 10000.0), (-1.0, 1.0), 0.0, 3),
        )?;
        params.add(
            Stillness,
            "Stillness",
            u::slider((0.0, 1.0), (0.0, 1.0), shake::STILLNESS as f64, 3),
        )?;
        params.add(
            TwitchFrequency,
            "Twitch Frequency",
            u::slider((0.0, 1000.0), (0.1, 30.0), 2.0, 3),
        )?;
        params.add(
            Drift,
            "Drift",
            u::slider((0.0, 1.0), (0.0, 1.0), shake::DRIFT as f64, 3),
        )?;
        params.add(
            CenterBias,
            "Center Bias",
            u::slider((0.0, 1000.0), (0.0, 1.0), 0.0, 3),
        )?;
        params.add(
            ZDist,
            "Z Dist",
            u::slider((0.001, 1000.0), (0.1, 2.0), 1.0, 3),
        )?;
    }
    params.add(MotionBlur, "Motion Blur", u::checkbox(d.motion_blur))?;
    params.add(
        MoBlurLength,
        "Mo Blur Length",
        u::slider((0.0, 100.0), (0.0, 2.0), d.mo_blur_length as f64, 3),
    )?;
    params.add(Seed, "Seed", u::slider((0.0, 100000.0), (0.0, 1.0), 0.0, 3))?;
    params.add(WrapX, "Wrap X", u::popup(&["No", "Tile", "Reflect"], 3))?;
    params.add(WrapY, "Wrap Y", u::popup(&["No", "Tile", "Reflect"], 3))?;
    let half = (0.0, (0.5 * PX) as f32);
    axis(
        params,
        [
            XStart, XRandAmp, XRandFreq, XWaveAmp, XWaveFreq, XPhase, XEnd,
        ],
        "X",
        0.2 * PX,
        half,
        1,
    )?;
    axis(
        params,
        [
            YStart, YRandAmp, YRandFreq, YWaveAmp, YWaveFreq, YPhase, YEnd,
        ],
        "Y",
        0.1 * PX,
        half,
        1,
    )?;
    axis(
        params,
        [
            ZStart, ZRandAmp, ZRandFreq, ZWaveAmp, ZWaveFreq, ZPhase, ZEnd,
        ],
        "Z",
        0.0,
        half,
        1,
    )?;
    axis(
        params,
        [
            TiltStart,
            TiltRandAmp,
            TiltRandFreq,
            TiltWaveAmp,
            TiltWaveFreq,
            TiltPhase,
            TiltEnd,
        ],
        "Tilt",
        0.0,
        (0.0, 90.0),
        2,
    )?;
    let pr = if dissolve { (-0.5, 0.5) } else { (-0.1, 0.1) };
    params.add_group(ChStart, ChEnd, "Channels", true, |p| {
        p.add(
            RedAmp,
            "Red Amplitude",
            u::slider((0.0, 1000.0), (0.0, 4.0), 1.0, 3),
        )?;
        p.add(
            GreenAmp,
            "Green Amplitude",
            u::slider((0.0, 1000.0), (0.0, 4.0), 1.0, 3),
        )?;
        p.add(
            BlueAmp,
            "Blue Amplitude",
            u::slider((0.0, 1000.0), (0.0, 4.0), 1.0, 3),
        )?;
        p.add(
            RedPhase,
            "Red Phase",
            u::slider((-10000.0, 10000.0), pr, 0.0, 4),
        )?;
        p.add(
            GreenPhase,
            "Green Phase",
            u::slider((-10000.0, 10000.0), pr, 0.0, 4),
        )?;
        p.add(
            BluePhase,
            "Blue Phase",
            u::slider((-10000.0, 10000.0), pr, 0.0, 4),
        )?;
        p.add(
            RgbRandomness,
            "RGB Randomness",
            u::slider((0.0, 1000.0), (0.0, 1.0), 0.0, 3),
        )?;
        p.add(
            RgbFrequency,
            "RGB Frequency",
            u::slider((0.0, 1000.0), (0.1, 30.0), 2.0, 3),
        )?;
        Ok(())
    })?;
    params.add(
        Opacity,
        "Opacity",
        u::popup(&["All Opaque", "Normal", "As Premult"], 2),
    )?;
    Ok(())
}

/// Everything a frame needs: parameters (normalised units), the layer frame at
/// the current resolution, time (s), frame rate and the transition (Dissolve Shake).
struct Setup {
    p: ShakeParams,
    fr: Frame,
    t: f32,
    fps: f32,
    dissolve: Option<DissolveParams>,
}

fn read(params: &P, in_data: &ae::InData, dissolve: bool) -> Setup {
    use Params::*;
    let (dx, dy) = u::downsample(in_data);
    let d = if dissolve {
        shake::dissolve_shake_defaults()
    } else {
        ShakeParams::default()
    };
    // in_data.width is the full-resolution layer width.
    let k = 2.0 / (in_data.width() as f32).max(1.0);
    // Slider values are 16.16 fixed point in the original.
    let f = |p| shake::fixed(u::get_float(params, p));
    let ax = |ids: [Params; 5], scale: f32| Axis {
        rand_amp: f(ids[0]) * scale,
        rand_freq: f(ids[1]),
        wave_amp: f(ids[2]) * scale,
        wave_freq: f(ids[3]),
        phase: f(ids[4]),
    };
    let wrap = |p| match u::get_popup(params, p) {
        1 => Wrap::No,
        2 => Wrap::Tile,
        _ => Wrap::Reflect,
    };
    let p = ShakeParams {
        style: match (dissolve, u::get_popup(params, Style)) {
            (false, 2) => shake::Style::Twitchy,
            (false, 3) => shake::Style::Jumpy,
            _ => shake::Style::Normal,
        },
        amplitude: f(Amplitude),
        frequency: f(Frequency),
        phase: if dissolve { 0.0 } else { f(Phase) },
        stillness: if dissolve { d.stillness } else { f(Stillness) },
        twitch_frequency: if dissolve {
            d.twitch_frequency
        } else {
            f(TwitchFrequency)
        },
        drift: if dissolve { d.drift } else { f(Drift) },
        center_bias: if dissolve { 0.0 } else { f(CenterBias) },
        z_dist: if dissolve { 1.0 } else { f(ZDist) },
        motion_blur: u::get_bool(params, MotionBlur),
        mo_blur_length: f(MoBlurLength),
        seed: f(Seed),
        wrap: [wrap(WrapX), wrap(WrapY)],
        x: ax([XRandAmp, XRandFreq, XWaveAmp, XWaveFreq, XPhase], k),
        y: ax([YRandAmp, YRandFreq, YWaveAmp, YWaveFreq, YPhase], k),
        z: ax([ZRandAmp, ZRandFreq, ZWaveAmp, ZWaveFreq, ZPhase], k),
        tilt: ax(
            [
                TiltRandAmp,
                TiltRandFreq,
                TiltWaveAmp,
                TiltWaveFreq,
                TiltPhase,
            ],
            1.0,
        ),
        rgb_amp: [f(RedAmp), f(GreenAmp), f(BlueAmp)],
        rgb_phase: [f(RedPhase), f(GreenPhase), f(BluePhase)],
        rgb_randomness: f(RgbRandomness),
        rgb_frequency: f(RgbFrequency),
        alpha_mode: match u::get_popup(params, Opacity) {
            1 => AlphaMode::AllOpaque,
            3 => AlphaMode::AsPremult,
            _ => AlphaMode::Normal,
        },
    };
    let fr = Frame {
        w: in_data.width() as f32 * dx,
        h: in_data.height() as f32 * dy,
        src_x: 0,
        src_y: 0,
    };
    let ts = in_data.time_scale().max(1) as f32;
    let t = in_data.current_time() as f32 / ts;
    let step = in_data.time_step() as f32 / ts;
    let fps = if step > 0.0 { 1.0 / step } else { 25.0 };
    let dissolve = dissolve.then(|| {
        let amount = if u::get_popup(params, AutoTrans) == 2 {
            // Share of the layer's time, reaching 1 on its last frame.
            let total = unsafe { (*in_data.as_ptr()).total_time } as f32 / ts;
            let span = total - step;
            if span > 0.0 {
                t / span
            } else {
                1.0
            }
        } else {
            f(DissolvePercent) / 100.0
        };
        DissolveParams {
            on_from_bg: u::get_popup(params, TransitionDir) == 2,
            amount,
            speed: f(DissolveSpeed),
        }
    });
    Setup {
        p,
        fr,
        t,
        fps,
        dissolve,
    }
}

fn frame(s: &Setup, src: &u::Placed, bg: Option<&facet_core::Image>) -> facet_core::Image {
    let fr = Frame {
        src_x: src.x,
        src_y: src.y,
        ..s.fr
    };
    match &s.dissolve {
        Some(d) => shake::render_dissolve(&src.img, bg, &fr, &s.p, d, s.t, s.fps),
        None => shake::render(&src.img, &fr, &s.p, s.t, s.fps),
    }
}

pub fn handle_command(
    cmd: ae::Command,
    in_data: ae::InData,
    mut out_data: ae::OutData,
    params: &mut P,
    dissolve: bool,
    about: &str,
) -> Result<(), ae::Error> {
    match cmd {
        ae::Command::About => out_data.set_return_msg(about),
        // The shaken frame pulls from anywhere in the layer: ask for all of it.
        ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
            let side = if dissolve {
                vec![(u::layer_param_index(params, Params::Background), u::ID_BG)]
            } else {
                Vec::new()
            };
            u::pre_render(&in_data, &mut extra, (1e6, 1e6), &side)
        })?,
        ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
            let s = read(params, &in_data, dissolve);
            u::smart_render(&extra, |src, side| {
                frame(&s, src, side[u::ID_BG as usize].as_ref())
            })
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("render", || {
            let s = read(params, &in_data, dissolve);
            u::legacy_render(&in_layer, &mut out_layer, |src| frame(&s, src, None))
        })?,
        _ => {}
    }
    Ok(())
}
