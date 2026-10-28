//! Glue shared by the facet AE effects: pixel conversion for 8/16/32-bit
//! worlds (SmartFX), parameter constructors/getters, diagnostics.

use after_effects as ae;
use facet_core::Image;
use rayon::prelude::*;

// ─────────────────────────── diagnostics ───────────────────────────

/// Appends a line to %TEMP%\facet_errors.log (only written on failures).
pub fn error_log(line: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("facet_errors.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(
            f,
            "{:?} {line}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
    }
}

/// Records where a panic happened (the payload alone has no location).
pub fn install_panic_log() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            error_log(&format!(
                "panic at {}: {}",
                info.location().map(|l| l.to_string()).unwrap_or_default(),
                info
            ));
            prev(info);
        }));
    });
}

/// Runs `f`, turning a panic into an AE error instead of taking the host down.
pub fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, ae::Error>) -> Result<T, ae::Error> {
    install_panic_log();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            error_log(&format!("{what} returned {e:?}"));
            Err(e)
        }
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            error_log(&format!("{what} panicked: {msg}"));
            Err(ae::Error::Generic)
        }
    }
}

// ───────────────────────────── pixels ─────────────────────────────

/// A world's pixels as straight float RGBA, plus where its (0,0) sits in layer space.
pub struct Placed {
    pub img: Image,
    pub x: i32,
    pub y: i32,
}

fn raw(layer: &ae::Layer) -> &ae::sys::PF_LayerDef {
    let ptr: *const ae::sys::PF_EffectWorld = ae::AsPtr::as_ptr(layer);
    unsafe { &*ptr }
}

/// Position of the world's top-left pixel in layer coordinates.
pub fn origin(layer: &ae::Layer) -> (i32, i32) {
    let r = raw(layer);
    (r.origin_x, r.origin_y)
}

/// White point of the world's integer format (8 bit: 255, 16 bit: 32768); 32 bit is float.
fn depth(layer: &ae::Layer) -> u8 {
    match layer.bit_depth() {
        32 => 32,
        16 => 16,
        _ => 8,
    }
}

pub fn read(layer: &ae::Layer) -> Placed {
    let (w, h) = (layer.width(), layer.height());
    let mut img = Image::new(w, h);
    let rb = layer.row_bytes();
    let base = unsafe { layer.data_ptr() } as usize;
    let d = depth(layer);
    img.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(y, out)| {
            let row = (base as isize + y as isize * rb) as *const u8;
            match d {
                32 => {
                    let px = unsafe { std::slice::from_raw_parts(row as *const ae::PixelF32, w) };
                    for (o, p) in out.iter_mut().zip(px) {
                        *o = [p.red, p.green, p.blue, p.alpha];
                    }
                }
                16 => {
                    let px = unsafe { std::slice::from_raw_parts(row as *const ae::Pixel16, w) };
                    let k = 1.0 / 32768.0;
                    for (o, p) in out.iter_mut().zip(px) {
                        *o = [
                            p.red as f32 * k,
                            p.green as f32 * k,
                            p.blue as f32 * k,
                            p.alpha as f32 * k,
                        ];
                    }
                }
                _ => {
                    let px = unsafe { std::slice::from_raw_parts(row as *const ae::Pixel8, w) };
                    let k = 1.0 / 255.0;
                    for (o, p) in out.iter_mut().zip(px) {
                        *o = [
                            p.red as f32 * k,
                            p.green as f32 * k,
                            p.blue as f32 * k,
                            p.alpha as f32 * k,
                        ];
                    }
                }
            }
        });
    let (x, y) = origin(layer);
    Placed { img, x, y }
}

/// Writes `src` (placed in layer space) into `layer`; pixels `src` does not cover become transparent.
pub fn write(src: &Placed, layer: &mut ae::Layer) {
    let (w, h) = (layer.width(), layer.height());
    let (ox, oy) = origin(layer);
    let rb = layer.row_bytes();
    let base = unsafe { layer.data_ptr_mut() } as usize;
    let d = depth(layer);
    (0..h).into_par_iter().for_each(|y| {
        let row = (base as isize + y as isize * rb) as *mut u8;
        let sy = y as i32 + oy - src.y;
        let get = |x: usize| -> [f32; 4] {
            let sx = x as i32 + ox - src.x;
            if sx < 0 || sy < 0 || sx as usize >= src.img.w || sy as usize >= src.img.h {
                [0.0; 4]
            } else {
                src.img.data[sy as usize * src.img.w + sx as usize]
            }
        };
        match d {
            32 => {
                let px = unsafe { std::slice::from_raw_parts_mut(row as *mut ae::PixelF32, w) };
                for (x, p) in px.iter_mut().enumerate() {
                    let c = get(x);
                    *p = ae::PixelF32 {
                        alpha: c[3].clamp(0.0, 1.0),
                        red: c[0],
                        green: c[1],
                        blue: c[2],
                    };
                }
            }
            16 => {
                let px = unsafe { std::slice::from_raw_parts_mut(row as *mut ae::Pixel16, w) };
                let q = |v: f32| (v.clamp(0.0, 1.0) * 32768.0 + 0.5) as u16;
                for (x, p) in px.iter_mut().enumerate() {
                    let c = get(x);
                    *p = ae::Pixel16 {
                        alpha: q(c[3]),
                        red: q(c[0]),
                        green: q(c[1]),
                        blue: q(c[2]),
                    };
                }
            }
            _ => {
                let px = unsafe { std::slice::from_raw_parts_mut(row as *mut ae::Pixel8, w) };
                let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                for (x, p) in px.iter_mut().enumerate() {
                    let c = get(x);
                    *p = ae::Pixel8 {
                        alpha: q(c[3]),
                        red: q(c[0]),
                        green: q(c[1]),
                        blue: q(c[2]),
                    };
                }
            }
        }
    });
}

