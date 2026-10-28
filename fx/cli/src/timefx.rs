//! Offline side of the time effects for `facet frames`: rebuilds the probe's
//! source clip (a still file moved by "move" whole pixels per frame over an
//! opaque black solid, "duration" seconds at 25 fps) and renders our effects.

use facet_core::flicker::{self, FlickerParams, MatteUse, Measure, Region};
use facet_core::glow::AlphaMode;
use facet_core::temporal::{
    from_working, to_working, Clip, FieldSwap, Frames, Pick, TimeAverage, WarpRgb,
};
use facet_core::{shake, Image};
use serde_json::Value;
use std::collections::HashMap;

pub const FPS: f32 = 25.0;

/// The probe's source clip.
pub struct Source<'a> {
    pub still: &'a Image,
    pub mv: (i32, i32),
    pub frames_total: i32,
    pub bg: [f32; 4],
}

impl Source<'_> {
    pub fn frame(&self, n: i32) -> Image {
        let (w, h) = (self.still.w, self.still.h);
        if n < 0 || n >= self.frames_total {
            return Image::new(w, h);
        }
        let (dx, dy) = (self.mv.0 * n, self.mv.1 * n);
        if self.bg[3] == 0.0 {
            // Transparent clip: the still as it is (straight colour kept under zero alpha).
            let mut out = Image::new(w, h);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (sx, sy) = (x - dx, y - dy);
                    if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                        out.data[(y * w as i32 + x) as usize] =
                            self.still.data[(sy * w as i32 + sx) as usize];
                    }
                }
            }
            return out;
        }
        let mut out = Image::filled(w, h, self.bg);
        for y in 0..h as i32 {
            let sy = y - dy;
            if sy < 0 || sy >= h as i32 {
                continue;
            }
            for x in 0..w as i32 {
                let sx = x - dx;
                if sx >= 0 && sx < w as i32 {
                    let p = self.still.data[(sy * w as i32 + sx) as usize];
                    // the still over the backdrop
                    let a = p[3];
                    let o = &mut out.data[(y * w as i32 + x) as usize];
                    for c in 0..3 {
                        o[c] = p[c] * a + o[c] * (1.0 - a);
                    }
                    o[3] = a + o[3] * (1.0 - a);
                }
            }
        }
        // Straight colour, as the effects take it.
        out.unpremultiply();
        out
    }
}

struct Cache(HashMap<i32, Image>);

impl Frames for Cache {
    fn get(&self, n: i32) -> &Image {
        &self.0[&n]
    }
}

fn fetch(src: &Source, need: &[i32], mode: AlphaMode) -> Cache {
    let mut m = HashMap::new();
    for &n in need {
        m.entry(n).or_insert_with(|| to_working(src.frame(n), mode));
    }
    Cache(m)
}

fn fields(v: i32) -> FieldSwap {
    match v {
        2 => FieldSwap::Ave,
        3 => FieldSwap::Up1,
        4 => FieldSwap::Down1,
        _ => FieldSwap::No,
    }
}

/// Effect name as AE shows it ("F_GlowDarks"); older configs say "Facet Glow Darks".
pub fn canon(name: &str) -> String {
    match name.strip_prefix("Facet ") {
        Some(rest) => format!("F_{}", rest.replace(' ', "")),
        None => name.to_string(),
    }
}

/// Layer inputs of an effect by parameter name ("Match", "Matte from Layer").
pub type Layers<'a> = HashMap<String, Source<'a>>;

/// A probe run: its "params" and the whole run (for "exprs").
pub struct P<'a> {
    pub params: &'a serde_json::Map<String, Value>,
    pub run: &'a Value,
}

impl P<'_> {
    /// Slider value as the original stores it (16.16 fixed).
    pub fn f(&self, name: &str, d: f32) -> f32 {
        self.params
            .get(name)
            .and_then(|v| v.as_f64())
            .map(|v| shake::fixed(v as f32))
            .unwrap_or(d)
    }
    pub fn i(&self, name: &str, d: i32) -> i32 {
        self.params
            .get(name)
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(d)
    }
}

