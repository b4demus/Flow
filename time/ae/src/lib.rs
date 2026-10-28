//! Flowtime: optical-flow retiming effect for After Effects.
//! Parameter layout follows docs/twixtor-functional-spec.md.

use after_effects as ae;
use rayon::prelude::*;
use flowtime_core::{self as ft, Display, FgLayer, FlowCache, Frames, Gray, Guide, Image, ImagePrep, Quality, Settings};

const FG_COUNT: u8 = 3;
const POINT_COUNT: u8 = 12;
const POINT_COLORS: [(u8, u8, u8); 12] = [
    (255, 64, 64),
    (64, 255, 64),
    (64, 128, 255),
    (255, 255, 64),
    (255, 64, 255),
    (64, 255, 255),
    (255, 160, 64),
    (160, 64, 255),
    (64, 255, 160),
    (255, 128, 160),
    (160, 255, 64),
    (255, 255, 255),
];

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Display,
    DrawGeom,
    SrcStart,
    AltColor,
    AltMotion,
    MarkSegs,
    InFps,
    SrcEnd,
    TrackStart,
    MotionVectors,
    ImagePrep,
    TrackRgba,
    PlentyMemory,
    TrackEnd,
    OutStart,
    RemapMode,
    Speed,
    FrameNum,
    FrameInterp,
    Warping,
    MotionBlur,
    OutEnd,
    BgStart,
    BgSens,
    BgEnd,
    FgStart(u8),
    FgMatte(u8),
    FgChan(u8),
    FgSens(u8),
    FgShrink(u8),
    FgEnd(u8),
    PtsStart,
    PtUse(u8),
    PtPos(u8),
    PtColor(u8),
    PtsEnd,
}

struct Plugin {
    cache: FlowCache,
    last_key: std::sync::atomic::AtomicU64,
}

impl Default for Plugin {
    fn default() -> Self {
        install_panic_log();
        Self { cache: FlowCache::new(12), last_key: Default::default() }
    }
}

ae::define_effect!(Plugin, (), Params);

fn popup(options: &[&str], default: i32) -> ae::PopupDef<'static> {
    ae::PopupDef::setup(|d| {
        d.set_options(options);
        d.set_default(default);
        d.set_value(default);
    })
}

#[allow(clippy::too_many_arguments)]
fn slider(valid: (f32, f32), slider: (f32, f32), default: f64, precision: i16) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|d| {
        d.set_valid_min(valid.0);
        d.set_valid_max(valid.1);
        d.set_slider_min(slider.0);
        d.set_slider_max(slider.1);
        d.set_default(default);
        d.set_value(default);
        d.set_precision(precision);
    })
}

