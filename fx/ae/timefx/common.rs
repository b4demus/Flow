//! Shared body of the facet time effects. Every plugin (ae/<effect>/src/lib.rs)
//! includes this file and picks its `Kind`: parameters, the frames each one
//! needs from AE, rendering and the Set Level buttons. Behaviour: spec/effects/time.md.

use after_effects as ae;
use facet_ae_util as u;
use facet_core::flicker::{self, FlickerParams, MatteUse, Measure, Region};
use facet_core::temporal::{
    from_working, to_working, AlphaMode, Clip, FieldSwap, Frames, Pick, TimeAverage, WarpRgb,
};
use facet_core::{shake, Image};

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Flicker,
    Remove,
    RemoveColor,
    RemoveMatte,
    RmMatteColor,
    Match,
    MatchColor,
    MatchMatte,
    MchMatteColor,
    FreezeFrame,
    GetFrame,
    JitterFrames,
    RandomEdits,
    ReverseClip,
    ReverseEdits,
    TimeWarpRgb,
    TimeAverage,
}

impl Kind {
    /// Flicker Remove / Match and their variants (they measure an average).
    fn measures(self) -> bool {
        matches!(
            self,
            Kind::Remove
                | Kind::RemoveColor
                | Kind::RemoveMatte
                | Kind::RmMatteColor
                | Kind::Match
                | Kind::MatchColor
                | Kind::MatchMatte
                | Kind::MchMatteColor
        )
    }
    fn remove(self) -> bool {
        matches!(
            self,
            Kind::Remove | Kind::RemoveColor | Kind::RemoveMatte | Kind::RmMatteColor
        )
    }
    fn color(self) -> bool {
        matches!(
            self,
            Kind::RemoveColor | Kind::RmMatteColor | Kind::MatchColor | Kind::MchMatteColor
        )
    }
    fn matte(self) -> bool {
        matches!(
            self,
            Kind::RemoveMatte | Kind::RmMatteColor | Kind::MatchMatte | Kind::MchMatteColor
        )
    }
    fn has_opacity(self) -> bool {
        !matches!(
            self,
            Kind::Flicker
                | Kind::FreezeFrame
                | Kind::JitterFrames
                | Kind::RandomEdits
                | Kind::TimeWarpRgb
        )
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    MatchLayer,
    MatteLayer,
    // Flicker
    Amplitude,
    RandLumaAmp,
    RandColorAmp,
    RandFreq,
    WaveStart,
    WaveAmp,
    WaveFreq,
    WaveRPhase,
    WaveGPhase,
    WaveBPhase,
    WaveEnd,
    RedAmp,
    GreenAmp,
    BlueAmp,
    // Remove / Match
    RectCorner1,
    RectCorner2,
    Level,
    LevelColor,
    SetLevel,
    MatteUse,
    InvertMatte,
    // other frames
    FreezeFrames,
    FrameStart,
    GetFrame,
    InterpFrames,
    EditFrameLength,
    JitterFrameDist,
    EditsStart,
    ReverseAll,
    SwapFields,
    RedShift,
    GreenShift,
    BlueShift,
    ClampChroma,
    AveOverFrames,
    Brightness,
    Seed,
    Opacity,
    OutputOpacity,
    ShowRect,
}

pub type P<'a> = ae::Parameters<'a, Params>;

const SEED: f64 = 0.123;
const OPACITY: [&str; 3] = ["All Opaque", "Normal", "As Premult"];
const SWAP: [&str; 4] = ["No", "Yes Ave", "Yes Up1", "Yes Down1"];

fn int(valid: (f32, f32), slider: (f32, f32), default: f64) -> ae::FloatSliderDef<'static> {
    u::slider(valid, slider, default, 0)
}