/// Renders a time effect at `time` (s). None: not a time effect.
pub fn render(
    effect: &str,
    src: &Source,
    p: &P,
    time: f32,
    layers: &Layers,
) -> Option<Result<Image, String>> {
    let clip = Clip {
        frame: time * FPS,
        total: src.frames_total as f32,
    };
    let cur = || src.frame(clip.frame as i32);
    let mode = alpha_mode(p);
    let pick = |pk: Pick| {
        let c = fetch(src, &pk.needs(clip), mode);
        Ok(from_working(pk.render(clip, &c), mode))
    };
    let r = match effect {
        "F_FreezeFrame" => pick(Pick::FreezeFrame {
            start: p.f("Frame Start", 0.0),
            frames: p.f("Freeze Frames", 5.0),
        }),
        "F_GetFrame" => pick(Pick::GetFrame {
            frame: p.f("Get Frame", 0.0),
            interp: p.i("Interp Frames", 0) != 0,
        }),
        "F_JitterFrames" => pick(Pick::JitterFrames {
            len: p.f("Edit Frame Length", 1.0),
            dist: p.f("Jitter Frame Dist", 10.0),
            seed: p.f("Seed", 8060.0 / 65536.0),
        }),
        "F_RandomEdits" => pick(Pick::RandomEdits {
            len: p.f("Edit Frame Length", 2.0),
            seed: p.f("Seed", 8060.0 / 65536.0),
        }),
        "F_ReverseClip" => pick(Pick::ReverseClip {
            fields: fields(p.i("Swap Fields", 3)),
        }),
        "F_ReverseEdits" => pick(Pick::ReverseEdits {
            len: p.f("Edit Frame Length", 5.0),
            start: p.f("Edits Start", 0.0),
            reverse_all: p.i("Reverse All", 0) != 0,
            fields: fields(p.i("Swap Fields", 1)),
        }),
        "F_TimeWarpRGB" => {
            let mode = match p.i("Input Opacity", 2) {
                1 => AlphaMode::AllOpaque,
                3 => AlphaMode::AsPremult,
                _ => AlphaMode::Normal,
            };
            let opaque = p.i("Output Opacity", 1) == 1;
            let fx = WarpRgb {
                shift: [
                    p.f("Red Shift Frames", 1.0),
                    p.f("Green Shift Frames", 0.0),
                    p.f("Blue Shift Frames", -1.0),
                ],
                clamp_chroma: p.f("Clamp Chroma", 1.0),
                opaque,
            };
            let c = fetch(src, &fx.needs(clip), mode);
            Ok(fx.render(clip, &to_working(cur(), mode), &c))
        }
        "F_TimeAverage" => {
            let fx = TimeAverage {
                frames: p.f("Ave Over Frames", 10.0),
                brightness: p.f("Brightness", 1.0),
            };
            let c = fetch(src, &fx.needs(clip), mode);
            Ok(from_working(
                fx.render(clip, &to_working(cur(), mode), &c),
                mode,
            ))
        }
        e if e.starts_with("F_Flicker") && e != "F_Flicker" => {
            flicker_family(e, src, p, clip, layers)
        }
        "F_Flicker" => {
            let d = FlickerParams::default();
            let fp = FlickerParams {
                amplitude: p.f("Amplitude", d.amplitude),
                rand_luma_amp: p.f("Rand Luma Amp", 1.0),
                rand_color_amp: p.f("Rand Color Amp", 0.0),
                rand_freq: p.f("Rand Freq", 30.0),
                wave_amp: p.f("Wave Amp", 0.0),
                wave_freq: p.f("Wave Freq", 5.0),
                wave_phase: [
                    p.f("Wave R Phase", 0.0),
                    p.f("Wave G Phase", 0.0),
                    p.f("Wave B Phase", 0.0),
                ],
                rgb_amp: [
                    p.f("Red Amp", 1.0),
                    p.f("Green Amp", 1.0),
                    p.f("Blue Amp", 1.0),
                ],
                brightness: p.f("Brightness", 1.0),
                seed: p.f("Seed", d.seed),
            };
            Ok(flicker::apply_gain(
                &cur(),
                flicker::flicker_gain(&fp, time),
                AlphaMode::AsPremult,
            ))
        }
        _ => return None,
    };
    Some(r)
}