fn checkbox(default: bool) -> ae::CheckBoxDef<'static> {
    ae::CheckBoxDef::setup(|d| {
        d.set_default(default);
        d.set_value(default);
    })
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut ae::Parameters<Params>, _in_data: InData, _: OutData) -> Result<(), Error> {
        params.add(
            Params::Display,
            "Display",
            popup(&["Twixtored Output", "Source", "FG1 Twixtored", "FG2 Twixtored", "FG3 Twixtored", "Background Twixtored"], 1),
        )?;
        params.add(Params::DrawGeom, "Draw Geom", popup(&["None", "All", "FG1", "FG2", "FG3", "Main_BG"], 2))?;

        params.add_group(Params::SrcStart, Params::SrcEnd, "Source Control", false, |p| {
            p.add(Params::AltColor, "Alt Color Source", ae::LayerDef::new())?;
            p.add(Params::AltMotion, "Alt Motion Source", ae::LayerDef::new())?;
            p.add(Params::MarkSegs, "Src: Mark Segments", popup(&["Cut A", "Cut B", "Cut C", "No MV, Use Nearest", "No MV, Frame Blend"], 1))?;
            p.add(Params::InFps, "Input: Frame Rate", slider((0.1, 1000.0), (0.1, 1000.0), 29.97, 3))?;
            Ok(())
        })?;

        params.add_group(Params::TrackStart, Params::TrackEnd, "Track Control", false, |p| {
            p.add(Params::MotionVectors, "Motion Vectors", popup(&["No Motion Vectors", "Sloppy", "Medium", "High", "Best"], 5))?;
            p.add(Params::ImagePrep, "Image Prep", popup(&["None", "Contrast/Edge Enhance", "Delinearize"], 1))?;
            p.add(Params::TrackRgba, "Track RGB+A", checkbox(false))?;
            p.add(Params::PlentyMemory, "Plenty Memory?", checkbox(true))?;
            Ok(())
        })?;

        params.add_group(Params::OutStart, Params::OutEnd, "Output Control", false, |p| {
            p.add(Params::RemapMode, "Time Remap Mode", popup(&["Speed", "Frame Number"], 1))?;
            p.add(Params::Speed, "Speed %", slider((-10000.0, 10000.0), (-1000.0, 1000.0), 100.0, 2))?;
            p.add(Params::FrameNum, "Frame Num", slider((-200000.0, 200000.0), (0.0, 2000.0), 0.0, 2))?;
            p.add(Params::FrameInterp, "Frame Interp", popup(&["Nearest", "Blend", "Motion Weighted Blend"], 2))?;
            p.add(Params::Warping, "Warping", popup(&["Inverse", "Inverse w/ Smart Blend", "Forward", "Forward, no Smart Blend"], 1))?;
            p.add(Params::MotionBlur, "Motion Blur Compensation", slider((0.0, 10.0), (0.0, 2.0), 0.0, 2))?;
            Ok(())
        })?;

        params.add_group(Params::BgStart, Params::BgEnd, "Main_BG Layer Settings", true, |p| {
            p.add(Params::BgSens, "Main_BG Sensitivity", slider((0.0, 100.0), (0.0, 100.0), 70.0, 1))?;
            Ok(())
        })?;

        for i in 0..FG_COUNT {
            let n = i + 1;
            params.add_group(Params::FgStart(i), Params::FgEnd(i), &format!("FG{n} Settings"), true, |p| {
                p.add(Params::FgMatte(i), &format!("FG{n} Matte"), ae::LayerDef::new())?;
                p.add(Params::FgChan(i), &format!("FG{n} Matte Chan"), popup(&["Red", "Green", "Blue", "Alpha"], 4))?;
                p.add(Params::FgSens(i), &format!("FG{n} Motion Sensitivity"), slider((0.0, 100.0), (0.0, 100.0), 70.0, 1))?;
                p.add(Params::FgShrink(i), &format!("FG{n} Inv Matte Shrink"), slider((0.0, 10.0), (0.0, 10.0), 0.0, 2))?;
                Ok(())
            })?;
        }

        params.add_group(Params::PtsStart, Params::PtsEnd, "Track Points", true, |p| {
            for i in 0..POINT_COUNT {
                let n = i + 1;
                p.add(Params::PtUse(i), &format!("Pt {n}: Use"), popup(&["Don't Use", "FG 1", "FG 2", "FG 3", "Main_BG Layer"], 1))?;
                p.add(Params::PtPos(i), &format!("Pt {n}: Pos"), ae::PointDef::setup(|d| {
                    d.set_default((-5.0, -5.0));
                }))?;
                let (r, g, b) = POINT_COLORS[i as usize];
                p.add(Params::PtColor(i), &format!("Pt {n}: Color"), ae::ColorDef::setup(|d| {
                    let px = ae::Pixel8 { alpha: 255, red: r, green: g, blue: b };
                    d.set_default(px);
                    d.set_value(px);
                }))?;
            }
            Ok(())
        })?;
        Ok(())
    }

    fn handle_command(&mut self, cmd: ae::Command, in_data: InData, mut out_data: OutData, params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
        if ft::prof::enabled() {
            error_log(&format!("cmd {:?} t={} thread={:?}", cmd.as_raw(), in_data.current_time(), std::thread::current().id()));
        }
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg("Flowtime v0.1\rOptical-flow retiming.\rb4demus");
            }
            ae::Command::Render { in_layer, mut out_layer } => {
                install_panic_log();
                let res = {
                    let _t = ft::prof::span("ae.render");
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(self, in_data, params, &in_layer, &mut out_layer)))
                };
                ft::prof::dump_to_log(&format!("t={} thread={:?}", in_data.current_time(), std::thread::current().id()));
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        error_log(&format!("render t={} returned {e:?}", in_data.current_time()));
                        return Err(e);
                    }
                    Err(p) => {
                        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                        error_log(&format!("render t={} panicked: {msg}", in_data.current_time()));
                        return Err(ae::Error::Generic);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ─────────────────────────── diagnostics ───────────────────────────

/// Appends a line to %TEMP%\flowtime_errors.log (only written on failures).
fn error_log(line: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("flowtime_errors.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{:?} {line}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
    }
}

/// Records where a panic happened (the payload alone has no location).
fn install_panic_log() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            error_log(&format!("panic at {}: {}", info.location().map(|l| l.to_string()).unwrap_or_default(), info));
            prev(info);
        }));
    });
}