pub fn params_setup(params: &mut P, kind: Kind) -> Result<(), ae::Error> {
    use Params::*;
    let big = 100000.0;
    match kind {
        Kind::Flicker => {
            params.add(
                Amplitude,
                "Amplitude",
                u::slider((0.0, big), (0.0, 1.0), 0.2, 3),
            )?;
            params.add(
                RandLumaAmp,
                "Rand Luma Amp",
                u::slider((0.0, big), (0.0, 2.0), 1.0, 3),
            )?;
            params.add(
                RandColorAmp,
                "Rand Color Amp",
                u::slider((0.0, big), (0.0, 2.0), 0.0, 3),
            )?;
            params.add(
                RandFreq,
                "Rand Freq",
                u::slider((0.0, big), (0.0, 60.0), 30.0, 2),
            )?;
            params.add_group(WaveStart, WaveEnd, "Wave", true, |p| {
                p.add(
                    WaveAmp,
                    "Wave Amp",
                    u::slider((0.0, big), (0.0, 2.0), 0.0, 3),
                )?;
                p.add(
                    WaveFreq,
                    "Wave Freq",
                    u::slider((0.0, big), (0.0, 60.0), 5.0, 2),
                )?;
                p.add(
                    WaveRPhase,
                    "Wave R Phase",
                    u::slider((-big, big), (-1.0, 1.0), 0.0, 3),
                )?;
                p.add(
                    WaveGPhase,
                    "Wave G Phase",
                    u::slider((-big, big), (-1.0, 1.0), 0.0, 3),
                )?;
                p.add(
                    WaveBPhase,
                    "Wave B Phase",
                    u::slider((-big, big), (-1.0, 1.0), 0.0, 3),
                )?;
                Ok(())
            })?;
            params.add(RedAmp, "Red Amp", u::slider((0.0, big), (0.0, 2.0), 1.0, 3))?;
            params.add(
                GreenAmp,
                "Green Amp",
                u::slider((0.0, big), (0.0, 2.0), 1.0, 3),
            )?;
            params.add(
                BlueAmp,
                "Blue Amp",
                u::slider((0.0, big), (0.0, 2.0), 1.0, 3),
            )?;
            params.add(
                Brightness,
                "Brightness",
                u::slider((0.0, big), (0.0, 2.0), 1.0, 3),
            )?;
            params.add(Seed, "Seed", u::slider((0.0, big), (0.0, 1.0), SEED, 3))?;
        }
        k if k.measures() => {
            let (what, word) = if k.remove() {
                ("Hold", "Hold")
            } else {
                ("Match", "Match")
            };
            if !k.remove() {
                params.add(MatchLayer, "Match", ae::LayerDef::new())?;
            }
            if k.matte() {
                params.add(MatteLayer, "Matte from Layer", ae::LayerDef::new())?;
            } else {
                // Corners as percent of the layer, the original's defaults.
                params.add(
                    RectCorner1,
                    "Rect Corner1",
                    ae::PointDef::setup(|d| {
                        d.set_default((20.0, 79.0));
                    }),
                )?;
                params.add(
                    RectCorner2,
                    "Rect Corner2",
                    ae::PointDef::setup(|d| {
                        d.set_default((79.0, 20.0));
                    }),
                )?;
            }
            let slider_max = if k.matte() { 1.0 } else { 2.0 };
            if k.color() {
                // The original's default grey is 127/255.
                params.add(
                    LevelColor,
                    &format!("{what} Color"),
                    u::color([127, 127, 127]),
                )?;
                params.add(
                    SetLevel,
                    &format!("Set {word} Color"),
                    ae::ButtonDef::setup(|d| {
                        d.set_label("Set");
                    }),
                )?;
            } else {
                params.add(
                    Level,
                    &format!("{what} Level"),
                    u::slider((0.01, big), (0.01, slider_max), 0.5, 3),
                )?;
                params.add(
                    SetLevel,
                    &format!("Set {word} Level"),
                    ae::ButtonDef::setup(|d| {
                        d.set_label("Set");
                    }),
                )?;
            }
            if k.matte() {
                params.add(MatteUse, "Matte Use", u::popup(&["Luma", "Alpha"], 2))?;
                params.add(InvertMatte, "Invert Matte", u::checkbox(false))?;
            }
        }
        Kind::FreezeFrame => {
            params.add(
                FreezeFrames,
                "Freeze Frames",
                int((1.0, big), (1.0, 30.0), 5.0),
            )?;
            params.add(FrameStart, "Frame Start", int((0.0, big), (0.0, 30.0), 0.0))?;
        }
        Kind::GetFrame => {
            params.add(
                GetFrame,
                "Get Frame",
                u::slider((0.0, 1.0e6), (0.0, 200.0), 0.0, 2),
            )?;
            params.add(InterpFrames, "Interp Frames", u::checkbox(false))?;
        }
        Kind::JitterFrames => {
            params.add(
                EditFrameLength,
                "Edit Frame Length",
                int((1.0, big), (1.0, 30.0), 1.0),
            )?;
            params.add(
                JitterFrameDist,
                "Jitter Frame Dist",
                u::slider((0.0, big), (0.0, 100.0), 10.0, 2),
            )?;
            params.add(Seed, "Seed", u::slider((0.0, big), (0.0, 1.0), SEED, 3))?;
        }
        Kind::RandomEdits => {
            params.add(
                EditFrameLength,
                "Edit Frame Length",
                u::slider((1.0, big), (1.0, 30.0), 2.0, 2),
            )?;
            params.add(Seed, "Seed", u::slider((0.0, big), (0.0, 1.0), SEED, 3))?;
        }
        Kind::ReverseClip => {
            params.add(SwapFields, "Swap Fields", u::popup(&SWAP, 3))?;
        }
        Kind::ReverseEdits => {
            params.add(
                EditFrameLength,
                "Edit Frame Length",
                u::slider((1.0, big), (1.0, 30.0), 5.0, 2),
            )?;
            params.add(EditsStart, "Edits Start", int((0.0, big), (0.0, 30.0), 0.0))?;
            params.add(ReverseAll, "Reverse All", u::checkbox(false))?;
            params.add(SwapFields, "Swap Fields", u::popup(&SWAP, 1))?;
        }
        Kind::TimeWarpRgb => {
            params.add(
                RedShift,
                "Red Shift Frames",
                int((-big, big), (-30.0, 30.0), 1.0),
            )?;
            params.add(
                GreenShift,
                "Green Shift Frames",
                int((-big, big), (-30.0, 30.0), 0.0),
            )?;
            params.add(
                BlueShift,
                "Blue Shift Frames",
                int((-big, big), (-30.0, 30.0), -1.0),
            )?;
            params.add(
                ClampChroma,
                "Clamp Chroma",
                u::slider((0.0, 1.0), (0.0, 1.0), 1.0, 3),
            )?;
            params.add(Opacity, "Input Opacity", u::popup(&OPACITY, 2))?;
            params.add(
                OutputOpacity,
                "Output Opacity",
                u::popup(&["All Opaque", "Copy From Input"], 1),
            )?;
        }
        Kind::TimeAverage => {
            params.add(
                AveOverFrames,
                "Ave Over Frames",
                int((1.0, big), (1.0, 100.0), 10.0),
            )?;
            params.add(
                Brightness,
                "Brightness",
                u::slider((0.0, big), (0.0, 2.0), 1.0, 3),
            )?;
        }
        _ => {}
    }
    if kind.has_opacity() {
        params.add(Opacity, "Opacity", u::popup(&OPACITY, 2))?;
    }
    if kind.measures() && !kind.matte() {
        params.add(ShowRect, "Show Rect", u::checkbox(true))?;
    }
    Ok(())
}

