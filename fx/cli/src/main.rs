//! Offline tools around the effects: run an effect on a still, compare renders.
//!
//! facet compare <a> <b> [diff.png]     PSNR / SSIM / max error, optional x8 diff image
//! facet gen-noise <out.png> <w> <h> [seed]
//! facet probe <cfg.json> <out.json>     offline side of tools/ae/probe.jsx (see probe.rs)
//! facet frames <cfg.json> <ae.json>     whole-frame ours vs AE for "render" probes

mod fx2;
mod fx3;
mod fx4;
mod fx5;
mod probe;
mod timefx;

use ::image::{ImageBuffer, Rgba};
use facet_core::image::luma;
use facet_core::Image;
use std::path::Path;

fn load(p: &Path) -> Result<Image, String> {
    let img = ::image::open(p)
        .map_err(|e| format!("{}: {e}", p.display()))?
        .to_rgba32f();
    let (w, h) = img.dimensions();
    let data = img.pixels().map(|p| [p[0], p[1], p[2], p[3]]).collect();
    Ok(Image {
        w: w as usize,
        h: h as usize,
        data,
    })
}

fn save(img: &Image, p: &Path) -> Result<(), String> {
    let buf = ImageBuffer::<Rgba<u16>, _>::from_fn(img.w as u32, img.h as u32, |x, y| {
        let c = img.data[y as usize * img.w + x as usize];
        let q = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        Rgba([q(c[0]), q(c[1]), q(c[2]), q(c[3])])
    });
    buf.save(p).map_err(|e| format!("{}: {e}", p.display()))
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
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (1.0 / mse).log10()
    }
}

