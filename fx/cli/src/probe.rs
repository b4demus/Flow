//! Offline side of the AE probes: renders our effects on the same test file and
//! samples the same points as tools/ae/probe.jsx, so AE only renders the
//! reference once and our side needs no AE restart (or its frame cache).
//!
//! facet gen-noise <out.png> <w> <h> [seed]       16-bit test image (fractal value noise)
//! facet probe <cfg.json> <out.json>              runs with an "effect" starting "F_"
//!                                                and source "file"

use facet_core::composite::Combine;
use facet_core::edgerays::{self, EdgeRaysParams, Show};
use facet_core::glint::{self, GlintParams, MatteType};
use facet_core::glow::{AlphaMode, Mix};
use facet_core::shake::{self, Axis, ShakeParams, Style, Wrap};
use facet_core::Image;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |a: f32| a * a * (3.0 - 2.0 * a);
    let r = |i: i32, j: i32| {
        hash((i as u32).wrapping_mul(73856093) ^ (j as u32).wrapping_mul(19349663) ^ seed) as f32
            / u32::MAX as f32
    };
    let (a, b, c, d) = (r(xi, yi), r(xi + 1, yi), r(xi, yi + 1), r(xi + 1, yi + 1));
    let (u, v) = (s(fx), s(fy));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// Contrasty fractal noise in 0..1 with sparse bright blobs (glints and rays need highlights).
pub fn gen_noise(out: &Path, w: usize, h: usize, seed: u32) -> Result<(), String> {
    let buf = ::image::ImageBuffer::<::image::Rgba<u16>, _>::from_fn(w as u32, h as u32, |x, y| {
        let (mut v, mut amp, mut f) = (0.0f32, 0.5f32, 1.0 / 48.0);
        for o in 0..5 {
            v += amp * value_noise(x as f32 * f, y as f32 * f, seed.wrapping_add(o * 977));
            amp *= 0.5;
            f *= 2.0;
        }
        let v = ((v - 0.35) * 2.2).clamp(0.0, 1.0);
        let q = (v * 65535.0 + 0.5) as u16;
        ::image::Rgba([q, q, q, 65535])
    });
    buf.save(out).map_err(|e| format!("{}: {e}", out.display()))
}

fn load16(p: &str) -> Result<Image, String> {
    let img = ::image::open(p)
        .map_err(|e| format!("{p}: {e}"))?
        .to_rgba16();
    let (w, h) = img.dimensions();
    let k = 1.0 / 65535.0;
    let data = img
        .pixels()
        .map(|p| {
            [
                p[0] as f32 * k,
                p[1] as f32 * k,
                p[2] as f32 * k,
                p[3] as f32 * k,
            ]
        })
        .collect();
    Ok(Image {
        w: w as usize,
        h: h as usize,
        data,
    })
}

struct Named<'a>(&'a serde_json::Map<String, Value>);

impl Named<'_> {
    fn f(&self, name: &str, default: f32) -> f32 {
        self.0
            .get(name)
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(default)
    }
    fn i(&self, name: &str, default: i32) -> i32 {
        self.0
            .get(name)
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(default)
    }
}

fn alpha_mode(v: i32) -> AlphaMode {
    match v {
        1 => AlphaMode::AllOpaque,
        3 => AlphaMode::AsPremult,
        _ => AlphaMode::Normal,
    }
}