/// Slider value as the original stores it (16.16 fixed).
fn f(params: &P, p: Params) -> f32 {
    shake::fixed(u::get_float(params, p))
}

fn alpha_mode(params: &P, kind: Kind) -> AlphaMode {
    if !kind.has_opacity() && kind != Kind::TimeWarpRgb {
        return AlphaMode::Normal;
    }
    match u::get_popup(params, Params::Opacity) {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    }
}

fn fields(v: i32) -> FieldSwap {
    match v {
        2 => FieldSwap::Ave,
        3 => FieldSwap::Up1,
        4 => FieldSwap::Down1,
        _ => FieldSwap::No,
    }
}

fn clip(in_data: &ae::InData) -> Clip {
    let (frame, _) = u::frame_number(in_data);
    Clip {
        frame,
        total: u::total_frames(in_data),
    }
}

/// What a frame-picking effect does with its parameters.
enum Plan {
    Pick(Pick),
    Warp(WarpRgb),
    Average(TimeAverage),
}

fn plan(params: &P, kind: Kind) -> Option<Plan> {
    use Params::*;
    Some(match kind {
        Kind::FreezeFrame => Plan::Pick(Pick::FreezeFrame {
            start: f(params, FrameStart),
            frames: f(params, FreezeFrames),
        }),
        Kind::GetFrame => Plan::Pick(Pick::GetFrame {
            frame: f(params, GetFrame),
            interp: u::get_bool(params, InterpFrames),
        }),
        Kind::JitterFrames => Plan::Pick(Pick::JitterFrames {
            len: f(params, EditFrameLength),
            dist: f(params, JitterFrameDist),
            seed: f(params, Seed),
        }),
        Kind::RandomEdits => Plan::Pick(Pick::RandomEdits {
            len: f(params, EditFrameLength),
            seed: f(params, Seed),
        }),
        Kind::ReverseClip => Plan::Pick(Pick::ReverseClip {
            fields: fields(u::get_popup(params, SwapFields)),
        }),
        Kind::ReverseEdits => Plan::Pick(Pick::ReverseEdits {
            len: f(params, EditFrameLength),
            start: f(params, EditsStart),
            reverse_all: u::get_bool(params, ReverseAll),
            fields: fields(u::get_popup(params, SwapFields)),
        }),
        Kind::TimeWarpRgb => Plan::Warp(WarpRgb {
            shift: [
                f(params, RedShift),
                f(params, GreenShift),
                f(params, BlueShift),
            ],
            clamp_chroma: f(params, ClampChroma),
            opaque: u::get_popup(params, OutputOpacity) == 1,
        }),
        Kind::TimeAverage => Plan::Average(TimeAverage {
            frames: f(params, AveOverFrames),
            brightness: f(params, Brightness),
        }),
        _ => return None,
    })
}

