//! BlurMoCurves and the shared weighted line integrator; spec/effects/batch3.md.
use crate::glow::AlphaMode;
use crate::shake::{pixel_to_norm, Frame, Sampler, Wrap};
use crate::{Image, Plane};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub rotate: f32,
    pub z: f32,
    pub shift: [f32; 2],
}
#[derive(Clone, Debug)]
pub struct CurvesParams {
    pub transform: bool,
    pub current: Transform,
    pub from: Transform,
    pub to: Transform,
    pub center: [f32; 2],
    pub bias: f32,
    pub brightness: f32,
    pub subpixel: bool,
    pub res: usize,
    pub wrap: [Wrap; 2],
    pub opacity: AlphaMode,
}
fn rot(v: [f32; 2], deg: f32) -> [f32; 2] {
    let (s, c) = (deg * 0.017453292).sin_cos();
    [v[0] * c + v[1] * s, -v[0] * s + v[1] * c]
}
fn end(xy: [f32; 2], center: [f32; 2], t: Transform) -> [f32; 2] {
    let r = rot([xy[0] - center[0], xy[1] - center[1]], -t.rotate);
    [
        r[0] * t.z + center[0] - t.shift[0],
        r[1] * t.z + center[1] - t.shift[1],
    ]
}
pub fn curves(src: &Image, mask: Option<&Plane>, fr: &Frame, p: &CurvesParams) -> Image {
    let mut pre = src.clone();
    match p.opacity {
        AlphaMode::Normal => pre.premultiply(),
        AlphaMode::AllOpaque => pre.data.iter_mut().for_each(|v| v[3] = 1.0),
        _ => {}
    }
    if p.res > 1 {
        let small = crate::resample::shrink(&pre, p.res);
        let mut cp = p.clone();
        cp.res = 1;
        cp.opacity = AlphaMode::AsPremult;
        cp.brightness = 1.0;
        let scale = p.res as f32;
        let result = curves(
            &small,
            None,
            &Frame {
                w: fr.w / scale,
                h: fr.h / scale,
                src_x: fr.src_x / p.res as i32,
                src_y: fr.src_y / p.res as i32,
            },
            &cp,
        );
        let mut out = crate::resample::grow(&result, p.res, src.w, src.h);
        for (i, o) in out.data.iter_mut().enumerate() {
            let m = mask.map(|m| m.data[i]).unwrap_or(1.0);
            for c in 0..4 {
                o[c] = pre.data[i][c] + (o[c] * p.brightness - pre.data[i][c]) * m;
            }
            o[3] = o[3].clamp(0.0, 1.0);
        }
        if p.opacity != AlphaMode::AsPremult {
            out.unpremultiply();
        }
        return out;
    }
    let smp = Sampler::new(&pre, p.wrap);
    let px = |v: [f32; 2]| {
        [
            (v[0] + 1.0) * 0.5 * fr.w - 0.5 - fr.src_x as f32,
            (fr.h - v[1] * fr.w) * 0.5 - 0.5 - fr.src_y as f32,
        ]
    };
    let mut out = Image::new(src.w, src.h);
    out.data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let n = pixel_to_norm(fr, x as f32 + fr.src_x as f32, y as f32 + fr.src_y as f32);
                let n = [n.0, n.1];
                let mut a = end(n, p.center, p.from);
                let mut b = end(n, p.center, p.to);
                if !p.transform {
                    let k = 0.5 / p.current.z.max(0.001);
                    let d = rot([(b[0] - a[0]) * k, (b[1] - a[1]) * k], p.current.rotate);
                    a = [n[0] - d[0], n[1] - d[1]];
                    b = [n[0] + d[0], n[1] + d[1]];
                }
                let v = smp.line(
                    px(a),
                    px(b),
                    [1.0 - p.bias, p.bias, p.bias],
                    1.0,
                    p.subpixel,
                );
                let m = mask.map(|m| m.data[y * src.w + x]).unwrap_or(1.0);
                for c in 0..4 {
                    o[c] = pre.data[y * src.w + x][c]
                        + (v[c] * p.brightness - pre.data[y * src.w + x][c]) * m;
                }
                o[3] = o[3].clamp(0.0, 1.0);
            }
        });
    if p.opacity != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> CurvesParams {
        let t = Transform {
            rotate: 0.0,
            z: 1.0,
            shift: [0.0; 2],
        };
        CurvesParams {
            transform: true,
            current: t,
            from: t,
            to: t,
            center: [0.0; 2],
            bias: 0.5,
            brightness: 1.0,
            subpixel: true,
            res: 1,
            wrap: [Wrap::Reflect; 2],
            opacity: AlphaMode::Normal,
        }
    }

    #[test]
    fn brightness_scales_alpha_before_unpremultiply_and_clamps_it() {
        let src = Image::filled(8, 4, [0.4, 0.5, 0.8, 0.5]);
        let fr = Frame {
            w: 8.0,
            h: 4.0,
            src_x: 0,
            src_y: 0,
        };
        let mut p = params();
        for (brightness, expected) in [(0.4, [0.4, 0.5, 0.8, 0.2]), (4.0, [0.8, 1.0, 1.6, 1.0])] {
            p.brightness = brightness;
            for res in [1, 2, 4] {
                p.res = res;
                let out = curves(&src, None, &fr, &p);
                for px in out.data {
                    for c in 0..4 {
                        assert!((px[c] - expected[c]).abs() < 1e-5, "{px:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn blur_only_removes_static_transform() {
        let src = Image {
            w: 16,
            h: 8,
            data: (0..128)
                .map(|i| [i as f32 / 128.0, 0.4, 0.8, 0.75])
                .collect(),
        };
        let fr = Frame {
            w: 16.0,
            h: 8.0,
            src_x: 0,
            src_y: 0,
        };
        let mut p = params();
        p.transform = false;
        p.current = Transform {
            rotate: 37.0,
            z: 1.7,
            shift: [0.3, -0.2],
        };
        p.from = p.current;
        p.to = p.current;
        let out = curves(&src, None, &fr, &p);
        for (a, b) in out.data.iter().zip(&src.data) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn line_integrator_preserves_ramp_mean_and_crosses_tile_seam() {
        let src = Image {
            w: 8,
            h: 4,
            data: (0..32).map(|i| [(i % 8) as f32, 0.5, 0.25, 1.0]).collect(),
        };
        let smp = Sampler::new(&src, [Wrap::Tile; 2]);
        for (from, to, expected) in [
            ([2.0, 1.25], [6.0, 1.25], 4.0),
            ([6.0, 1.25], [10.0, 1.25], 3.2),
        ] {
            for subpixel in [false, true] {
                let v = smp.line(from, to, [1.0; 3], 1.0, subpixel);
                assert!((v[0] - expected).abs() < 1e-6, "{v:?}");
                assert!((v[3] - 1.0).abs() < 1e-6);
            }
        }
    }
}
