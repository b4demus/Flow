//! Offline retimer for image sequences: same engine as the AE plugin, used for
//! tests and for comparing against reference renders.
//!
//! flowtime <in_dir> <out_dir> [--speed 50] [--in-fps 24] [--out-fps 24]
//!          [--quality best|high|medium|sloppy|none] [--interp nearest|blend|mwb]
//!          [--warp inverse|smart|forward|forward-nosmart] [--prep none|contrast|delin]
//!          [--sens 70] [--mb 0] [--frames N]

use flowtime_core::*;
use ::image::{ImageBuffer, Rgba};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Args {
    input: PathBuf,
    output: PathBuf,
    speed: f64,
    in_fps: f64,
    out_fps: f64,
    quality: Quality,
    interp: FrameInterp,
    warp: Warping,
    prep: ImagePrep,
    sens: f32,
    mb: f32,
    frames: Option<usize>,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let input = it.next().ok_or("missing <in_dir>")?.into();
    let output = it.next().ok_or("missing <out_dir>")?.into();
    let mut a = Args {
        input,
        output,
        speed: 50.0,
        in_fps: 24.0,
        out_fps: 24.0,
        quality: Quality::Best,
        interp: FrameInterp::Blend,
        warp: Warping::Inverse,
        prep: ImagePrep::None,
        sens: 70.0,
        mb: 0.0,
        frames: None,
    };
    while let Some(k) = it.next() {
        if k == "--prof" {
            flowtime_core::prof::enable();
            continue;
        }
        let v = it.next().ok_or(format!("missing value for {k}"))?;
        let num = |v: &str| v.parse::<f64>().map_err(|e| format!("{k}: {e}"));
        match k.as_str() {
            "--speed" => a.speed = num(&v)?,
            "--in-fps" => a.in_fps = num(&v)?,
            "--out-fps" => a.out_fps = num(&v)?,
            "--sens" => a.sens = num(&v)? as f32,
            "--mb" => a.mb = num(&v)? as f32,
            "--frames" => a.frames = Some(num(&v)? as usize),
            "--quality" => {
                a.quality = match v.as_str() {
                    "none" => Quality::NoVectors,
                    "sloppy" => Quality::Sloppy,
                    "medium" => Quality::Medium,
                    "high" => Quality::High,
                    _ => Quality::Best,
                }
            }
            "--interp" => {
                a.interp = match v.as_str() {
                    "nearest" => FrameInterp::Nearest,
                    "mwb" => FrameInterp::MotionWeightedBlend,
                    _ => FrameInterp::Blend,
                }
            }
            "--warp" => {
                a.warp = match v.as_str() {
                    "smart" => Warping::InverseSmartBlend,
                    "forward" => Warping::Forward,
                    "forward-nosmart" => Warping::ForwardNoSmartBlend,
                    _ => Warping::Inverse,
                }
            }
            "--prep" => {
                a.prep = match v.as_str() {
                    "contrast" => ImagePrep::ContrastEdge,
                    "delin" => ImagePrep::Delinearize,
                    _ => ImagePrep::None,
                }
            }
            _ => return Err(format!("unknown option {k}")),
        }
    }
    Ok(a)
}

fn load(p: &Path) -> Result<Image, String> {
    let img = ::image::open(p).map_err(|e| format!("{}: {e}", p.display()))?.to_rgba32f();
    let (w, h) = img.dimensions();
    let data = img
        .pixels()
        .map(|p| {
            let a = p[3];
            [p[0] * a, p[1] * a, p[2] * a, a]
        })
        .collect();
    Ok(Image { w: w as usize, h: h as usize, data })
}