/// AE parameter names and defaults of F_Glint (as in ae/glint).
fn glint_params(n: &Named) -> GlintParams {
    GlintParams {
        brightness: n.f("Brightness", 1.0),
        hue_shift: n.f("Hue Shift", 0.0),
        scale_colors: [1.0; 3],
        brightness_dir: [
            n.f("Brightness X", 1.0),
            n.f("Brightness Y", 1.0),
            n.f("Brightness Diag1", 1.0),
            n.f("Brightness Diag2", 1.0),
        ],
        threshold: n.f("Threshold", 0.7),
        threshold_add: [0.0; 3],
        threshold_blur: n.f("Threshold Blur", 5.376),
        size: n.f("Size", 480.0),
        size_dir: [
            n.f("Size X", 1.0),
            n.f("Size Y", 1.0),
            n.f("Size Diag1", 0.75),
            n.f("Size Diag2", 0.75),
        ],
        size_rgb: [
            n.f("Size Red", 0.5),
            n.f("Size Green", 1.0),
            n.f("Size Blue", 1.5),
        ],
        blur_glint: n.f("Blur Glint", 0.0),
        from_alpha: n.f("Glint From Alpha", 0.0),
        invert_matte: false,
        matte_type: MatteType::Luma,
        edges: glint::EDGES,
        mix: Mix {
            combine: Combine::Screen,
            affect_alpha: n.f("Affect Alpha", 1.0),
            under_source: n.f("Glint Under Source", 0.0),
            light_background: 0.0,
            source_opacity: n.f("Source Opacity", 1.0),
            bg_brightness: n.f("Bg Brightness", 1.0),
            alpha_mode: alpha_mode(n.i("Opacity", 2)),
        },
    }
}

/// AE parameter names and defaults of F_EdgeRays; the centre defaults to the frame centre.
fn edgerays_params(n: &Named, w: usize, h: usize) -> EdgeRaysParams {
    let d = EdgeRaysParams::default();
    let off: f32 = std::env::var("FACET_ER_COFF")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-0.5);
    EdgeRaysParams {
        center: [w as f32 / 2.0 + off, h as f32 / 2.0 + off],
        length: n.f("Rays Length", 0.25),
        length_rgb: [
            n.f("Length Red", 1.0),
            n.f("Length Green", 1.0),
            n.f("Length Blue", 1.0),
        ],
        reverse: n.f("Reverse Rays", 0.0),
        brightness: n.f("Rays Brightness", 2.0),
        blur: n.f("Blur Rays", 0.0),
        bias_outer: n.f("Bias Outer Bright", 0.0),
        show: match n.i("Show", 1) {
            2 => Show::Edges,
            3 => Show::Rays,
            _ => Show::Result,
        },
        edge_thickness: n.f("Edge Thickness", 10.56),
        edge_brightness: n.f("Edge Brightness", 1.0),
        shimmer_amp: n.f("Shimmer Amp", 0.5),
        shimmer_freq: n.f("Shimmer Freq", 40.0),
        layer_width: w as f32,
        res: match n.i("Rays Res", 1) {
            2 => 2,
            3 => 4,
            _ => 1,
        },
        from_alpha: n.f("Rays From Alpha", 0.0),
        mix: Mix {
            source_opacity: n.f("Source Opacity", 1.0),
            under_source: n.f("Rays Under Source", 0.0),
            affect_alpha: n.f("Affect Alpha", 1.0),
            ..d.mix.clone()
        },
        ..d
    }
}