// ───────────────────────────── pixels ─────────────────────────────

fn is_deep(layer: &ae::Layer) -> bool {
    let ptr: *const ae::sys::PF_EffectWorld = ae::AsPtr::as_ptr(layer);
    unsafe { ((*ptr).world_flags as u32 & ae::sys::PF_WorldFlag_DEEP as u32) != 0 }
}

fn layer_to_image(layer: &ae::Layer) -> Image {
    let _t = ft::prof::span("ae.layer_in");
    let (w, h) = (layer.width(), layer.height());
    let mut img = Image::new(w, h);
    let deep = is_deep(layer);
    let rb = layer.row_bytes();
    let base = unsafe { layer.data_ptr() } as usize;
    img.data.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        let row = (base as isize + y as isize * rb) as *const u8;
        if deep {
            let px = unsafe { std::slice::from_raw_parts(row as *const ae::Pixel16, w) };
            let k = 1.0 / 32768.0;
            for (o, p) in out.iter_mut().zip(px) {
                *o = [p.red as f32 * k, p.green as f32 * k, p.blue as f32 * k, p.alpha as f32 * k];
            }
        } else {
            let px = unsafe { std::slice::from_raw_parts(row as *const ae::Pixel8, w) };
            let k = 1.0 / 255.0;
            for (o, p) in out.iter_mut().zip(px) {
                *o = [p.red as f32 * k, p.green as f32 * k, p.blue as f32 * k, p.alpha as f32 * k];
            }
        }
    });
    img
}
fn layer_channel(layer: &ae::Layer, chan: i32) -> Gray {
    let img = layer_to_image(layer);
    let data = img
        .data
        .iter()
        .map(|p| match chan {
            1 => p[0],
            2 => p[1],
            3 => p[2],
            _ => p[3],
        })
        .collect();
    Gray { w: img.w, h: img.h, data }
}

fn image_to_layer(img: &Image, layer: &mut ae::Layer) {
    let _t = ft::prof::span("ae.layer_out");
    let deep = is_deep(layer);
    let (w, h) = (layer.width().min(img.w), layer.height().min(img.h));
    let rb = layer.row_bytes();
    let base = unsafe { layer.data_ptr_mut() } as usize;
    (0..h).into_par_iter().for_each(|y| {
        let row = (base as isize + y as isize * rb) as *mut u8;
        let src = &img.data[y * img.w..y * img.w + w];
        if deep {
            let px = unsafe { std::slice::from_raw_parts_mut(row as *mut ae::Pixel16, w) };
            let q = |v: f32| (v.clamp(0.0, 1.0) * 32768.0 + 0.5) as u16;
            for (p, c) in px.iter_mut().zip(src) {
                *p = ae::Pixel16 { alpha: q(c[3]), red: q(c[0]), green: q(c[1]), blue: q(c[2]) };
            }
        } else {
            let px = unsafe { std::slice::from_raw_parts_mut(row as *mut ae::Pixel8, w) };
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            for (p, c) in px.iter_mut().zip(src) {
                *p = ae::Pixel8 { alpha: q(c[3]), red: q(c[0]), green: q(c[1]), blue: q(c[2]) };
            }
        }
    });
}
fn error_image(w: usize, h: usize) -> Image {
    Image { w, h, data: vec![[0.5, 0.0, 0.0, 0.5]; w * h] }
}

fn draw_cross(img: &mut Image, x: f32, y: f32, color: [f32; 3]) {
    let (cx, cy) = (x.round() as isize, y.round() as isize);
    for d in -6..=6isize {
        for (px, py) in [(cx + d, cy), (cx, cy + d)] {
            if px >= 0 && py >= 0 && (px as usize) < img.w && (py as usize) < img.h {
                img.data[py as usize * img.w + px as usize] = [color[0], color[1], color[2], 1.0];
            }
        }
    }
}

// ───────────────────────────── render ─────────────────────────────

/// Layer time (in `time_scale` units) at which source frame `frame` is shown.
struct Clock {
    scale: f64,
    in_fps: f64,
}

impl Clock {
    fn time(&self, frame: i64) -> i32 {
        (frame as f64 / self.in_fps * self.scale).round() as i32
    }
    fn step(&self) -> i32 {
        (self.scale / self.in_fps).round().max(1.0) as i32
    }
}