fn save(img: &Image, p: &Path) -> Result<(), String> {
    let buf = ImageBuffer::<Rgba<u8>, _>::from_fn(img.w as u32, img.h as u32, |x, y| {
        let c = img.data[y as usize * img.w + x as usize];
        let a = c[3].clamp(0.0, 1.0);
        let un = |v: f32| if a > 1e-6 { (v / a).clamp(0.0, 1.0) } else { 0.0 };
        let q = |v: f32| (v * 255.0 + 0.5) as u8;
        Rgba([q(un(c[0])), q(un(c[1])), q(un(c[2])), q(a)])
    });
    buf.save(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn list_frames(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "tif" | "tiff")))
        .collect();
    files.sort();
    files
}

fn psnr(a: &Image, b: &Image) -> f64 {
    let mut se = 0.0f64;
    for (p, q) in a.data.iter().zip(&b.data) {
        for c in 0..3 {
            let d = (p[c] - q[c]) as f64;
            se += d * d;
        }
    }
    let mse = se / (a.data.len() * 3) as f64;
    if mse <= 1e-12 { 99.0 } else { 10.0 * (1.0 / mse).log10() }
}

/// Mean SSIM on luma over 8x8 windows (stride 4).
fn ssim(a: &Image, b: &Image) -> f64 {
    let (la, lb) = (a.luma(false), b.luma(false));
    let (c1, c2) = (0.01f64.powi(2), 0.03f64.powi(2));
    let (mut acc, mut n) = (0.0f64, 0usize);
    let mut y = 0;
    while y + 8 <= a.h {
        let mut x = 0;
        while x + 8 <= a.w {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
            for yy in y..y + 8 {
                for xx in x..x + 8 {
                    let (u, v) = (la.at(xx, yy) as f64, lb.at(xx, yy) as f64);
                    sa += u;
                    sb += v;
                    saa += u * u;
                    sbb += v * v;
                    sab += u * v;
                }
            }
            let k = 64.0;
            let (ma, mb) = (sa / k, sb / k);
            let (va, vb, cov) = (saa / k - ma * ma, sbb / k - mb * mb, sab / k - ma * mb);
            acc += ((2.0 * ma * mb + c1) * (2.0 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
            x += 4;
        }
        y += 4;
    }
    acc / n.max(1) as f64
}

/// `flowtime score <ref> <a> <b> <step> [crops_dir]`: per-frame PSNR/SSIM of two
/// retimed sequences against the real frames; frames with index % step == 0
/// were inputs, the rest were synthesised.
fn score(reference: &Path, runs: &[&Path; 2], step: usize, crops: Option<&Path>) {
    let rf = list_frames(reference);
    let rs: Vec<Vec<PathBuf>> = runs.iter().map(|d| list_frames(d)).collect();
    let n = rf.len().min(rs[0].len()).min(rs[1].len());
    let names: Vec<String> = runs.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    println!("frame  kind   {:>16} {:>16}", names[0], names[1]);
    println!("                 PSNR    SSIM     PSNR    SSIM");
    let mut sums = [[0.0f64; 2]; 2];
    let mut cnt = 0usize;
    let mut worst: Vec<(f64, usize)> = Vec::new();
    for i in 0..n {
        let r = load(&rf[i]).unwrap();
        let mut row = String::new();
        let synth = i % step != 0;
        let mut first = 0.0;
        for (k, files) in rs.iter().enumerate() {
            let img = load(&files[i]).unwrap();
            let (p, s) = (psnr(&r, &img), ssim(&r, &img));
            row += &format!(" {p:8.2} {s:7.4}");
            if synth {
                sums[k][0] += p;
                sums[k][1] += s;
            }
            if k == 0 {
                first = p;
            }
        }
        if synth {
            cnt += 1;
            worst.push((first, i));
        }
        println!("{i:5}  {}{row}", if synth { "synth" } else { "key  " });
    }
    let c = cnt.max(1) as f64;
    println!("\nmean over {cnt} synthesised frames:");
    for k in 0..2 {
        println!("  {:>10}: PSNR {:.2} dB  SSIM {:.4}", names[k], sums[k][0] / c, sums[k][1] / c);
    }
    if let Some(dir) = crops {
        std::fs::create_dir_all(dir).unwrap();
        worst.sort_by(|a, b| a.0.total_cmp(&b.0));
        for &(_, i) in worst.iter().take(3) {
            let imgs = [load(&rf[i]).unwrap(), load(&rs[0][i]).unwrap(), load(&rs[1][i]).unwrap()];
            let (w, h) = (imgs[0].w / 2, imgs[0].h / 2);
            let mut sheet = Image::new(w * 3, h);
            for (k, img) in imgs.iter().enumerate() {
                for y in 0..h {
                    for x in 0..w {
                        sheet.data[y * w * 3 + k * w + x] = img.data[(2 * y) * img.w + 2 * x];
                    }
                }
            }
            save(&sheet, &dir.join(format!("worst_{i:04}.png"))).unwrap();
        }
        println!("side-by-side (ref | {} | {}) of the 3 worst frames in {}", names[0], names[1], dir.display());
    }
}

/// `flowtime darken <in> <out> <gain> <noise>`: low-light version of a sequence
/// (gain on linear-ish values, per-frame noise in 8-bit steps, 8-bit output),
/// to reproduce how flow behaves on dark, noisy footage.
fn darken(input: &Path, output: &Path, gain: f32, noise: f32) {
    std::fs::create_dir_all(output).unwrap();
    for (n, p) in list_frames(input).iter().enumerate() {
        let mut img = load(p).unwrap();
        let mut s = 0x9e3779b97f4a7c15u64 ^ (n as u64).wrapping_mul(0x2545f4914f6cdd1d);
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64) as f32 - 0.5
        };
        for px in img.data.iter_mut() {
            for c in 0..3 {
                px[c] = (px[c].powf(2.2) * gain).powf(1.0 / 2.2) + rnd() * noise / 255.0;
            }
        }
        save(&img, &output.join(format!("frame_{n:04}.png"))).unwrap();
    }
}

