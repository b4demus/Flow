//! Flicker family: Flicker (brightness wobble over time) and Flicker Remove /
//! Flicker Match (scale each frame so the average in a rectangle or under a
//! matte reaches a hold level, or follows another clip). spec/effects/time.md.
//!
//! Images in and out are straight colour; `AlphaMode` says how the effect
//! works on them (Normal: on premultiplied colour, as the original does).

use crate::glow::AlphaMode;
use crate::image::Image;
use rayon::prelude::*;

/// The original's monochrome (luma) weights.
pub const MONO: [f32; 3] = crate::colorops::SAT_LUMA;

#[inline]
pub fn mono(p: [f32; 4]) -> f32 {
    p[0] * MONO[0] + p[1] * MONO[1] + p[2] * MONO[2]
}

#[derive(Clone, Debug)]
pub struct FlickerParams {
    pub amplitude: f32,
    pub rand_luma_amp: f32,
    pub rand_color_amp: f32,
    pub rand_freq: f32,
    pub wave_amp: f32,
    pub wave_freq: f32,
    /// Per-channel wave phase (turns).
    pub wave_phase: [f32; 3],
    pub rgb_amp: [f32; 3],
    pub brightness: f32,
    pub seed: f32,
}

impl Default for FlickerParams {
    fn default() -> Self {
        Self {
            amplitude: 13107.0 / 65536.0,
            rand_luma_amp: 1.0,
            rand_color_amp: 0.0,
            rand_freq: 30.0,
            wave_amp: 0.0,
            wave_freq: 5.0,
            wave_phase: [0.0; 3],
            rgb_amp: [1.0; 3],
            brightness: 1.0,
            seed: 8060.0 / 65536.0,
        }
    }
}

/// Per-channel gain of Flicker at time `t` (s).
pub fn flicker_gain(p: &FlickerParams, t: f32) -> [f32; 3] {
    let x = t * p.rand_freq * 2.0;
    let luma = crate::shake::noise2(x, 0.0, 1, (p.seed * 4096.0) as i32);
    let col = crate::shake::noise2_rgb(x, 0.0, 1, ((p.seed + 0.3333) * 4096.0) as i32);
    let mut g = [0f32; 3];
    for c in 0..3 {
        let rand = (p.rand_luma_amp * (luma - 0.5) + p.rand_color_amp * (col[c] - 0.5)) * 2.0;
        let turns = t * p.wave_freq + (0.25 - p.wave_phase[c]);
        let wave = (turns * std::f32::consts::TAU).sin() * p.wave_amp;
        g[c] = p.brightness + p.amplitude * p.rgb_amp[c] * (rand + wave);
    }
    g
}

/// The image the effect works on: premultiplied (Normal), as is (As Premult)
/// or with alpha forced to 1 (All Opaque).
pub fn working(src: &Image, mode: AlphaMode) -> Image {
    let mut w = src.clone();
    match mode {
        AlphaMode::Normal => w.premultiply(),
        AlphaMode::AllOpaque => w.data.par_iter_mut().for_each(|p| p[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    w
}

/// Multiplies colour by per-channel gains (alpha untouched); `src` straight.
pub fn apply_gain(src: &Image, g: [f32; 3], mode: AlphaMode) -> Image {
    let mut out = src.clone();
    out.data.par_iter_mut().for_each(|p| {
        match mode {
            AlphaMode::AllOpaque => p[3] = 1.0,
            // Premultiplied work loses the colour under zero alpha.
            AlphaMode::Normal if p[3] == 0.0 => *p = [0.0; 4],
            _ => {}
        }
        p[0] *= g[0];
        p[1] *= g[1];
        p[2] *= g[2];
    });
    out
}

/// Measuring rectangle as whole pixel columns x0..x1 and rows y0..y1 (end exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x0: i32,
    pub x1: i32,
    pub y0: i32,
    pub y1: i32,
}

/// The pixels the original measures for a rectangle given by two corners in
/// layer pixels (any order) on a `w`×`h` image: centre ± half size, cut to
/// whole pixels toward zero; rows sit one pixel higher than the corners say.
pub fn rect_bounds(c1: (f64, f64), c2: (f64, f64), w: usize, h: usize) -> Bounds {
    // In double: corners that land on whole pixels must not drop one.
    let (cx, cy) = ((c1.0 + c2.0) * 0.5, (c1.1 + c2.1) * 0.5);
    let (hw, hh) = ((c1.0 - c2.0).abs() * 0.5, (c1.1 - c2.1).abs() * 0.5);
    let cy = cy - 1.0;
    let (mut x0, mut x1) = ((cx - hw) as i32, (cx + hw) as i32);
    let (mut y0, mut y1) = ((cy - hh) as i32, (cy + hh) as i32);
    if x1 < x0 {
        std::mem::swap(&mut x0, &mut x1);
    }
    if y1 < y0 {
        std::mem::swap(&mut y0, &mut y1);
    }
    let (w, h) = (w as i32, h as i32);
    let x0 = x0.clamp(0, w - 1);
    let y0 = y0.clamp(0, h - 1);
    Bounds {
        x0,
        x1: x1.clamp(x0 + 1, w),
        y0,
        y1: y1.clamp(y0 + 1, h),
    }
}

/// Average of `f(pixel)` over the rectangle.
pub fn average_in_rect(
    img: &Image,
    b: Bounds,
    f: impl Fn([f32; 4]) -> [f32; 3] + Sync,
) -> [f32; 3] {
    let w = img.w as i32;
    let sum = (b.y0..b.y1)
        .into_par_iter()
        .map(|y| {
            let mut s = [0f64; 3];
            for x in b.x0..b.x1 {
                let v = f(img.data[(y * w + x) as usize]);
                for k in 0..3 {
                    s[k] += v[k] as f64;
                }
            }
            s
        })
        .reduce(|| [0f64; 3], |p, q| [p[0] + q[0], p[1] + q[1], p[2] + q[2]]);
    let n = ((b.x1 - b.x0) * (b.y1 - b.y0)) as f64;
    sum.map(|v| (v / n) as f32)
}

/// Σ f·m / Σ m over the whole image (m = matte value per pixel).
pub fn average_in_matte(
    img: &Image,
    matte: &[f32],
    f: impl Fn([f32; 4]) -> [f32; 3] + Sync,
) -> [f32; 3] {
    let (s, m) = img
        .data
        .par_iter()
        .zip(matte.par_iter())
        .map(|(p, &m)| {
            let v = f(*p);
            (
                [
                    v[0] as f64 * m as f64,
                    v[1] as f64 * m as f64,
                    v[2] as f64 * m as f64,
                ],
                m as f64,
            )
        })
        .reduce(
            || ([0f64; 3], 0f64),
            |a, b| {
                (
                    [a.0[0] + b.0[0], a.0[1] + b.0[1], a.0[2] + b.0[2]],
                    a.1 + b.1,
                )
            },
        );
    s.map(|v| (v / m) as f32)
}

/// What Flicker Remove / Match measure: the monochrome level or the colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Level,
    Color,
}

pub fn measure_fn(m: Measure) -> impl Fn([f32; 4]) -> [f32; 3] + Sync {
    move |p| match m {
        Measure::Level => [mono(p); 3],
        Measure::Color => [p[0], p[1], p[2]],
    }
}

/// Which matte value a matte image gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatteUse {
    Luma,
    Alpha,
    /// The red channel: what the original weighs by when no matte layer is
    /// given (except Flicker Match Matte, which uses the Match alpha).
    Red,
}