/// Mean SSIM on luma over 8x8 windows (stride 4).
fn ssim(a: &Image, b: &Image) -> f64 {
    let la: Vec<f32> = a.data.iter().map(|p| luma(*p)).collect();
    let lb: Vec<f32> = b.data.iter().map(|p| luma(*p)).collect();
    let (c1, c2) = (0.01f64.powi(2), 0.03f64.powi(2));
    let (mut acc, mut n) = (0.0f64, 0usize);
    let mut y = 0;
    while y + 8 <= a.h {
        let mut x = 0;
        while x + 8 <= a.w {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
            for yy in y..y + 8 {
                for xx in x..x + 8 {
                    let (p, q) = (la[yy * a.w + xx] as f64, lb[yy * a.w + xx] as f64);
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            let k = 64.0;
            let (ma, mb) = (sa / k, sb / k);
            let (va, vb, cov) = (saa / k - ma * ma, sbb / k - mb * mb, sab / k - ma * mb);
            acc += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
            x += 4;
        }
        y += 4;
    }
    if n == 0 {
        1.0
    } else {
        acc / n as f64
    }
}

fn compare(a: &Path, b: &Path, diff: Option<&Path>, premult: bool) -> Result<(), String> {
    let (mut ia, mut ib) = (load(a)?, load(b)?);
    if ia.w != ib.w || ia.h != ib.h {
        return Err(format!(
            "size mismatch {}x{} vs {}x{}",
            ia.w, ia.h, ib.w, ib.h
        ));
    }
    if premult {
        ia.premultiply();
        ib.premultiply();
    }
    let max = ia
        .data
        .iter()
        .zip(&ib.data)
        .flat_map(|(p, q)| (0..4).map(move |c| (p[c] - q[c]).abs()))
        .fold(0.0f32, f32::max);
    println!(
        "PSNR {:.2} dB  SSIM {:.4}  max {:.4}",
        psnr(&ia, &ib),
        ssim(&ia, &ib),
        max
    );
    if let Some(d) = diff {
        let data = ia
            .data
            .iter()
            .zip(&ib.data)
            .map(|(p, q)| {
                [
                    (p[0] - q[0]).abs() * 8.0,
                    (p[1] - q[1]).abs() * 8.0,
                    (p[2] - q[2]).abs() * 8.0,
                    1.0,
                ]
            })
            .collect();
        save(
            &Image {
                w: ia.w,
                h: ia.h,
                data,
            },
            d,
        )?;
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("compare") if args.len() >= 3 => compare(Path::new(&args[1]), Path::new(&args[2]), args.get(3).filter(|s| s.as_str() != "--premult").map(Path::new), args.iter().any(|s| s == "--premult")),
        Some("gen-noise") if args.len() >= 4 => {
            let num = |i: usize| args.get(i).and_then(|s| s.parse::<usize>().ok());
            match (num(2), num(3)) {
                (Some(w), Some(h)) => probe::gen_noise(Path::new(&args[1]), w, h, num(4).unwrap_or(1) as u32),
                _ => Err("gen-noise: bad size".into()),
            }
        }
        Some("probe") if args.len() >= 3 => probe::probe(Path::new(&args[1]), Path::new(&args[2])),
        // facet row <img> <y> <x0> <x1> [chan]: pixel values along a row
        Some("row") if args.len() >= 5 => load(Path::new(&args[1])).map(|img| {
            let n = |i: usize| args[i].parse::<usize>().unwrap_or(0);
            let (y, x0, x1) = (n(2), n(3), n(4).min(img.w - 1));
            let c = args.get(5).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let vals: Vec<String> = (x0..=x1).map(|x| format!("{:.5}", img.data[y * img.w + x][c])).collect();
            println!("{}", vals.join(" "));
        }),
        // facet col <img> <x> <y0> <y1> [chan]
        Some("col") if args.len() >= 5 => load(Path::new(&args[1])).map(|img| {
            let n = |i: usize| args[i].parse::<usize>().unwrap_or(0);
            let (x, y0, y1) = (n(2), n(3), n(4).min(img.h - 1));
            let c = args.get(5).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let vals: Vec<String> = (y0..=y1).map(|y| format!("{:.5}", img.data[y * img.w + x][c])).collect();
            println!("{}", vals.join(" "));
        }),
        Some("probe-ref") if args.len() >= 4 => probe::probe_ref(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3])),
        // facet frames <cfg.json> <ae_result.json>: whole frames, ours vs AE (cfg with "render")
        Some("frames") if args.len() >= 3 => probe::frames(Path::new(&args[1]), Path::new(&args[2])),
        // facet fitshift <ref> <ours> [chan] [margin]: sub-pixel shift (ref ≈ ours moved by dx, dy)
        // from a least-squares fit of the difference to ours' gradient
        Some("fitshift") if args.len() >= 3 => (|| -> Result<(), String> {
            let (a, b) = (load(Path::new(&args[1]))?, load(Path::new(&args[2]))?);
            let c = args.get(3).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let m = args.get(4).and_then(|s| s.parse::<usize>().ok()).unwrap_or(4);
            let (mut sxx, mut sxy, mut syy, mut sxd, mut syd) = (0f64, 0f64, 0f64, 0f64, 0f64);
            for y in m..b.h - m {
                for x in m..b.w - m {
                    let at = |xx: usize, yy: usize| b.data[yy * b.w + xx][c] as f64;
                    let (gx, gy) = ((at(x + 1, y) - at(x - 1, y)) * 0.5, (at(x, y + 1) - at(x, y - 1)) * 0.5);
                    let d = a.data[y * a.w + x][c] as f64 - at(x, y);
                    sxx += gx * gx;
                    sxy += gx * gy;
                    syy += gy * gy;
                    sxd += gx * d;
                    syd += gy * d;
                }
            }
            let det = sxx * syy - sxy * sxy;
            // ref(x) ≈ ours(x) + g·s  →  ref is ours sampled s further along +x/+y
            let (dx, dy) = ((syy * sxd - sxy * syd) / det, (sxx * syd - sxy * sxd) / det);
            println!("dx {dx:+.5} dy {dy:+.5} (pixels, ref samples ours at x+dx, y+dy)");
            Ok(())
        })(),
        // facet view <in> <out.png> [gain] [alpha]: 8-bit preview (alpha: show the alpha channel)
        Some("view") if args.len() >= 3 => load(Path::new(&args[1])).and_then(|mut img| {
            let gain = args.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
            let alpha = args.get(4).map(|s| s == "alpha").unwrap_or(false);
            for p in img.data.iter_mut() {
                *p = if alpha { [p[3] * gain, p[3] * gain, p[3] * gain, 1.0] } else { [p[0] * gain, p[1] * gain, p[2] * gain, 1.0] };
            }
            save(&img, Path::new(&args[2]))
        }),
        _ => Err("usage: facet compare <a> <b> [diff.png] | gen-noise <out.png> <w> <h> [seed] | probe <cfg.json> <out.json>".into()),
    };
    if let Err(e) = r {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