/// `other` resampled (by offset only) onto `frame`'s pixel grid; uncovered pixels are transparent.
pub fn align(other: &Placed, frame: &Placed) -> Image {
    let (w, h) = (frame.img.w, frame.img.h);
    let mut out = Image::new(w, h);
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            let sy = y as i32 + frame.y - other.y;
            if sy < 0 || sy as usize >= other.img.h {
                return;
            }
            for (x, o) in row.iter_mut().enumerate() {
                let sx = x as i32 + frame.x - other.x;
                if sx >= 0 && (sx as usize) < other.img.w {
                    *o = other.img.data[sy as usize * other.img.w + sx as usize];
                }
            }
        });
    out
}

// ───────────────────────────── SmartFX ─────────────────────────────

/// Checkout ids of optional layer inputs (the source itself is id 0).
pub const ID_BG: i32 = 1;
pub const ID_MATTE: i32 = 2;

/// Which optional layer inputs PreRender checked out (passed on to SmartRender).
#[derive(Default, Clone, Copy, Debug)]
pub struct SideLayers {
    pub checked: [bool; 4],
}

/// Layer parameter index if a layer is chosen in it.
pub fn layer_param_index<P: Eq + PartialEq + std::hash::Hash + Copy + std::fmt::Debug>(
    params: &ae::Parameters<P>,
    p: P,
) -> Option<usize> {
    let set = params
        .get(p)
        .ok()
        .and_then(|d| d.as_layer().ok().map(|l| l.value().is_some()))
        .unwrap_or(false);
    if set {
        params.index(p)
    } else {
        None
    }
}

/// PreRender for effects whose output covers the layer: asks for the input
/// grown by `reach` (pixels the effect pulls from), checks out optional layer
/// inputs `(param index, id)` with the same rect. (No GuidMixInPtr: AE only
/// allows it with the I_MIX_GUID_DEPENDENCIES flag and shows an error dialog
/// otherwise; stale cached frames are avoided by the per-build PiPL version.)
pub fn pre_render(
    in_data: &ae::InData,
    extra: &mut ae::PreRenderExtra,
    reach: (f32, f32),
    side: &[(Option<usize>, i32)],
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let mut in_req = req;
    in_req.rect = inflate(req.rect, reach.0.min(1e6) as i32, reach.1.min(1e6) as i32);
    let cb = extra.callbacks();
    let (t, step, scale) = (
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    );
    let src = cb.checkout_layer(0, 0, &in_req, t, step, scale)?;
    let mut checked = SideLayers::default();
    for &(index, id) in side {
        if let Some(idx) = index {
            cb.checkout_layer(idx as i32, id, &in_req, t, step, scale)?;
            checked.checked[id as usize] = true;
        }
    }
    extra.set_pre_render_data(checked);
    extra.set_result_rect(to_rect(intersect(src.result_rect, req.rect)));
    extra.set_max_result_rect(to_rect(src.max_result_rect));
    extra.set_returns_extra_pixels(false);
    Ok(())
}

/// SmartRender: hands the source (placed in layer space) and the optional
/// inputs (aligned to it, by id) to `f`, writes what it returns.
pub fn smart_render(
    extra: &ae::SmartRenderExtra,
    f: impl FnOnce(&Placed, &[Option<Image>; 4]) -> Image,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let Some(input) = cb.checkout_layer_pixels(0)? else {
        return Ok(());
    };
    let Some(mut output) = cb.checkout_output()? else {
        return Ok(());
    };
    let src = read(&input);
    let _ = cb.checkin_layer_pixels(0);
    let checked = extra
        .pre_render_data::<SideLayers>()
        .copied()
        .unwrap_or_default();
    let mut side: [Option<Image>; 4] = Default::default();
    for id in 1..4 {
        if checked.checked[id] {
            if let Ok(Some(l)) = cb.checkout_layer_pixels(id as u32) {
                side[id] = Some(align(&read(&l), &src));
                let _ = cb.checkin_layer_pixels(id as u32);
            }
        }
    }
    let img = f(&src, &side);
    if let Some(i) = img
        .data
        .iter()
        .position(|px| px.iter().any(|v| !v.is_finite()))
    {
        error_log(&format!("non-finite output at {i} of {}x{}", img.w, img.h));
    }
    write(
        &Placed {
            img,
            x: src.x,
            y: src.y,
        },
        &mut output,
    );
    Ok(())
}