fn checkout_layer(in_data: InData, index: usize, clock: &Clock, frame: i64) -> Option<Image> {
    let pd = ae::ParamDef::checkout(in_data, index as i32, clock.time(frame), clock.step(), clock.scale as u32, Some(ae::ParamType::Layer)).ok()?;
    let ld = pd.as_layer().ok()?;
    let layer = ld.value()?;
    Some(layer_to_image(&layer))
}

fn popup_at(params: &ae::Parameters<Params>, p: Params, time: Option<(i32, i32, u32)>) -> i32 {
    let r = match time {
        None => params.get(p).and_then(|v| v.as_popup().map(|d| d.value())),
        Some((t, s, sc)) => params.get_at(p, Some(t), Some(s), Some(sc)).and_then(|v| v.as_popup().map(|d| d.value())),
    };
    r.unwrap_or(1)
}

fn float(params: &ae::Parameters<Params>, p: Params) -> f64 {
    params.get(p).and_then(|v| v.as_float_slider().map(|d| d.value())).unwrap_or(0.0)
}

fn float_at(params: &ae::Parameters<Params>, p: Params, t: i32, s: i32, sc: u32) -> f64 {
    params.get_at(p, Some(t), Some(s), Some(sc)).and_then(|v| v.as_float_slider().map(|d| d.value())).unwrap_or(0.0)
}

fn checkbox_value(params: &ae::Parameters<Params>, p: Params) -> bool {
    params.get(p).and_then(|v| v.as_checkbox().map(|d| d.value())).unwrap_or(false)
}

fn point_at(params: &ae::Parameters<Params>, p: Params, t: i32, s: i32, sc: u32) -> Option<(f32, f32)> {
    params.get_at(p, Some(t), Some(s), Some(sc)).and_then(|v| v.as_point().map(|d| d.value())).ok()
}