/// Other frames of the layer this render reads.
fn needed_frames(params: &P, kind: Kind, c: Clip) -> Vec<i32> {
    match plan(params, kind) {
        Some(Plan::Pick(pk)) => pk.needs(c),
        Some(Plan::Warp(w)) => w.needs(c),
        Some(Plan::Average(a)) => a.needs(c),
        None => Vec::new(),
    }
}

struct Working {
    frames: Vec<(i32, Image)>,
    blank: Image,
}

impl Frames for Working {
    fn get(&self, n: i32) -> &Image {
        self.frames
            .iter()
            .find(|(f, _)| *f == n)
            .map(|(_, i)| i)
            .unwrap_or(&self.blank)
    }
}

fn flicker_params(params: &P) -> FlickerParams {
    use Params::*;
    FlickerParams {
        amplitude: f(params, Amplitude),
        rand_luma_amp: f(params, RandLumaAmp),
        rand_color_amp: f(params, RandColorAmp),
        rand_freq: f(params, RandFreq),
        wave_amp: f(params, WaveAmp),
        wave_freq: f(params, WaveFreq),
        wave_phase: [
            f(params, WaveRPhase),
            f(params, WaveGPhase),
            f(params, WaveBPhase),
        ],
        rgb_amp: [f(params, RedAmp), f(params, GreenAmp), f(params, BlueAmp)],
        brightness: f(params, Brightness),
        seed: f(params, Seed),
    }
}

fn point(params: &P, p: Params) -> (f64, f64) {
    params
        .get(p)
        .ok()
        .and_then(|d| d.as_point().ok().map(|v| v.value()))
        .map(|(x, y)| (x as f64, y as f64))
        .unwrap_or((0.0, 0.0))
}

fn matte_use(params: &P) -> MatteUse {
    if u::get_popup(params, Params::MatteUse) == 1 {
        MatteUse::Luma
    } else {
        MatteUse::Alpha
    }
}

/// Average the measuring effects see: `measured` and `matte` are straight and
/// placed like `at` (the source buffer, whose origin is in layer pixels).
fn average(
    params: &P,
    kind: Kind,
    at: &u::Placed,
    measured: &Image,
    matte: Option<&Image>,
) -> [f32; 3] {
    let what = if kind.color() {
        Measure::Color
    } else {
        Measure::Level
    };
    let mode = alpha_mode(params, kind);
    if kind.matte() {
        let use_ = flicker::default_matte_use(
            matte.is_some(),
            kind == Kind::MatchMatte,
            matte_use(params),
        );
        let m = matte.unwrap_or(measured);
        flicker::measure(
            measured,
            &Region::Matte(m, use_, u::get_bool(params, Params::InvertMatte)),
            what,
            mode,
        )
    } else {
        let (c1, c2) = (
            point(params, Params::RectCorner1),
            point(params, Params::RectCorner2),
        );
        let off = |c: (f64, f64)| (c.0 - at.x as f64, c.1 - at.y as f64);
        let b = flicker::rect_bounds(off(c1), off(c2), measured.w, measured.h);
        flicker::measure(measured, &Region::Rect(b), what, mode)
    }
}

fn level(params: &P, kind: Kind) -> [f32; 3] {
    if kind.color() {
        u::get_color(params, Params::LevelColor)
    } else {
        [f(params, Params::Level); 3]
    }
}

fn render_measuring(params: &P, kind: Kind, src: &u::Placed, side: &[Option<Image>; 4]) -> Image {
    let mode = alpha_mode(params, kind);
    let blank;
    let measured = if kind.remove() {
        &src.img
    } else {
        match &side[u::ID_BG as usize] {
            Some(m) => m,
            None => {
                blank = Image::new(src.img.w, src.img.h);
                &blank
            }
        }
    };
    let ave = average(
        params,
        kind,
        src,
        measured,
        side[u::ID_MATTE as usize].as_ref(),
    );
    let lv = level(params, kind);
    let g = if kind.remove() {
        flicker::remove_gain(lv, ave)
    } else {
        flicker::match_gain(lv, ave)
    };
    flicker::apply_gain(&src.img, g, mode)
}