// ─────────────────────────── other frames ───────────────────────────

/// Checkout ids of other source frames: ID_FRAMES + index into the frame list.
pub const ID_FRAMES: i32 = 16;

/// What PreRender of a time effect checked out (passed on to SmartRender).
#[derive(Default, Clone, Debug)]
pub struct TimeData {
    pub side: SideLayers,
    /// Layer frame numbers, in checkout order.
    pub frames: Vec<i32>,
}

/// Other frames of the source, each aligned to the current frame's pixels.
pub struct FrameSet {
    pub frames: Vec<(i32, Image)>,
}

impl FrameSet {
    pub fn get(&self, n: i32) -> Option<&Image> {
        self.frames.iter().find(|(f, _)| *f == n).map(|(_, i)| i)
    }
}

/// Current layer frame number (layer time / frame duration) and frame duration in time units.
pub fn frame_number(in_data: &ae::InData) -> (f32, i32) {
    let step = if in_data.time_step() != 0 {
        in_data.time_step()
    } else {
        in_data.local_time_step()
    };
    let n = if step != 0 {
        in_data.current_time() as f32 / step as f32
    } else {
        0.0
    };
    (n, step)
}

/// Layer duration in frames.
pub fn total_frames(in_data: &ae::InData) -> f32 {
    let (_, step) = frame_number(in_data);
    let total = unsafe { (*in_data.as_ptr()).total_time };
    if step != 0 {
        total as f32 / step as f32
    } else {
        0.0
    }
}

/// PreRender for effects that also read other frames of their own layer:
/// like `pre_render`, plus a checkout of every frame in `frames` (layer frame
/// numbers; the current frame may be among them).
pub fn pre_render_frames(
    in_data: &ae::InData,
    extra: &mut ae::PreRenderExtra,
    reach: (f32, f32),
    side: &[(Option<usize>, i32)],
    frames: &[i32],
) -> Result<(), ae::Error> {
    let req = extra.output_request();
    let mut in_req = req;
    in_req.rect = inflate(req.rect, reach.0.min(1e6) as i32, reach.1.min(1e6) as i32);
    let cb = extra.callbacks();
    let (t, scale) = (in_data.current_time(), in_data.time_scale());
    let (_, step) = frame_number(in_data);
    let src = cb.checkout_layer(0, 0, &in_req, t, step, scale)?;
    let mut data = TimeData::default();
    for &(index, id) in side {
        if let Some(idx) = index {
            cb.checkout_layer(idx as i32, id, &in_req, t, step, scale)?;
            data.side.checked[id as usize] = true;
        }
    }
    let mut seen = std::collections::HashSet::new();
    for &f in frames {
        if !seen.insert(f) {
            continue;
        }
        let id = ID_FRAMES + data.frames.len() as i32;
        cb.checkout_layer(0, id, &in_req, f * step, step, scale)?;
        data.frames.push(f);
    }
    extra.set_pre_render_data(data);
    extra.set_result_rect(to_rect(intersect(src.result_rect, req.rect)));
    extra.set_max_result_rect(to_rect(src.max_result_rect));
    extra.set_returns_extra_pixels(false);
    Ok(())
}

/// SmartRender counterpart of `pre_render_frames`.
pub fn smart_render_frames(
    extra: &ae::SmartRenderExtra,
    f: impl FnOnce(&Placed, &[Option<Image>; 4], &FrameSet) -> Image,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let Some(input) = cb.checkout_layer_pixels(0)? else {
        return Ok(());
    };
    let Some(mut output) = cb.checkout_output()? else {
        return Ok(());
    };
    let src = read(&input);
    let _ = cb.checkin_layer_pixels(0);
    let data = extra
        .pre_render_data::<TimeData>()
        .cloned()
        .unwrap_or_default();
    let mut side: [Option<Image>; 4] = Default::default();
    for id in 1..4 {
        if data.side.checked[id] {
            if let Ok(Some(l)) = cb.checkout_layer_pixels(id as u32) {
                side[id] = Some(align(&read(&l), &src));
                let _ = cb.checkin_layer_pixels(id as u32);
            }
        }
    }
    let mut set = FrameSet {
        frames: Vec::with_capacity(data.frames.len()),
    };
    for (i, &n) in data.frames.iter().enumerate() {
        let id = (ID_FRAMES + i as i32) as u32;
        let img = match cb.checkout_layer_pixels(id) {
            Ok(Some(l)) => {
                let a = align(&read(&l), &src);
                let _ = cb.checkin_layer_pixels(id);
                a
            }
            _ => Image::new(src.img.w, src.img.h),
        };
        set.frames.push((n, img));
    }
    let img = f(&src, &side, &set);
    write(
        &Placed {
            img,
            x: src.x,
            y: src.y,
        },
        &mut output,
    );
    Ok(())
}