fn fingerprint(img: &Image) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let n = img.data.len();
    for k in 0..64 {
        let p = img.data[(k * 7919 + 13) % n];
        for c in p {
            h = (h ^ c.to_bits() as u64).wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn render(plugin: &Plugin, in_data: InData, params: &mut ae::Parameters<Params>, in_layer: &ae::Layer, out_layer: &mut ae::Layer) -> Result<(), ae::Error> {
    let (w, h) = (in_layer.width(), in_layer.height());
    let scale = in_data.time_scale() as f64;
    let mut lstep = in_data.local_time_step().unsigned_abs() as f64;
    if lstep == 0.0 {
        lstep = in_data.time_step().unsigned_abs() as f64;
    }
    if lstep == 0.0 {
        lstep = scale / 30.0;
    }
    let out_fps = scale / lstep;
    let in_fps = float(params, Params::InFps).max(0.1);
    let clock = Clock { scale, in_fps };
    let display = popup_at(params, Params::Display, None);
    let draw_geom = popup_at(params, Params::DrawGeom, None);

    // Source display: untouched input in source timing, with the guide geometry.
    if display == 2 {
        let mut img = layer_to_image(in_layer);
        if draw_geom > 1 {
            let now = (in_data.current_time(), in_data.time_step().max(1), in_data.time_scale());
            for i in 0..POINT_COUNT {
                let use_ = popup_at(params, Params::PtUse(i), Some(now));
                let layer_ok = use_ > 1 && (draw_geom == 2 || draw_geom == use_ + 1);
                if !layer_ok {
                    continue;
                }
                if let Some((x, y)) = point_at(params, Params::PtPos(i), now.0, now.1, now.2) {
                    let c = params.get(Params::PtColor(i)).and_then(|v| v.as_color().map(|d| d.value())).ok();
                    let col = c.map_or([1.0, 0.0, 0.0], |p| [p.red as f32 / 255.0, p.green as f32 / 255.0, p.blue as f32 / 255.0]);
                    draw_cross(&mut img, x, y, col);
                }
            }
        }
        image_to_layer(&img, out_layer);
        return Ok(());
    }

    // Output time -> source frame.
    let out_pos = in_data.current_time() as f64 / lstep;
    let remap = popup_at(params, Params::RemapMode, None);
    let (src, frames_per_output) = if remap == 2 {
        let v = float(params, Params::FrameNum);
        let next = float_at(params, Params::FrameNum, in_data.current_time() + lstep as i32, lstep as i32, scale as u32);
        (ft::source_from_frame_num(v, out_fps, in_fps), ((next - v) * in_fps / out_fps) as f32)
    } else {
        let cur = float(params, Params::Speed);
        let k = out_pos.floor();
        let frac = out_pos - k;
        let keyed = params.get(Params::Speed).and_then(|p| p.keyframe_count()).unwrap_or(0) > 0;
        let src = if k < 0.0 {
            out_pos * cur / 100.0 / out_fps * in_fps
        } else if !keyed {
            ft::source_from_speed(&[cur * k, cur], frac, out_fps, in_fps)
        } else {
            let n = (k as usize).min(100_000);
            let speeds: Vec<f64> = (0..=n).map(|j| float_at(params, Params::Speed, (j as f64 * lstep) as i32, lstep as i32, scale as u32)).collect();
            ft::source_from_speed(&speeds, frac, out_fps, in_fps)
        };
        (src, (cur / 100.0 * in_fps / out_fps) as f32)
    };
    let pair = ft::pair(src);

    // Source frames (Alt Color Source replaces the color, Alt Motion Source the tracking).
    let color_idx = params.index(Params::AltColor).unwrap_or(0);
    let fetch = |frame: i64| checkout_layer(in_data, color_idx, &clock, frame).or_else(|| checkout_layer(in_data, 0, &clock, frame));
    let (Some(a), Some(b)) = (fetch(pair.a), if pair.b == pair.a { None } else { fetch(pair.b) }.or_else(|| fetch(pair.a))) else {
        image_to_layer(&error_image(w, h), out_layer);
        return Ok(());
    };
    if a.w != w || a.h != h || b.w != w || b.h != h {
        image_to_layer(&error_image(w, h), out_layer);
        return Ok(());
    }
    // Past either end of the clip AE hands back a transparent frame: hold the
    // real neighbour instead of fading towards nothing.
    let coverage = |img: &Image| img.data.iter().step_by(97).map(|p| p[3]).sum::<f32>() / (img.data.len() / 97).max(1) as f32;
    let (a, b) = match (coverage(&a) < 0.01, coverage(&b) < 0.01) {
        (false, true) => (a.clone(), a),
        (true, false) => (b.clone(), b),
        _ => (a, b),
    };
    let motion_idx = params.index(Params::AltMotion).unwrap_or(0);
    let ma = checkout_layer(in_data, motion_idx, &clock, pair.a).filter(|m| m.w == w && m.h == h && motion_idx != 0);
    let mb = checkout_layer(in_data, motion_idx, &clock, pair.b).filter(|m| m.w == w && m.h == h && motion_idx != 0);

    let ta = (clock.time(pair.a), clock.step(), scale as u32);
    let tb = (clock.time(pair.b), clock.step(), scale as u32);
    let over = ft::segment_override(ft::Segment::from_popup(popup_at(params, Params::MarkSegs, Some(ta))), ft::Segment::from_popup(popup_at(params, Params::MarkSegs, Some(tb))));

    // Track points: used only when assigned to the same layer at both source
    // frames and inside the image at both.
    let mut guides: [Vec<Guide>; 4] = Default::default();
    for i in 0..POINT_COUNT {
        let ua = popup_at(params, Params::PtUse(i), Some(ta));
        let ub = popup_at(params, Params::PtUse(i), Some(tb));
        if ua <= 1 || ua != ub {
            continue;
        }
        let (Some(pa), Some(pb)) = (point_at(params, Params::PtPos(i), ta.0, ta.1, ta.2), point_at(params, Params::PtPos(i), tb.0, tb.1, tb.2)) else {
            continue;
        };
        let inside = |p: (f32, f32)| p.0 >= 0.0 && p.1 >= 0.0 && p.0 < w as f32 && p.1 < h as f32;
        if !inside(pa) || !inside(pb) {
            continue;
        }
        // Popup: 2..4 = FG1..FG3, 5 = Main_BG.
        let slot = if ua == 5 { 3 } else { (ua - 2) as usize };
        guides[slot].push(Guide { from: [pa.0, pa.1], to: [pb.0, pb.1] });
    }

    // Foreground layers, in order; a missing matte ends the list.
    let ds = {
        let d = in_data.downsample_x();
        if d.den != 0 { d.num as f32 / d.den as f32 } else { 1.0 }
    };
    let mut fg: Vec<FgLayer> = Vec::new();
    for i in 0..FG_COUNT {
        let Some(idx) = params.index(Params::FgMatte(i)) else { break };
        let chan = popup_at(params, Params::FgChan(i), None);
        let grab = |frame: i64| {
            let pd = ae::ParamDef::checkout(in_data, idx as i32, clock.time(frame), clock.step(), scale as u32, Some(ae::ParamType::Layer)).ok()?;
            let ld = pd.as_layer().ok()?;
            let layer = ld.value()?;
            Some(layer_channel(&layer, chan))
        };
        let (Some(m_a), Some(m_b)) = (grab(pair.a), grab(pair.b)) else { break };
        if m_a.w != w || m_a.h != h || m_b.w != w || m_b.h != h {
            image_to_layer(&error_image(w, h), out_layer);
            return Ok(());
        }
        fg.push(FgLayer {
            matte_a: m_a,
            matte_b: m_b,
            sensitivity: float(params, Params::FgSens(i)) as f32,
            inv_shrink: float(params, Params::FgShrink(i)) as f32 * ds,
            guides: std::mem::take(&mut guides[i as usize]),
        });
    }

    let quality = match popup_at(params, Params::MotionVectors, None) {
        1 => Quality::NoVectors,
        2 => Quality::Sloppy,
        3 => Quality::Medium,
        4 => Quality::High,
        _ => Quality::Best,
    };
    let settings = Settings {
        quality,
        prep: match popup_at(params, Params::ImagePrep, None) {
            2 => ImagePrep::ContrastEdge,
            3 => ImagePrep::Delinearize,
            _ => ImagePrep::None,
        },
        track_rgba: checkbox_value(params, Params::TrackRgba),
        interp: match popup_at(params, Params::FrameInterp, None) {
            1 => ft::FrameInterp::Nearest,
            3 => ft::FrameInterp::MotionWeightedBlend,
            _ => ft::FrameInterp::Blend,
        },
        warping: match popup_at(params, Params::Warping, None) {
            2 => ft::Warping::InverseSmartBlend,
            3 => ft::Warping::Forward,
            4 => ft::Warping::ForwardNoSmartBlend,
            _ => ft::Warping::Inverse,
        },
        draft: matches!(in_data.quality(), ae::Quality::Lo),
        mb_amount: float(params, Params::MotionBlur) as f32,
        frames_per_output,
    };
    let display = match display {
        3 => Display::Fg(0),
        4 => Display::Fg(1),
        5 => Display::Fg(2),
        6 => Display::Background,
        _ => Display::Output,
    };
    let bg_sens = float(params, Params::BgSens) as f32;

    // Everything that changes the flow goes into the cache key.
    // Content-addressed: effect_ref differs between AE's render threads, so it
    // must not be part of the key (identical inputs give identical flows anyway).
    let mut key = 0xcbf29ce484222325u64;
    for v in [
        fingerprint(&a),
        fingerprint(&b),
        quality as u64,
        settings.prep as u64,
        settings.track_rgba as u64,
        settings.draft as u64,
        bg_sens.to_bits() as u64,
        fg.len() as u64,
        ma.as_ref().map_or(0, fingerprint),
        guides[3].len() as u64,
    ] {
        key = (key ^ v).wrapping_mul(0x100000001b3);
    }
    for l in &fg {
        key = (key ^ (l.sensitivity.to_bits() as u64) ^ ((l.inv_shrink.to_bits() as u64) << 32) ^ fingerprint(&Image {
            w: l.matte_a.w,
            h: l.matte_a.h,
            data: l.matte_a.data.iter().map(|&v| [v; 4]).collect(),
        }))
        .wrapping_mul(0x100000001b3);
    }
    for g in fg.iter().flat_map(|l| l.guides.iter()).chain(guides[3].iter()) {
        for v in [g.from[0], g.from[1], g.to[0], g.to[1]] {
            key = (key ^ v.to_bits() as u64).wrapping_mul(0x100000001b3);
        }
    }
    if ft::prof::enabled() {
        drop(ft::prof::span(if key == plugin.last_key.swap(key, std::sync::atomic::Ordering::Relaxed) { "key.same" } else { "key.changed" }));
    }
    let cache = if checkbox_value(params, Params::PlentyMemory) { Some((&plugin.cache, key)) } else { None };

    let frames = Frames { a: &a, b: &b, motion_a: ma.as_ref(), motion_b: mb.as_ref(), id_a: pair.a, id_b: if pair.b == pair.a { pair.a } else { pair.b } };
    let out = ft::retime(&frames, pair.t, over, &fg, bg_sens, &guides[3], &settings, display, cache);
    image_to_layer(&out, out_layer);
    Ok(())
}