fn render(
    params: &P,
    in_data: &ae::InData,
    kind: Kind,
    src: &u::Placed,
    side: &[Option<Image>; 4],
    set: &u::FrameSet,
) -> Image {
    if kind == Kind::Flicker {
        let ts = in_data.time_scale().max(1) as f32;
        let t = in_data.current_time() as f32 / ts;
        // The original scales the colour as it is, also under zero alpha.
        return flicker::apply_gain(
            &src.img,
            flicker::flicker_gain(&flicker_params(params), t),
            AlphaMode::AsPremult,
        );
    }
    if kind.measures() {
        return render_measuring(params, kind, src, side);
    }
    let mode = alpha_mode(params, kind);
    let c = clip(in_data);
    let blank = Image::new(src.img.w, src.img.h);
    let wk = Working {
        frames: set
            .frames
            .iter()
            .map(|(n, i)| (*n, to_working(i.clone(), mode)))
            .collect(),
        blank,
    };
    let cur = || to_working(src.img.clone(), mode);
    match plan(params, kind) {
        Some(Plan::Pick(pk)) => from_working(pk.render(c, &wk), mode),
        Some(Plan::Warp(w)) => {
            // The original passes the colours on as they are (no unpremultiply).
            w.render(c, &cur(), &wk)
        }
        Some(Plan::Average(a)) => from_working(a.render(c, &cur(), &wk), mode),
        None => src.img.clone(),
    }
}

/// Set Level / Set Color: the current average, so this frame stays as it is.
fn set_level(params: &mut P, in_data: &ae::InData, kind: Kind) -> Result<(), ae::Error> {
    let (t, step, scale) = (
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );
    let checkout = |index: Option<usize>| -> Option<u::Placed> {
        let raw = in_data
            .interact()
            .checkout_param(index? as i32, t, step, scale)
            .ok()?;
        let mut ld = unsafe { raw.u.ld };
        let placed =
            (!ld.data.is_null()).then(|| u::read(&ae::Layer::from_raw(&mut ld, *in_data, None)));
        let _ = in_data.interact().checkin_param(&raw);
        placed
    };
    let Some(src) = checkout(Some(0)) else {
        return Ok(());
    };
    let measured = if kind.remove() {
        Some(src.img.clone())
    } else {
        checkout(u::layer_param_index(params, Params::MatchLayer)).map(|m| u::align(&m, &src))
    };
    let Some(measured) = measured else {
        return Ok(());
    };
    let matte = if kind.matte() {
        checkout(u::layer_param_index(params, Params::MatteLayer)).map(|m| u::align(&m, &src))
    } else {
        None
    };
    let ave = average(params, kind, &src, &measured, matte.as_ref());
    if kind.color() {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let px = ae::Pixel8 {
            alpha: 255,
            red: q(ave[0]),
            green: q(ave[1]),
            blue: q(ave[2]),
        };
        let mut p = params.get_mut(Params::LevelColor)?;
        p.as_color_mut()?.set_value(px);
        p.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    } else {
        let mut p = params.get_mut(Params::Level)?;
        p.as_float_slider_mut()?.set_value(ave[0].max(0.01) as f64);
        p.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    }
    Ok(())
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
        ae::Command::UserChangedParam { param_index } => {
            if kind.measures() && params.index(Params::SetLevel) == Some(param_index) {
                u::guarded("set_level", || set_level(params, &in_data, kind))?;
            }
        }
        ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
            // Averages need the whole layer, frame pickers only the requested area.
            let reach = if kind.measures() {
                (1e6, 1e6)
            } else {
                (0.0, 0.0)
            };
            let mut side = Vec::new();
            if kind.measures() && !kind.remove() {
                side.push((u::layer_param_index(params, Params::MatchLayer), u::ID_BG));
            }
            if kind.matte() {
                side.push((
                    u::layer_param_index(params, Params::MatteLayer),
                    u::ID_MATTE,
                ));
            }
            let frames = needed_frames(params, kind, clip(&in_data));
            u::pre_render_frames(&in_data, &mut extra, reach, &side, &frames)
        })?,
        ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
            u::smart_render_frames(&extra, |src, side, set| {
                render(params, &in_data, kind, src, side, set)
            })
        })?,
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => u::guarded("render", || {
            let empty = u::FrameSet { frames: Vec::new() };
            u::legacy_render(&in_layer, &mut out_layer, |src| {
                render(params, &in_data, kind, src, &Default::default(), &empty)
            })
        })?,
        _ => {}
    }
    Ok(())
}