/// Non-SmartFX hosts: whole layer, no optional inputs.
pub fn legacy_render(
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
    f: impl FnOnce(&Placed) -> Image,
) -> Result<(), ae::Error> {
    let src = read(in_layer);
    let img = f(&src);
    write(
        &Placed {
            img,
            x: src.x,
            y: src.y,
        },
        out_layer,
    );
    Ok(())
}

// ───────────────────────────── rects ─────────────────────────────

pub fn inflate(r: ae::sys::PF_LRect, dx: i32, dy: i32) -> ae::sys::PF_LRect {
    ae::sys::PF_LRect {
        left: r.left - dx,
        top: r.top - dy,
        right: r.right + dx,
        bottom: r.bottom + dy,
    }
}

pub fn intersect(a: ae::sys::PF_LRect, b: ae::sys::PF_LRect) -> ae::sys::PF_LRect {
    let r = ae::sys::PF_LRect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    if r.right <= r.left || r.bottom <= r.top {
        ae::sys::PF_LRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    } else {
        r
    }
}

pub fn to_rect(r: ae::sys::PF_LRect) -> ae::Rect {
    ae::Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

// ───────────────────────────── params ─────────────────────────────

pub fn popup(options: &[&str], default: i32) -> ae::PopupDef<'static> {
    ae::PopupDef::setup(|d| {
        d.set_options(options);
        d.set_default(default);
        d.set_value(default);
    })
}

/// Float slider: `valid` is the hard range, `slider` the UI range.
pub fn slider(
    valid: (f32, f32),
    slider: (f32, f32),
    default: f64,
    precision: i16,
) -> ae::FloatSliderDef<'static> {
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

pub fn checkbox(default: bool) -> ae::CheckBoxDef<'static> {
    ae::CheckBoxDef::setup(|d| {
        d.set_default(default);
        d.set_value(default);
    })
}

pub fn color(rgb: [u8; 3]) -> ae::ColorDef<'static> {
    ae::ColorDef::setup(|d| {
        let px = ae::Pixel8 {
            alpha: 255,
            red: rgb[0],
            green: rgb[1],
            blue: rgb[2],
        };
        d.set_default(px);
        d.set_value(px);
    })
}

pub fn get_float<P: Eq + PartialEq + std::hash::Hash + Copy + std::fmt::Debug>(
    params: &ae::Parameters<P>,
    p: P,
) -> f32 {
    params
        .get(p)
        .and_then(|v| v.as_float_slider().map(|d| d.value()))
        .unwrap_or(0.0) as f32
}

pub fn get_popup<P: Eq + PartialEq + std::hash::Hash + Copy + std::fmt::Debug>(
    params: &ae::Parameters<P>,
    p: P,
) -> i32 {
    params
        .get(p)
        .and_then(|v| v.as_popup().map(|d| d.value()))
        .unwrap_or(1)
}

pub fn get_bool<P: Eq + PartialEq + std::hash::Hash + Copy + std::fmt::Debug>(
    params: &ae::Parameters<P>,
    p: P,
) -> bool {
    params
        .get(p)
        .and_then(|v| v.as_checkbox().map(|d| d.value()))
        .unwrap_or(false)
}

/// Colour parameter as float RGB (16-bit precision when the host provides it).
pub fn get_color<P: Eq + PartialEq + std::hash::Hash + Copy + std::fmt::Debug>(
    params: &ae::Parameters<P>,
    p: P,
) -> [f32; 3] {
    params
        .get(p)
        .and_then(|v| v.as_color().map(|d| d.value()))
        .map(|c| {
            [
                c.red as f32 / 255.0,
                c.green as f32 / 255.0,
                c.blue as f32 / 255.0,
            ]
        })
        .unwrap_or([1.0; 3])
}

/// Horizontal and vertical downsample factors of the current render (1 = full res).
pub fn downsample(in_data: &ae::InData) -> (f32, f32) {
    let f = |r: ae::RationalScale| {
        if r.den != 0 {
            r.num as f32 / r.den as f32
        } else {
            1.0
        }
    };
    (f(in_data.downsample_x()), f(in_data.downsample_y()))
}
