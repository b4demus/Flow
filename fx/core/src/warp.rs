//! Warp effects: Warp Transform (2D/3D transform of the layer). spec/effects/batch2.md.

use crate::glow::AlphaMode;
use crate::image::Image;
use crate::image::Plane;
use crate::shake::{pixel_to_norm, Frame, Sampler, Wrap};
use rayon::prelude::*;

/// Shared WARP_MASK graph. None selects luma, Some(3) alpha. In a three-channel
/// graph coerce_to_alpha selects component zero (red), not the missing alpha.
pub fn warp_mask(
    src: Option<&Image>,
    w: usize,
    h: usize,
    channel: Option<usize>,
    invert: bool,
    blur: f32,
    opacity: AlphaMode,
) -> Option<Plane> {
    if src.is_none() && !invert {
        return None;
    }
    let mut p = Plane {
        w,
        h,
        data: (0..w * h)
            .map(|i| {
                let m = src
                    .map(|s| match channel {
                        Some(3) => s.data[i][3],
                        Some(c) => {
                            s.data[i][c]
                                * if opacity == AlphaMode::Normal {
                                    s.data[i][3]
                                } else {
                                    1.0
                                }
                        }
                        None => {
                            crate::flicker::mono(s.data[i])
                                * if opacity == AlphaMode::Normal {
                                    s.data[i][3]
                                } else {
                                    1.0
                                }
                        }
                    })
                    .unwrap_or(1.0);
                if invert {
                    1.0 - m
                } else {
                    m
                }
            })
            .collect(),
    };
    // Blur's graph enum is zero based: 2 means three box passes (Gauss).
    crate::boxes::box_passes(&mut p, blur.round(), blur.round(), crate::Edge::Reflect, 3);
    Some(p)
}

const DEG: f32 = 0.017_453_292;

/// The original's `tilt` point transform: perspective tilt, shift, rotate,
/// scale, shear, then (without wrap) a clamp to ±63.9.
#[derive(Clone, Debug)]
pub struct Tilt {
    tilt: Option<TiltPersp>,
    rot: Option<(f32, f32)>,
    shift: (f32, f32),
    scale: (f32, f32),
    shear: Option<(f32, f32)>,
    clamp: [bool; 2],
}

#[derive(Clone, Debug)]
struct TiltPersp {
    p3: f32,
    p4: f32,
    c: f32,
    s: f32,
    p12: f32,
    p13: f32,
    persp: f32,
    p15: f32,
    p16: f32,
}

impl Tilt {
    /// `tilt2` = (swivel, tilt) in degrees, `rotate` in degrees; the other
    /// arguments as the original passes them (already negated where it does).
    pub fn new(
        tilt2: (f32, f32),
        persp: f32,
        shift: (f32, f32),
        rotate: f32,
        scale: (f32, f32),
        shear: (f32, f32),
        wrap: [Wrap; 2],
    ) -> Self {
        // (in our y-up coordinates both components come out negated)
        let (ax, ay) = (-tilt2.0, tilt2.1);
        let tilt = (ax * ax + ay * ay != 0.0).then(|| {
            let inv_p = 1.0 / persp;
            let len = (ax * ax + ay * ay).sqrt();
            let th = len * DEG;
            let inv = 1.0 / len;
            let (c, s) = (th.cos(), th.sin());
            TiltPersp {
                p3: ay * inv,
                p4: ax * inv,
                c,
                s,
                p12: inv_p * s,
                p13: inv_p * c,
                persp,
                p15: c * persp,
                p16: s * persp,
            }
        });
        let rot = (rotate != 0.0).then(|| {
            let a = rotate * DEG;
            (a.cos(), a.sin())
        });
        let shear = (shear.0 != 0.0 || shear.1 != 0.0).then_some(shear);
        Self {
            tilt,
            rot,
            shift,
            scale,
            shear,
            clamp: [wrap[0] == Wrap::No, wrap[1] == Wrap::No],
        }
    }