fn alpha_mode(p: &P) -> AlphaMode {
    match p.i("Opacity", 2) {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    }
}

/// A point parameter given as an expression "[x,y]" in the probe ("exprs").
fn point(run: &Value, name: &str, d: (f64, f64)) -> (f64, f64) {
    run.get("exprs")
        .and_then(|e| e.get(name))
        .and_then(|v| v.as_str())
        .and_then(|s| {
            let v: Vec<f64> = s
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            (v.len() == 2).then(|| (v[0], v[1]))
        })
        .unwrap_or(d)
}

/// A colour parameter given as an expression "[r,g,b,a]" (8-bit in the original).
fn color(run: &Value, name: &str) -> [f32; 3] {
    run.get("exprs")
        .and_then(|e| e.get(name))
        .and_then(|v| v.as_str())
        .map(|s| {
            let v: Vec<f32> = s
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            [0, 1, 2].map(|i| flicker::color8(v.get(i).copied().unwrap_or(0.5)))
        })
        .unwrap_or([127.0 / 255.0; 3])
}

/// Flicker Remove / Match and their Color / Matte variants.
fn flicker_family(
    effect: &str,
    src: &Source,
    p: &P,
    clip: Clip,
    layers: &Layers,
) -> Result<Image, String> {
    let name = effect.trim_start_matches("F_Flicker");
    let color_v = name.contains("Color");
    let matte_v = name.contains("Matte");
    let remove = name.starts_with("Remove") || name.starts_with("Rm");
    let what = if color_v {
        Measure::Color
    } else {
        Measure::Level
    };
    let mode = alpha_mode(p);
    let n = clip.frame as i32;
    let img = src.frame(n);
    let layer = |k: &str| layers.get(k).map(|s| s.frame(n));
    // Remove measures the source itself, Match the Match clip (black when missing).
    let measured = if remove {
        img.clone()
    } else {
        layer("Match").unwrap_or_else(|| Image::new(img.w, img.h))
    };
    let matte_img;
    let region = if matte_v {
        let given = layer("Matte from Layer");
        let use_ = if p.i("Matte Use", 2) == 1 {
            MatteUse::Luma
        } else {
            MatteUse::Alpha
        };
        let use_ = flicker::default_matte_use(given.is_some(), name == "MatchMatte", use_);
        matte_img = given.unwrap_or_else(|| measured.clone());
        Region::Matte(&matte_img, use_, p.i("Invert Matte", 0) != 0)
    } else {
        let (w, h) = (img.w as f64, img.h as f64);
        let c1 = point(p.run, "Rect Corner1", (0.2 * w, 0.79 * h));
        let c2 = point(p.run, "Rect Corner2", (0.79 * w, 0.2 * h));
        Region::Rect(flicker::rect_bounds(c1, c2, img.w, img.h))
    };
    let ave = flicker::measure(&measured, &region, what, mode);
    let level = if color_v {
        color(p.run, if remove { "Hold Color" } else { "Match Color" })
    } else {
        [p.f(if remove { "Hold Level" } else { "Match Level" }, 0.5); 3]
    };
    let g = if remove {
        flicker::remove_gain(level, ave)
    } else {
        flicker::match_gain(level, ave)
    };
    Ok(flicker::apply_gain(&img, g, mode))
}