/// Matte use when the matte may be missing: without a matte layer the
/// original weighs by the red channel of the measured clip where Alpha is
/// asked for; Flicker Match Matte keeps the Match clip's alpha.
pub fn default_matte_use(have_matte: bool, match_matte: bool, asked: MatteUse) -> MatteUse {
    if have_matte || match_matte || asked != MatteUse::Alpha {
        asked
    } else {
        MatteUse::Red
    }
}

/// Matte values of `img` (already in working form), optionally inverted.
pub fn matte_values(img: &Image, use_: MatteUse, invert: bool) -> Vec<f32> {
    img.data
        .par_iter()
        .map(|p| {
            let m = match use_ {
                MatteUse::Luma => mono(*p),
                MatteUse::Alpha => p[3],
                MatteUse::Red => p[0],
            };
            if invert {
                1.0 - m
            } else {
                m
            }
        })
        .collect()
}

/// Where the average is taken.
#[derive(Clone, Debug)]
pub enum Region<'a> {
    Rect(Bounds),
    /// Matte image (straight, same size), use and invert.
    Matte(&'a Image, MatteUse, bool),
}

/// Average of `img` (straight) in working form over the region.
pub fn measure(img: &Image, region: &Region, what: Measure, mode: AlphaMode) -> [f32; 3] {
    let w = working(img, mode);
    match region {
        Region::Rect(b) => average_in_rect(&w, *b, measure_fn(what)),
        Region::Matte(m, use_, inv) => {
            let mv = matte_values(&working(m, mode), *use_, *inv);
            average_in_matte(&w, &mv, measure_fn(what))
        }
    }
}

/// Flicker Remove: gain = hold / max(0.01, average).
pub fn remove_gain(hold: [f32; 3], ave: [f32; 3]) -> [f32; 3] {
    [
        hold[0] / ave[0].max(0.01),
        hold[1] / ave[1].max(0.01),
        hold[2] / ave[2].max(0.01),
    ]
}

/// Flicker Match: gain = average of the match clip / level.
pub fn match_gain(level: [f32; 3], ave: [f32; 3]) -> [f32; 3] {
    [ave[0] / level[0], ave[1] / level[1], ave[2] / level[2]]
}

/// Colour parameters reach the original as 8-bit values (its default grey is 127/255).
pub fn color8(v: f32) -> f32 {
    (v * 255.0).clamp(0.0, 255.0).round() / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rect_matches_original() {
        // AE defaults on a 640x360 layer, measured in AE: columns 128..505, rows 71..283.
        let b = rect_bounds((128.0, 284.4), (505.6, 72.0), 640, 360);
        assert_eq!(
            b,
            Bounds {
                x0: 128,
                x1: 505,
                y0: 71,
                y1: 283
            }
        );
        let b = rect_bounds((100.0, 50.0), (300.6, 200.4), 640, 360);
        assert_eq!(
            b,
            Bounds {
                x0: 100,
                x1: 300,
                y0: 49,
                y1: 199
            }
        );
    }
}