/// `flowtime synth <dir> <frames> <w> <h>`: panning noise background with a
/// textured disc crossing it, for smoke tests without real footage.
fn synth(dir: &Path, n: usize, w: usize, h: usize) {
    std::fs::create_dir_all(dir).expect("create synth dir");
    let tex = |x: f32, y: f32| {
        let s = (x * 0.11).sin() * (y * 0.07).cos() + (x * 0.031 + y * 0.047).sin() * 0.7 + ((x * 0.53).sin() * (y * 0.41).sin()) * 0.3;
        0.5 + 0.25 * s
    };
    for f in 0..n {
        let ft = f as f32;
        let mut img = Image::new(w, h);
        let (cx, cy, r) = (w as f32 * 0.2 + ft * 9.0, h as f32 * 0.5 + (ft * 0.2).sin() * 20.0, h as f32 * 0.18);
        for y in 0..h {
            for x in 0..w {
                let (xf, yf) = (x as f32, y as f32);
                let bg = tex(xf + ft * 3.0, yf);
                let mut c = [bg * 0.6, bg * 0.8, bg, 1.0];
                let d = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt();
                if d < r {
                    let a = ft * 0.05;
                    let (u, v) = ((xf - cx) * a.cos() - (yf - cy) * a.sin(), (xf - cx) * a.sin() + (yf - cy) * a.cos());
                    let k = tex(u * 2.0 + 400.0, v * 2.0 + 400.0);
                    c = [k, k * 0.5, 0.15, 1.0];
                }
                img.data[y * w + x] = c;
            }
        }
        save(&img, &dir.join(format!("src_{f:05}.png"))).unwrap();
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(String::as_str) == Some("score") && raw.len() >= 6 {
        score(Path::new(&raw[2]), &[Path::new(&raw[3]), Path::new(&raw[4])], raw[5].parse().expect("step"), raw.get(6).map(Path::new));
        return;
    }
    if raw.get(1).map(String::as_str) == Some("sheet") && raw.len() >= 6 {
        // flowtime sheet <dir> <out.png> <first> <count> [cols]: tiles frames row-major.
        let files = list_frames(Path::new(&raw[2]));
        let (first, count): (usize, usize) = (raw[4].parse().unwrap(), raw[5].parse().unwrap());
        let cols: usize = raw.get(6).map_or(10, |v| v.parse().unwrap());
        let imgs: Vec<Image> = files.iter().skip(first).take(count).map(|p| load(p).unwrap()).collect();
        let (w, h) = (imgs[0].w, imgs[0].h);
        let rows = imgs.len().div_ceil(cols);
        let mut out = Image::new(w * cols, h * rows);
        for (k, img) in imgs.iter().enumerate() {
            let (ox, oy) = ((k % cols) * w, (k / cols) * h);
            for y in 0..h.min(img.h) {
                for x in 0..w.min(img.w) {
                    // Opaque so empty frames show as black, not transparent.
                    let c = img.data[y * img.w + x];
                    out.data[(oy + y) * out.w + ox + x] = [c[0], c[1], c[2], 1.0];
                }
            }
        }
        save(&out, Path::new(&raw[3])).unwrap();
        return;
    }
    if raw.get(1).map(String::as_str) == Some("darken") && raw.len() >= 6 {
        let f = |i: usize| raw[i].parse::<f32>().expect("number");
        darken(Path::new(&raw[2]), Path::new(&raw[3]), f(4), f(5));
        return;
    }
    if raw.get(1).map(String::as_str) == Some("synth") && raw.len() >= 6 {
        let n = |i: usize| raw[i].parse::<usize>().expect("number");
        synth(Path::new(&raw[2]), n(3), n(4), n(5));
        return;
    }
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\nusage: flowtime <in_dir> <out_dir> [--speed 50] ...");
            std::process::exit(2);
        }
    };
    let files = list_frames(&a.input);

    if files.len() < 2 {
        eprintln!("need at least 2 frames in {}", a.input.display());
        std::process::exit(2);
    }
    std::fs::create_dir_all(&a.output).expect("create output dir");

    let last = (files.len() - 1) as f64;
    let total = a.frames.unwrap_or_else(|| ((last / a.in_fps) * a.out_fps * 100.0 / a.speed.abs().max(1e-3)).floor() as usize + 1);
    let cache = FlowCache::new(8);
    let settings = Settings {
        quality: a.quality,
        prep: a.prep,
        interp: a.interp,
        warping: a.warp,
        mb_amount: a.mb,
        frames_per_output: (a.speed / 100.0 * a.in_fps / a.out_fps) as f32,
        ..Default::default()
    };
    let mut frames: std::collections::HashMap<i64, Image> = Default::default();
    let started = Instant::now();
    for n in 0..total {
        let speeds = vec![a.speed; n + 1];
        let src = source_from_speed(&speeds, 0.0, a.out_fps, a.in_fps).clamp(0.0, last);
        let p = pair(src);
        let b_idx = p.b.min(last as i64);
        for id in [p.a, b_idx] {
            if !frames.contains_key(&id) {
                frames.insert(id, load(&files[id as usize]).unwrap_or_else(|e| panic!("{e}")));
            }
        }
        frames.retain(|k, _| *k >= p.a - 1);
        let fr = Frames { a: &frames[&p.a], b: &frames[&b_idx], motion_a: None, motion_b: None, id_a: p.a, id_b: b_idx };
        let t0 = Instant::now();
        let out = retime(&fr, p.t, PairOverride::None, &[], a.sens, &[], &settings, Display::Output, Some((&cache, 1)));
        save(&out, &a.output.join(format!("frame_{n:05}.png"))).unwrap_or_else(|e| panic!("{e}"));
        println!("out {n:5}  src {src:9.3}  ({} -> {} t={:.3})  {:6.0} ms", p.a, b_idx, p.t, t0.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{total} frames in {:.1} s", started.elapsed().as_secs_f64());
    if flowtime_core::prof::enabled() {
        print!("{}", flowtime_core::prof::report());
    }
}