    #[inline]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let (mut x, mut y) = (x, y);
        if let Some(t) = &self.tilt {
            let f = x * -t.p4 + y * t.p3;
            let den = f * -t.p16 - t.c;
            let u = if den != 0.0 {
                if (0.0 < den) == (0.0 < t.p13) {
                    (1.5 - t.p13) / den
                } else {
                    t.p13 / -den
                }
            } else {
                t.p13 / 0.0001
            };
            let p11 = (x * t.p3 + y * t.p4) * t.persp * u;
            let p12 = (f * t.p15 - t.s) * u + t.p12;
            x = p11 * t.p3 + p12 * -t.p4;
            y = p11 * t.p4 + p12 * t.p3;
        }
        x += self.shift.0;
        y += self.shift.1;
        if let Some((c, s)) = self.rot {
            let (a, b) = (x, y);
            x = a * c + b * s;
            y = a * -s + b * c;
        }
        x *= self.scale.0;
        y *= self.scale.1;
        if let Some((sx, sy)) = self.shear {
            let (a, b) = (x, y);
            x = a + b * sx;
            y = b + a * sy;
        }
        if self.clamp[0] {
            x = (x as f64).clamp(-63.9, 63.9) as f32;
        }
        if self.clamp[1] {
            y = (y as f64).clamp(-63.9, 63.9) as f32;
        }
        (x, y)
    }
}

#[derive(Clone, Debug)]
pub struct WarpTransformParams {
    pub scale: (f32, f32),
    /// Normalised shift.
    pub shift: (f32, f32),
    pub z_dist: f32,
    /// Degrees.
    pub rotate: f32,
    pub swivel: f32,
    pub tilt: f32,
    pub perspective: f32,
    pub shear: (f32, f32),
    pub wrap: [Wrap; 2],
    /// Mip-mapped sampling where the picture shrinks.
    pub filter: bool,
    pub alpha_mode: AlphaMode,
}

impl WarpTransformParams {
    pub fn tilt(&self) -> Tilt {
        let safe = |s: f32| {
            if -s > 0.0 {
                s.min(-0.001)
            } else {
                s.max(0.001)
            }
        };
        Tilt::new(
            (self.swivel, self.tilt),
            self.perspective,
            (-self.shift.0, -self.shift.1),
            -self.rotate,
            (
                self.z_dist / safe(self.scale.0),
                self.z_dist / safe(self.scale.1),
            ),
            (-self.shear.0, -self.shear.1),
            self.wrap,
        )
    }
}