/// Pixel-valued parameters (shown in pixels in AE) -> normalised units.
/// FACET_SHAKE_NORM=<px> overrides the reference half-width (default: half the layer width).
fn shake_norm(w: usize) -> f32 {
    std::env::var("FACET_SHAKE_NORM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(w as f32 / 2.0)
}

/// AE parameter names of F_Shake / Dissolve Shake (as in ae/shake); defaults from `d`.
fn shake_params(n: &Named, w: usize, d: &ShakeParams) -> ShakeParams {
    let k = 1.0 / shake_norm(w);
    // Slider values are 16.16 fixed point in the original.
    let f = |name: &str, d: f32| shake::fixed(n.f(name, d));
    let axis = |a: &str, ra: f32, scale: f32| Axis {
        rand_amp: f(&format!("{a} Rand Amp"), ra) * scale,
        rand_freq: f(&format!("{a} Rand Freq"), 1.0),
        wave_amp: f(&format!("{a} Wave Amp"), 0.0) * scale,
        wave_freq: f(&format!("{a} Wave Freq"), 0.5),
        phase: f(&format!("{a} Phase"), 0.0),
    };
    let wrap = |v: i32| match v {
        1 => Wrap::No,
        2 => Wrap::Tile,
        _ => Wrap::Reflect,
    };
    ShakeParams {
        style: match n.i("Style", 1) {
            2 => Style::Twitchy,
            3 => Style::Jumpy,
            _ => Style::Normal,
        },
        amplitude: f("Amplitude", d.amplitude),
        frequency: f("Frequency", d.frequency),
        phase: f("Phase", 0.0),
        stillness: f("Stillness", shake::STILLNESS),
        twitch_frequency: f("Twitch Frequency", 2.0),
        drift: f("Drift", shake::DRIFT),
        center_bias: f("Center Bias", 0.0),
        z_dist: f("Z Dist", 1.0),
        motion_blur: n.i("Motion Blur", d.motion_blur as i32) != 0,
        mo_blur_length: f("Mo Blur Length", d.mo_blur_length),
        seed: f("Seed", 0.0),
        wrap: [wrap(n.i("Wrap X", 3)), wrap(n.i("Wrap Y", 3))],
        x: axis("X", 192.0, k),
        y: axis("Y", 96.0, k),
        z: axis("Z", 0.0, k),
        tilt: axis("Tilt", 0.0, 1.0),
        rgb_amp: [
            f("Red Amplitude", 1.0),
            f("Green Amplitude", 1.0),
            f("Blue Amplitude", 1.0),
        ],
        rgb_phase: [
            f("Red Phase", 0.0),
            f("Green Phase", 0.0),
            f("Blue Phase", 0.0),
        ],
        rgb_randomness: f("RGB Randomness", 0.0),
        rgb_frequency: f("RGB Frequency", 2.0),
        alpha_mode: alpha_mode(n.i("Opacity", 2)),
    }
}

fn render(
    effect: &str,
    src: &Image,
    bg: Option<&Image>,
    params: &Named,
    time: f32,
) -> Result<Image, String> {
    match effect {
        "F_Glint" => Ok(glint::render(src, None, None, &glint_params(params))),
        "F_EdgeRays" => Ok(edgerays::render(
            src,
            None,
            None,
            &edgerays_params(params, src.w, src.h),
        )),
        "F_Shake" => {
            let fr = shake::Frame {
                w: src.w as f32,
                h: src.h as f32,
                src_x: 0,
                src_y: 0,
            };
            Ok(shake::render(
                src,
                &fr,
                &shake_params(params, src.w, &ShakeParams::default()),
                time,
                25.0,
            ))
        }
        "F_DissolveShake" => {
            let fr = shake::Frame {
                w: src.w as f32,
                h: src.h as f32,
                src_x: 0,
                src_y: 0,
            };
            let p = shake_params(params, src.w, &shake::dissolve_shake_defaults());
            let d = shake::DissolveParams {
                on_from_bg: params.i("Transition Dir", 1) == 2,
                amount: if params.i("Auto Trans", 1) == 2 {
                    params.f("Percent Time", 0.0)
                } else {
                    shake::fixed(params.f("Dissolve Percent", 0.0)) / 100.0
                },
                speed: shake::fixed(params.f("Dissolve Speed", 3.0)),
            };
            Ok(shake::render_dissolve(src, bg, &fr, &p, &d, time, 25.0))
        }
        _ => Err(format!("no offline renderer for {effect}")),
    }
}

/// Whole-frame comparison: every "<name>@ours" run rendered offline against the
/// frame AE rendered for "<name>@ref". Prints max / mean error and PSNR, writes
/// ours_<name>.png next to AE's frames.
pub fn frames(cfg: &Path, ae_result: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let cfg: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let dir = cfg["render"].as_str().ok_or("cfg has no render folder")?;
    let res: Value =
        serde_json::from_str(&std::fs::read_to_string(ae_result).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut nums: HashMap<String, i64> = HashMap::new();
    for r in res["runs"].as_array().into_iter().flatten() {
        if let (Some(id), Some(f)) = (r["id"].as_str(), r["frame"].as_i64()) {
            nums.insert(id.to_string(), f);
        }
    }
    let mut files: HashMap<String, Image> = HashMap::new();
    for run in cfg["runs"].as_array().into_iter().flatten() {
        let id = run["id"].as_str().unwrap_or("");
        let Some(name) = id.strip_suffix("@ours") else {
            continue;
        };
        let Some(&f) = nums.get(&format!("{name}@ref")) else {
            println!("{name:<28} (no reference frame)");
            continue;
        };
        let file = run["file"].as_str().ok_or("run without file")?;
        if !files.contains_key(file) {
            files.insert(file.to_string(), load16(file)?);
        }
        let empty = serde_json::Map::new();
        let params = Named(
            run.get("params")
                .and_then(|v| v.as_object())
                .unwrap_or(&empty),
        );
        let time = run.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let effect = &crate::timefx::canon(run["effect"].as_str().unwrap_or(""));
        // "bgLayer": the Background input is an opaque solid of that colour.
        let bg = run.get("bgLayer").and_then(|v| v.as_array()).map(|c| {
            let v = |i: usize| c.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            let s = &files[file];
            Image::filled(s.w, s.h, [v(0), v(1), v(2), 1.0])
        });
        // "layers": {param: {file, move, transparent}} (missing fields from the run).
        let empty_layers = serde_json::Map::new();
        let layer_specs = run
            .get("layers")
            .and_then(|v| v.as_object())
            .unwrap_or(&empty_layers);
        for spec in layer_specs.values() {
            let lf = spec.get("file").and_then(|v| v.as_str()).unwrap_or(file);
            if !files.contains_key(lf) {
                files.insert(lf.to_string(), load16(lf)?);
            }
        }
        // Time effects see the probe's whole source clip.
        let mv = run
            .get("move")
            .and_then(|v| v.as_array())
            .map(|a| {
                (
                    a[0].as_i64().unwrap_or(0) as i32,
                    a[1].as_i64().unwrap_or(0) as i32,
                )
            })
            .unwrap_or((0, 0));
        let duration = run
            .get("duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(time as f64 + 1.0);
        let transparent = run
            .get("transparent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut exposed_still = files[file].clone();
        let source_gain = run.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        if source_gain != 1.0 {
            for pixel in &mut exposed_still.data {
                for channel in &mut pixel[..3] {
                    *channel *= source_gain;
                }
            }
        }
        let clip = crate::timefx::Source {
            still: &exposed_still,
            mv,
            frames_total: (duration * crate::timefx::FPS as f64).round() as i32,
            bg: if transparent {
                [0.0; 4]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            },
        };
        let mut layers = crate::timefx::Layers::new();
        for (k, spec) in layer_specs {
            let get = |f: &str| spec.get(f).or_else(|| run.get(f));
            let lf = spec.get("file").and_then(|v| v.as_str()).unwrap_or(file);
            let lmv = get("move")
                .and_then(|v| v.as_array())
                .map(|a| {
                    (
                        a[0].as_i64().unwrap_or(0) as i32,
                        a[1].as_i64().unwrap_or(0) as i32,
                    )
                })
                .unwrap_or((0, 0));
            let lt = get("transparent")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            layers.insert(
                k.clone(),
                crate::timefx::Source {
                    still: &files[lf],
                    mv: lmv,
                    frames_total: clip.frames_total,
                    bg: if lt { [0.0; 4] } else { [0.0, 0.0, 0.0, 1.0] },
                },
            );
        }
        let tp = crate::timefx::P {
            params: params.0,
            run,
        };
        let cur = clip.frame((time * crate::timefx::FPS) as i32);
        let mut ours = match crate::timefx::render(effect, &clip, &tp, time, &layers)
            .or_else(|| {
                crate::fx2::render(
                    effect,
                    &cur,
                    &tp,
                    &layers,
                    (time * crate::timefx::FPS) as i32,
                )
            })
            .or_else(|| {
                crate::fx3::render(
                    effect,
                    &cur,
                    &tp,
                    &layers,
                    (time * crate::timefx::FPS) as i32,
                )
            })
            .or_else(|| {
                crate::fx4::render(
                    effect,
                    &clip,
                    &tp,
                    &layers,
                    (time * crate::timefx::FPS) as i32,
                )
            }).or_else(|| crate::fx5::render(effect,&clip,&tp,&layers,(time*crate::timefx::FPS) as i32)) {
            Some(r) => r?,
            None => render(effect, &files[file], bg.as_ref(), &params, time)?,
        };
        let raw_sample_image=run.get("grid").map(|_|ours.clone());
        ours.data
            .iter_mut()
            .for_each(|p| p.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0)));
        let reference = load16(&format!("{dir}/frame_{f:04}.png"))?;
        if (reference.w, reference.h) != (ours.w, ours.h) {
            return Err(format!(
                "{name}: size {}x{} vs {}x{}",
                reference.w, reference.h, ours.w, ours.h
            ));
        }
        let (mut max, mut sum, mut se, mut worst) = ([0f32; 4], 0f64, 0f64, 0usize);
        let mut wv = 0f32;
        for (i, (a, b)) in reference.data.iter().zip(&ours.data).enumerate() {
            for c in 0..4 {
                let d = (a[c] - b[c]).abs();
                max[c] = max[c].max(d);
                if c < 3 {
                    sum += d as f64;
                    se += (d * d) as f64;
                }
                if d > wv {
                    wv = d;
                    worst = i;
                }
            }
        }
        let n = (reference.data.len() * 3) as f64;
        let psnr = if se <= 1e-12 {
            99.0
        } else {
            10.0 * (n / se).log10()
        };
        let (wx, wy) = (worst % ours.w, worst / ours.w);
        println!(
            "{name:<28} max R {:.4} G {:.4} B {:.4} A {:.4} | mean {:.5} | PSNR {psnr:.1} | worst ({wx},{wy}) ref {:?} ours {:?}",
            max[0], max[1], max[2], max[3], sum / n, reference.data[worst], ours.data[worst]
        );
        crate::save(&ours, &Path::new(dir).join(format!("ours_{name}.png")))?;
        if let Some(grid)=run.get("grid").and_then(|v|v.as_array()).filter(|v|v.len()==6) {
            let ours=raw_sample_image.as_ref().unwrap();
            let g:Vec<_>=grid.iter().map(|v|v.as_f64().unwrap_or(0.0)).collect();
            let mut samples=Vec::new();
            for y in 0..g[5] as usize {for x in 0..g[4] as usize {
                let xx=(g[0]+(g[2]-g[0])*x as f64/(g[4]-1.0).max(1.0)).round().clamp(0.0,(ours.w-1) as f64) as usize;
                let yy=(g[1]+(g[3]-g[1])*y as f64/(g[5]-1.0).max(1.0)).round().clamp(0.0,(ours.h-1) as f64) as usize;
                samples.push(ours.at(xx,yy));
            }}
            std::fs::write(Path::new(dir).join(format!("samples_{name}.json")),serde_json::to_string(&samples).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        }
    }
    Ok(())
}

/// Points exactly as probe.jsx samples them (pixel centres on integers).
fn points(run: &Value) -> Vec<(f32, f32)> {
    let arr = |k: &str| {
        run.get(k).and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|v| v.as_f64())
                .map(|v| v as f32)
                .collect::<Vec<_>>()
        })
    };
    if let Some(g) = arr("grid") {
        let (x0, y0, x1, y1, n, m) = (g[0], g[1], g[2], g[3], g[4] as usize, g[5] as usize);
        let mut out = Vec::new();
        for j in 0..m {
            for i in 0..n {
                let fx = if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                let fy = if m > 1 {
                    j as f32 / (m - 1) as f32
                } else {
                    0.0
                };
                out.push(((x0 + (x1 - x0) * fx).round(), (y0 + (y1 - y0) * fy).round()));
            }
        }
        out
    } else if let Some(l) = arr("line") {
        let n = l[4] as usize;
        (0..n)
            .map(|i| {
                let f = if n > 1 {
                    i as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                (l[0] + (l[2] - l[0]) * f, l[1] + (l[3] - l[1]) * f)
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Reference side of a whole-frame probe (cfg "render"): samples the frames AE
/// rendered for the "@ref" runs (frame numbers from AE's result file).
pub fn probe_ref(cfg: &Path, ae_result: &Path, out: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let cfg: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let dir = cfg["render"].as_str().ok_or("cfg has no render folder")?;
    let res: Value =
        serde_json::from_str(&std::fs::read_to_string(ae_result).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut frames: HashMap<String, i64> = HashMap::new();
    for r in res["runs"].as_array().into_iter().flatten() {
        if let (Some(id), Some(f)) = (r["id"].as_str(), r["frame"].as_i64()) {
            frames.insert(id.to_string(), f);
        }
    }
    let mut runs = Vec::new();
    for run in cfg["runs"].as_array().into_iter().flatten() {
        let id = run["id"].as_str().unwrap_or("");
        let Some(&f) = frames.get(id) else { continue };
        let img = load16(&format!("{dir}/frame_{f:04}.png"))?;
        let samples: Vec<String> = points(run)
            .into_iter()
            .map(|(x, y)| {
                let (xi, yi) = (
                    x.round().clamp(0.0, (img.w - 1) as f32) as usize,
                    y.round().clamp(0.0, (img.h - 1) as f32) as usize,
                );
                let p = img.data[yi * img.w + xi];
                format!("{:.6} {:.6} {:.6} {:.6}", p[0], p[1], p[2], p[3])
            })
            .collect();
        runs.push(json!({ "id": id, "samples": samples.join(";") }));
    }
    let doc = json!({ "log": res["log"].clone(), "runs": runs });
    std::fs::write(out, serde_json::to_string(&doc).unwrap())
        .map_err(|e| format!("{}: {e}", out.display()))
}

pub fn probe(cfg: &Path, out: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let cfg: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut files: HashMap<String, Image> = HashMap::new();
    let mut runs = Vec::new();
    let mut log = Vec::new();
    // Whole-frame references are 16-bit PNGs: compare in their 0..1 range.
    let clamp = cfg.get("render").is_some();
    for run in cfg["runs"].as_array().into_iter().flatten() {
        let effect =
            &crate::timefx::canon(run.get("effect").and_then(|v| v.as_str()).unwrap_or(""));
        if !effect.starts_with("F_") || run.get("source").and_then(|v| v.as_str()) != Some("file") {
            continue;
        }
        let id = run["id"].as_str().unwrap_or("");
        let file = run["file"].as_str().ok_or("run without file")?;
        if !files.contains_key(file) {
            files.insert(file.to_string(), load16(file)?);
        }
        let mut src = files[file].clone();
        let value = run.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        if value != 1.0 {
            src.data.iter_mut().for_each(|p| {
                p[0] *= value;
                p[1] *= value;
                p[2] *= value;
            });
        }
        let empty = serde_json::Map::new();
        let params = Named(
            run.get("params")
                .and_then(|v| v.as_object())
                .unwrap_or(&empty),
        );
        let time = run.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let img = match render(effect, &src, None, &params, time) {
            Ok(img) => img,
            Err(e) => {
                log.push(format!("{id}: {e}"));
                continue;
            }
        };
        let samples: Vec<String> = points(run)
            .into_iter()
            .map(|(x, y)| {
                let (xi, yi) = (
                    x.round().clamp(0.0, (img.w - 1) as f32) as usize,
                    y.round().clamp(0.0, (img.h - 1) as f32) as usize,
                );
                let mut p = img.data[yi * img.w + xi];
                if clamp {
                    p.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));
                }
                format!("{:.6} {:.6} {:.6} {:.6}", p[0], p[1], p[2], p[3])
            })
            .collect();
        runs.push(json!({ "id": id, "samples": samples.join(";") }));
    }
    log.push("ok".into());
    let doc = json!({ "log": log, "runs": runs });
    std::fs::write(out, serde_json::to_string(&doc).unwrap())
        .map_err(|e| format!("{}: {e}", out.display()))
}