/// `src` straight colour placed at (fr.src_x, fr.src_y) in the frame.
pub fn warp_transform(src: &Image, fr: &Frame, p: &WarpTransformParams) -> Image {
    let mut pre = src.clone();
    match p.alpha_mode {
        AlphaMode::Normal => pre.premultiply(),
        AlphaMode::AllOpaque => pre.data.par_iter_mut().for_each(|px| px[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    let t = p.tilt();
    let mut out = Image::new(src.w, src.h);
    if p.filter {
        let mip = crate::mipmap::MipSampler::new(&pre, p.wrap);
        let k = 2.0 / fr.w;
        out.data
            .par_chunks_mut(src.w.max(1))
            .enumerate()
            .for_each(|(j, row)| {
                for (i, o) in row.iter_mut().enumerate() {
                    let (x, y) = pixel_to_norm(
                        fr,
                        (i as i32 + fr.src_x) as f32,
                        (j as i32 + fr.src_y) as f32,
                    );
                    let (u, v) = t.apply(x, y);
                    let (ux, vx) = t.apply(x + k, y);
                    let (uy, vy) = t.apply(x, y - k);
                    let lod = crate::mipmap::lod(fr, (ux - u, vx - v), (uy - u, vy - v));
                    *o = mip.at(fr, u, v, lod);
                }
            });
    } else {
        let smp = Sampler::new(&pre, p.wrap);
        out.data
            .par_chunks_mut(src.w.max(1))
            .enumerate()
            .for_each(|(j, row)| {
                for (i, o) in row.iter_mut().enumerate() {
                    let (x, y) = pixel_to_norm(
                        fr,
                        (i as i32 + fr.src_x) as f32,
                        (j as i32 + fr.src_y) as f32,
                    );
                    let (u, v) = t.apply(x, y);
                    *o = smp.at(fr, u, v);
                }
            });
    }
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

// ───────────────────────────── Distort RGB ─────────────────────────────

#[derive(Clone, Debug)]
pub struct DistortRgbParams {
    pub amount: f32,
    pub amount_rel: (f32, f32),
    /// Normalised (AE pixels / 240).
    pub blur_lens: f32,
    /// Degrees.
    pub rotate_dir: f32,
    /// Red, green, blue, alpha.
    pub warp: [f32; 4],
    pub wrap: [Wrap; 2],
    pub filter: bool,
    pub alpha_mode: AlphaMode,
}

/// Each channel is moved along the gradient of the blurred lens brightness.
/// `lens` straight colour, same size and place as `src`.
pub fn distort_rgb(
    src: &Image,
    lens: &Image,
    mask: Option<&Plane>,
    fr: &Frame,
    p: &DistortRgbParams,
) -> Image {
    use crate::blur::Edge;
    use crate::image::Plane;
    let (w, h) = (src.w, src.h);
    let mut m = Plane {
        w,
        h,
        data: lens
            .data
            .iter()
            .map(|px| {
                let k = if p.alpha_mode == AlphaMode::Normal {
                    px[3]
                } else {
                    1.0
                };
                crate::flicker::mono(*px) * k
            })
            .collect(),
    };
    // AE's normalize_scale=4 parameter is pixels; the DLL receives 8*pixels/frame_width.
    let r = p.blur_lens * 240.0;
    crate::boxes::box3(&mut m, r, r, Edge::Reflect, true);
    let scale = p.amount * (p.blur_lens * 1920.0 / fr.w + 0.1);
    let gain = scale * fr.w * 0.05;
    let a = (p.rotate_dir / 360.0) * std::f32::consts::TAU;
    let (sn, cs) = a.sin_cos();
    let mut pre = src.clone();
    match p.alpha_mode {
        AlphaMode::Normal => pre.premultiply(),
        AlphaMode::AllOpaque => pre.data.par_iter_mut().for_each(|px| px[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    // displacement field (normalised units, y up)
    let mut disp = vec![(0f32, 0f32); w * h];
    disp.par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            let at = |x: usize, y: usize| m.data[y * w + x];
            for (i, d) in row.iter_mut().enumerate() {
                let gx = if w < 2 {
                    0.0
                } else if i == 0 {
                    (at(1, j) - at(0, j)) * 2.0
                } else if i == w - 1 {
                    (at(w - 1, j) - at(w - 2, j)) * 2.0
                } else {
                    at(i + 1, j) - at(i - 1, j)
                };
                // y up: the row above minus the row below
                let gy = if h < 2 {
                    0.0
                } else if j == 0 {
                    (at(i, 0) - at(i, 1)) * 2.0
                } else if j == h - 1 {
                    (at(i, h - 2) - at(i, h - 1)) * 2.0
                } else {
                    at(i, j - 1) - at(i, j + 1)
                };
                let (gx, gy) = (gx * gain * p.amount_rel.0, gy * gain * p.amount_rel.1);
                // the original turns the other way round
                let m = mask.map(|m| m.data[j * w + i]).unwrap_or(1.0);
                *d = ((gx * cs + gy * sn) * m, (-gx * sn + gy * cs) * m);
            }
        });
    let mip = p
        .filter
        .then(|| crate::mipmap::RipSampler::new(&pre, p.wrap));
    let smp = Sampler::new(&pre, p.wrap);
    let mut out = Image::new(w, h);
    out.data
        .par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            for (i, o) in row.iter_mut().enumerate() {
                let (dx, dy) = disp[j * w + i];
                let (x, y) = pixel_to_norm(
                    fr,
                    (i as i32 + fr.src_x) as f32,
                    (j as i32 + fr.src_y) as f32,
                );
                let (nx, ny) = disp[j * w + (i + 1).min(w - 1)];
                let (mx, my) = disp[j.saturating_sub(1) * w + i];
                let left = disp[j * w + i.saturating_sub(1)];
                let down = disp[(j + 1).min(h - 1) * w + i];
                for c in 0..3 {
                    let wc = p.warp[c];
                    let (u, v) = (x + dx * wc, y + dy * wc);
                    let s = match &mip {
                        Some(mp) => {
                            let q = fr.w * 0.25 * wc;
                            let center = [dx, dy];
                            let l = [left.0, left.1];
                            let r = [nx, ny];
                            let up = [mx, my];
                            let dn = [down.0, down.1];
                            let lod = std::array::from_fn(|axis| {
                                // warp_rel measures the displacement itself;
                                // unlike absolute warp it adds no identity derivative.
                                let ax = 0.0;
                                let ay = 0.0;
                                let a = if i == 0 {
                                    ((r[axis] - center[axis]) * q * 2.0 + ax).abs()
                                } else if i == w - 1 {
                                    ((center[axis] - l[axis]) * q * 2.0 + ax).abs()
                                } else {
                                    ((center[axis] - l[axis]) * q + ax).abs()
                                        + ((r[axis] - center[axis]) * q + ax).abs()
                                };
                                let edge = if j == 0 || j == h - 1 { 2.0 } else { 1.0 };
                                let b = ((center[axis] - up[axis]) * q * edge + ay).abs()
                                    + ((dn[axis] - center[axis]) * q * edge + ay).abs();
                                crate::mipmap::fast_log2(a.max(b) + 0.5 * a.min(b))
                            });
                            mp.at(fr, u, v, lod)
                        }
                        None => smp.at(fr, u, v),
                    };
                    o[c] = s[c];
                }
                // Distort_RGB lacks rgba_ok in its graph: the AE wrapper keeps
                // the source matte instead of geometrically moving alpha.
                o[3] = if p.alpha_mode == AlphaMode::AllOpaque {
                    1.0
                } else {
                    src.data[j * w + i][3]
                };
            }
        });
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

// ───────────────────────────── Warp Chroma ─────────────────────────────

#[derive(Clone, Debug)]
pub struct WarpChromaParams {
    pub steps: i32,
    /// Normalised centre.
    pub center: (f32, f32),
    pub from_z: f32,
    /// Degrees.
    pub from_rotate: f32,
    /// Normalised.
    pub from_shift: (f32, f32),
    pub to_z: f32,
    pub to_rotate: f32,
    pub to_shift: (f32, f32),
    pub warp_amount: f32,
    pub brightness: f32,
    pub colors: [[f32; 3]; 3],
    pub white_balance: bool,
    pub wrap: [Wrap; 2],
    pub filter: bool,
    pub alpha_mode: AlphaMode,
}

/// Weights of the three colours at s (0..1 along the path): the first two
/// share the first half, the last two the second, as unit vectors.
fn chroma_basis(s: f32) -> [f32; 3] {
    if s <= 0.5 {
        let d = 1.0 - s;
        [((1.0 - 2.0 * s) / d).max(0.0).sqrt(), (s / d).sqrt(), 0.0]
    } else {
        let t = 1.0 - s;
        let d = 1.0 - t;
        [0.0, (t / d).sqrt(), ((1.0 - 2.0 * t) / d).max(0.0).sqrt()]
    }
}

/// Per-step RGB weights: each colour's share normalised over the steps; with
/// white balance every channel then sums to 1. Times `brightness`.
pub fn chroma_weights(
    n: usize,
    colors: &[[f32; 3]; 3],
    brightness: f32,
    white_balance: bool,
) -> Vec<[f32; 3]> {
    let basis: Vec<[f32; 3]> = (0..n)
        .map(|k| {
            chroma_basis(if n > 1 {
                k as f32 / (n - 1) as f32
            } else {
                0.0
            })
        })
        .collect();
    let mut tot = [0f32; 3];
    for b in &basis {
        for j in 0..3 {
            tot[j] += b[j];
        }
    }
    let mut w: Vec<[f32; 3]> = basis
        .iter()
        .map(|b| {
            let nb = [0, 1, 2].map(|j| if tot[j] > 0.0 { b[j] / tot[j] } else { 0.0 });
            [0, 1, 2].map(|c| nb[0] * colors[0][c] + nb[1] * colors[1][c] + nb[2] * colors[2][c])
        })
        .collect();
    for c in 0..3 {
        let sum: f32 = w.iter().map(|v| v[c]).sum();
        let k = if white_balance && sum > 0.0 {
            brightness / sum
        } else {
            brightness
        };
        w.iter_mut().for_each(|v| v[c] *= k);
    }
    w
}

fn rot_cw(x: f32, y: f32, deg: f32) -> (f32, f32) {
    let a = deg * DEG;
    let (s, c) = a.sin_cos();
    (x * c + y * s, -x * s + y * c)
}

/// Colour-tinted copies of the picture spread from one transform to another.
pub fn warp_chroma(src: &Image, mask: Option<&Plane>, fr: &Frame, p: &WarpChromaParams) -> Image {
    let mut pre = src.clone();
    match p.alpha_mode {
        AlphaMode::Normal => pre.premultiply(),
        AlphaMode::AllOpaque => pre.data.par_iter_mut().for_each(|px| px[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    let n = p.steps.clamp(3, 100) as usize;
    let wts = chroma_weights(n, &p.colors, p.brightness, p.white_balance);
    let smp = Sampler::new(&pre, p.wrap);
    let mip = p
        .filter
        .then(|| crate::mipmap::RipSampler::new(&pre, p.wrap));
    let end = |x: f32, y: f32, z: f32, rot: f32, shift: (f32, f32), amount: f32| -> (f32, f32) {
        let (rx, ry) = rot_cw(x - p.center.0, y - p.center.1, -rot / 360.0 * 360.0);
        let (tx, ty) = (
            rx * z + (p.center.0 - shift.0),
            ry * z + (p.center.1 - shift.1),
        );
        (x + (tx - x) * amount, y + (ty - y) * amount)
    };
    let point = |i: usize, j: usize, from: bool| {
        let (x, y) = pixel_to_norm(
            fr,
            (i as i32 + fr.src_x) as f32,
            (j as i32 + fr.src_y) as f32,
        );
        let amount = p.warp_amount * mask.map(|m| m.data[j * src.w + i]).unwrap_or(1.0);
        if from {
            end(x, y, p.from_z, p.from_rotate, p.from_shift, amount)
        } else {
            end(x, y, p.to_z, p.to_rotate, p.to_shift, amount)
        }
    };
    let mut out = Image::new(src.w, src.h);
    out.data
        .par_chunks_mut(src.w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            for (i, o) in row.iter_mut().enumerate() {
                let (fx, fy) = point(i, j, true);
                let (tx, ty) = point(i, j, false);
                let level = |from: bool| {
                    if mask.is_none() {
                        return [0.5
                            * crate::mipmap::fast_log2(if from { p.from_z } else { p.to_z });
                            2];
                    }
                    let l = point(i.saturating_sub(1), j, from);
                    let r = point((i + 1).min(src.w - 1), j, from);
                    let up = point(i, j.saturating_sub(1), from);
                    let dn = point(i, (j + 1).min(src.h - 1), from);
                    let (qx, qy) = (
                        if i == 0 || i == src.w - 1 {
                            fr.w * 0.5
                        } else {
                            fr.w * 0.25
                        },
                        if j == 0 || j == src.h - 1 {
                            fr.w * 0.5
                        } else {
                            fr.w * 0.25
                        },
                    );
                    let c = if from { (fx, fy) } else { (tx, ty) };
                    let a = [
                        ((c.0 - l.0).abs() + (r.0 - c.0).abs()) * qx,
                        ((c.1 - l.1).abs() + (r.1 - c.1).abs()) * qx,
                    ];
                    let b = [
                        ((c.0 - up.0).abs() + (dn.0 - c.0).abs()) * qy,
                        ((c.1 - up.1).abs() + (dn.1 - c.1).abs()) * qy,
                    ];
                    std::array::from_fn(|c| {
                        crate::mipmap::fast_log2(a[c].max(b[c]) + 0.5 * a[c].min(b[c]))
                    })
                };
                let lod0 = if p.filter { level(true) } else { [0.0; 2] };
                let lod1 = if p.filter { level(false) } else { [0.0; 2] };
                let k = 1.0 / (n - 1) as f32;
                let (dx, dy) = ((tx - fx) * k, (ty - fy) * k);
                let mut acc = [0f32; 4];
                let (mut u, mut v) = (fx, fy);
                for (step, w) in wts.iter().enumerate() {
                    let t = step as f32 / (n - 1) as f32;
                    let s = if let Some(mip) = &mip {
                        mip.at(
                            fr,
                            u,
                            v,
                            [
                                lod0[0] + (lod1[0] - lod0[0]) * t,
                                lod0[1] + (lod1[1] - lod0[1]) * t,
                            ],
                        )
                    } else {
                        smp.at(fr, u, v)
                    };
                    acc[0] += s[0] * w[0];
                    acc[1] += s[1] * w[1];
                    acc[2] += s[2] * w[2];
                    acc[3] += s[3] * (1.0 / n as f32);
                    u += dx;
                    v += dy;
                }
                *o = acc;
            }
        });
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

#[cfg(test)]
mod rgb_tests {
    use super::*;

    #[test]
    fn mask_channel_depends_on_graph_component_count() {
        let src = Image::filled(8, 8, [0.4, 0.6, 0.9, 0.25]);
        let alpha = warp_mask(Some(&src), 8, 8, Some(3), false, 5.0, AlphaMode::Normal).unwrap();
        let rgb_alpha =
            warp_mask(Some(&src), 8, 8, Some(0), false, 5.0, AlphaMode::Normal).unwrap();
        let inv = warp_mask(Some(&src), 8, 8, Some(0), true, 5.0, AlphaMode::Normal).unwrap();
        let opaque =
            warp_mask(Some(&src), 8, 8, Some(0), false, 0.0, AlphaMode::AllOpaque).unwrap();
        assert!(alpha.data.iter().all(|v| (*v - 0.25).abs() < 1e-6));
        assert!(rgb_alpha.data.iter().all(|v| (*v - 0.1).abs() < 1e-6));
        assert!(inv.data.iter().all(|v| (*v - 0.9).abs() < 1e-6));
        assert!(opaque.data.iter().all(|v| (*v - 0.4).abs() < 1e-6));
        assert!(warp_mask(None, 8, 8, None, false, 5.0, AlphaMode::Normal).is_none());
    }

    #[test]
    fn distort_zero_keeps_matte_and_straight_color() {
        let src = Image {
            w: 8,
            h: 4,
            data: (0..32)
                .map(|i| [i as f32 / 32.0, 0.25, 0.5, 0.1 + i as f32 / 40.0])
                .collect(),
        };
        let p = DistortRgbParams {
            amount: 0.0,
            amount_rel: (1.0, 1.0),
            blur_lens: 0.4,
            rotate_dir: 0.0,
            warp: [0.5, 0.75, 1.0, 0.75],
            wrap: [Wrap::Reflect; 2],
            filter: true,
            alpha_mode: AlphaMode::Normal,
        };
        let out = distort_rgb(
            &src,
            &src,
            None,
            &Frame {
                w: 8.0,
                h: 4.0,
                src_x: 0,
                src_y: 0,
            },
            &p,
        );
        for (a, b) in out.data.iter().zip(&src.data) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 1e-6, "{a:?} {b:?}");
            }
        }
    }
}
