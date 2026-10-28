//! Dense optical flow: coarse-to-fine patch inverse search with densification
//! (DIS, Kroeger et al. 2016), written from the paper.
//!
//! Extras needed by the retimer:
//! * an optional per-pixel weight mask, so a layer is tracked only from its own
//!   pixels and the rest of the field is filled in afterwards;
//! * guide points that pin the flow near user track points;
//! * a sensitivity limit on vector length.

use crate::filters::{fill_flow, median3, pyramid};
use crate::image::{Flow, Gray};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    NoVectors,
    Sloppy,
    Medium,
    High,
    Best,
}

#[derive(Clone, Copy, Debug)]
pub struct FlowParams {
    pub quality: Quality,
    /// 0..100: how far pixels may move (100 = unlimited).
    pub sensitivity: f32,
    /// Extra halving of the working resolution (draft render).
    pub draft: bool,
}

impl Default for FlowParams {
    fn default() -> Self {
        Self { quality: Quality::Best, sensitivity: 70.0, draft: false }
    }
}

/// A correspondence `from` (in frame 0) -> `to` (in frame 1), full-res pixels.
#[derive(Clone, Copy, Debug)]
pub struct Guide {
    pub from: [f32; 2],
    pub to: [f32; 2],
}

struct LevelCfg {
    finest: usize,
    patch: usize,
    stride: usize,
    iters: usize,
}

fn cfg(p: &FlowParams) -> LevelCfg {
    let mut c = match p.quality {
        Quality::NoVectors | Quality::Best => LevelCfg { finest: 0, patch: 8, stride: 4, iters: 16 },
        Quality::High => LevelCfg { finest: 1, patch: 8, stride: 4, iters: 12 },
        Quality::Medium => LevelCfg { finest: 2, patch: 8, stride: 4, iters: 12 },
        Quality::Sloppy => LevelCfg { finest: 2, patch: 8, stride: 6, iters: 6 },
    };
    if p.draft {
        c.finest += 1;
    }
    c
}

/// Flow from `i0` to `i1` (for each pixel of `i0`, where it is in `i1`).
/// `mask0` (same size, 0..1) restricts which pixels of `i0` drive the estimate.
pub fn compute_flow(i0: &Gray, i1: &Gray, mask0: Option<&Gray>, guides: &[Guide], params: &FlowParams) -> Flow {
    let (w, h) = (i0.w, i0.h);
    if params.quality == Quality::NoVectors || w < 4 || h < 4 {
        return Flow::zero(w, h);
    }
    let _t = crate::prof::span("flow");
    let c = cfg(params);
    let pyr = crate::prof::span("flow.pyramid");
    let p0 = pyramid(i0, 16, 12);
    let p1 = pyramid(i1, 16, 12);
    let pm = mask0.map(|m| pyramid(m, 16, 12));
    drop(pyr);
    let levels = p0.len();
    let finest = c.finest.min(levels - 1);

    let mut flow: Option<Flow> = None;
    for lv in (finest..levels).rev() {
        let (g0, g1) = (&p0[lv], &p1[lv]);
        let scale = (1u32 << lv) as f32;
        let init = match flow.take() {
            None => Flow::zero(g0.w, g0.h),
            Some(f) => upsample(&f, g0.w, g0.h),
        };
        let lguides: Vec<Guide> = guides
            .iter()
            .map(|g| Guide {
                from: [g.from[0] / scale, g.from[1] / scale],
                to: [g.to[0] / scale, g.to[1] / scale],
            })
            .collect();
        let init = apply_guides(&init, &lguides);
        let m = pm.as_ref().map(|p| &p[lv]);
        let mut f = level_search(g0, g1, m, &init, &c);
        f = { let _t = crate::prof::span("flow.median"); median3(&f) };
        f = apply_guides(&f, &lguides);
        flow = Some(f);
    }
    let mut f = flow.unwrap();
    if f.w != w || f.h != h {
        f = upsample(&f, w, h);
    }
    limit_length(&mut f, params.sensitivity);
    f
}

/// Resample a flow field to a new size, scaling the vectors accordingly.
pub fn upsample(f: &Flow, w: usize, h: usize) -> Flow {
    let (sx, sy) = (f.w as f32 / w as f32, f.h as f32 / h as f32);
    let data = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let v = f.sample((x + 0.5) * sx - 0.5, (y + 0.5) * sy - 0.5);
            [v[0] / sx, v[1] / sy]
        })
        .collect();
    Flow { w, h, data }
}

fn limit_length(f: &mut Flow, sensitivity: f32) {
    if sensitivity >= 99.5 {
        return;
    }
    // 0 -> no motion at all, 70 -> ~30% of the frame, 100 -> unlimited.
    let s = (sensitivity / 100.0).clamp(0.0, 1.0);
    let max = s.powf(1.5) * 0.5 * f.w.max(f.h) as f32;
    f.data.par_iter_mut().for_each(|v| {
        let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if l > max {
            let k = if l > 0.0 { max / l } else { 0.0 };
            v[0] *= k;
            v[1] *= k;
        }
    });
}

/// Blend the field towards each guide's displacement with a gaussian falloff.
fn apply_guides(f: &Flow, guides: &[Guide]) -> Flow {
    if guides.is_empty() {
        return f.clone();
    }
    let radius = (f.w.max(f.h) as f32 * 0.04).max(3.0);
    let inv = 1.0 / (2.0 * radius * radius);
    let data = (0..f.w * f.h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % f.w) as f32, (i / f.w) as f32);
            let mut acc = [0.0f32; 2];
            let mut wsum = 0.0;
            for g in guides {
                let d2 = (x - g.from[0]).powi(2) + (y - g.from[1]).powi(2);
                let k = (-d2 * inv).exp();
                acc[0] += k * (g.to[0] - g.from[0]);
                acc[1] += k * (g.to[1] - g.from[1]);
                wsum += k;
            }
            let v = f.data[i];
            if wsum < 1e-4 {
                return v;
            }
            let k = wsum.min(1.0);
            [v[0] * (1.0 - k) + acc[0] / wsum * k, v[1] * (1.0 - k) + acc[1] / wsum * k]
        })
        .collect();
    Flow { w: f.w, h: f.h, data }
}

fn grid(n: usize, patch: usize, stride: usize) -> Vec<usize> {
    if n <= patch {
        return vec![0];
    }
    let mut v: Vec<usize> = (0..=n - patch).step_by(stride).collect();
    if *v.last().unwrap() != n - patch {
        v.push(n - patch);
    }
    v
}

const MAXP: usize = 8;
const MAXN: usize = MAXP * MAXP;
/// Minimum texture (smallest eigenvalue of the patch structure tensor per
/// pixel) for a patch to be searched: about one 8-bit step of gradient. Flatter
/// patches (dark or smooth areas, noise) match any vector equally well and
/// would produce random jumps, so they inherit the field from their neighbours.
const MIN_TEXTURE: f32 = (1.0 / 255.0) * (1.0 / 255.0);

struct PatchResult {
    u: [f32; 2],
    weight: f32,
    /// |I1(x+u) - I0(x)| at the final vector for each patch pixel (row-major),
    /// reused by densification instead of resampling.
    res: [f32; MAXN],
}

fn level_search(g0: &Gray, g1: &Gray, mask: Option<&Gray>, init: &Flow, c: &LevelCfg) -> Flow {
    let (w, h) = (g0.w, g0.h);
    let ps = c.patch.min(w).min(h).min(MAXP);
    let grad_t = crate::prof::span("flow.gradients");
    let xs = grid(w, ps, c.stride);
    let ys = grid(h, ps, c.stride);

    // Central-difference gradients of the template frame, row by row.
    let mut gx = vec![0.0f32; w * h];
    let mut gy = vec![0.0f32; w * h];
    gx.par_chunks_mut(w).zip(gy.par_chunks_mut(w)).enumerate().for_each(|(y, (rx, ry))| {
        let row = &g0.data[y * w..(y + 1) * w];
        let up = &g0.data[y.saturating_sub(1) * w..][..w];
        let dn = &g0.data[(y + 1).min(h - 1) * w..][..w];
        for x in 0..w {
            rx[x] = 0.5 * (row[(x + 1).min(w - 1)] - row[x.saturating_sub(1)]);
            ry[x] = 0.5 * (dn[x] - up[x]);
        }
    });

    drop(grad_t);
    let search_t = crate::prof::span("flow.patch_search");
    let patches: Vec<PatchResult> = ys
        .par_iter()
        .flat_map_iter(|&py| xs.iter().map(move |&px| (px, py)))
        .map(|(px, py)| {
            let mw = match mask {
                None => 1.0,
                Some(m) => {
                    let mut s = 0.0;
                    for y in py..py + ps {
                        s += m.data[y * w + px..y * w + px + ps].iter().sum::<f32>();
                    }
                    s / (ps * ps) as f32
                }
            };
            let center = (px as f32 + ps as f32 * 0.5 - 0.5, py as f32 + ps as f32 * 0.5 - 0.5);
            let u0 = init.sample(center.0, center.1);
            if mw < 0.3 {
                return PatchResult { u: u0, weight: 0.0, res: [0.0; MAXN] };
            }
            match search_patch(g0, g1, &gx, &gy, px, py, ps, u0, c.iters) {
                Some((u, res, texture)) => PatchResult { u, weight: mw * (texture / (4.0 * MIN_TEXTURE)).min(1.0), res },
                None => PatchResult { u: u0, weight: 0.0, res: [0.0; MAXN] },
            }
        })
        .collect();

    drop(search_t);
    let _dens_t = crate::prof::span("flow.densify");
    // Densification: each pixel averages the patches covering it, weighted by
    // how well each patch's vector explains that pixel.
    let cover_lists = |starts: &[usize], n: usize| {
        let mut v: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, &s) in starts.iter().enumerate() {
            for list in &mut v[s..(s + ps).min(n)] {
                list.push(i);
            }
        }
        v
    };
    let col_of = cover_lists(&xs, w);
    let row_of = cover_lists(&ys, h);
    let nx = xs.len();
    let mut cover = vec![0.0f32; w * h];
    let mut data = vec![[0.0f32; 2]; w * h];
    data.par_chunks_mut(w).zip(cover.par_chunks_mut(w)).enumerate().for_each(|(y, (row, crow))| {
        for x in 0..w {
            let mut acc = [0.0f32; 2];
            let mut wsum = 0.0;
            for &ri in &row_of[y] {
                let ly = y - ys[ri];
                for &ci in &col_of[x] {
                    let p = &patches[ri * nx + ci];
                    if p.weight <= 0.0 {
                        continue;
                    }
                    let d = p.res[ly * ps + (x - xs[ci])];
                    let k = p.weight / (d * 255.0).max(1.0);
                    acc[0] += k * p.u[0];
                    acc[1] += k * p.u[1];
                    wsum += k;
                }
            }
            if wsum > 0.0 {
                row[x] = [acc[0] / wsum, acc[1] / wsum];
                crow[x] = 1.0;
            } else {
                // No usable patch: keep the coarse estimate with a low weight so
                // textured neighbours win in the fill below.
                row[x] = init.data[y * w + x];
                crow[x] = 0.05;
            }
        }
    });
    let flow = Flow { w, h, data };
    if cover.iter().all(|&k| k >= 1.0) {
        flow
    } else {
        fill_flow(&flow, &Gray { w, h, data: cover })
    }
}

/// Samples `g1` over a `ps` x `ps` patch at `(px, py) + u`. A translation has
/// the same bilinear weights for every pixel, so they are computed once.
#[inline]
fn warp_patch(g1: &Gray, px: usize, py: usize, ps: usize, u: [f32; 2], out: &mut [f32; MAXN]) {
    let (fx, fy) = (px as f32 + u[0], py as f32 + u[1]);
    let (x0f, y0f) = (fx.floor(), fy.floor());
    let (ax, ay) = (fx - x0f, fy - y0f);
    let (x0, y0) = (x0f as isize, y0f as isize);
    let (w, h) = (g1.w as isize, g1.h as isize);
    if x0 >= 0 && y0 >= 0 && x0 + (ps as isize) < w && y0 + (ps as isize) < h {
        let (w00, w10, w01, w11) = ((1.0 - ax) * (1.0 - ay), ax * (1.0 - ay), (1.0 - ax) * ay, ax * ay);
        let stride = g1.w;
        for r in 0..ps {
            let b = (y0 as usize + r) * stride + x0 as usize;
            let r0 = &g1.data[b..b + ps + 1];
            let r1 = &g1.data[b + stride..b + stride + ps + 1];
            let o = &mut out[r * ps..r * ps + ps];
            for c in 0..ps {
                o[c] = w00 * r0[c] + w10 * r0[c + 1] + w01 * r1[c] + w11 * r1[c + 1];
            }
        }
    } else {
        for r in 0..ps {
            for c in 0..ps {
                out[r * ps + c] = g1.sample(fx + c as f32, fy + r as f32);
            }
        }
    }
}

/// Inverse-compositional Gauss-Newton on one patch, with mean normalisation
/// so global brightness changes do not drag the vector. Returns the vector
/// with the lowest error seen (the initial one included) and its residuals.
#[allow(clippy::too_many_arguments)]
fn search_patch(
    g0: &Gray,
    g1: &Gray,
    gx: &[f32],
    gy: &[f32],
    px: usize,
    py: usize,
    ps: usize,
    u0: [f32; 2],
    iters: usize,
) -> Option<([f32; 2], [f32; MAXN], f32)> {
    let w = g0.w;
    let n = ps * ps;
    let nf = n as f32;
    let mut t = [0.0f32; MAXN];
    let mut tgx = [0.0f32; MAXN];
    let mut tgy = [0.0f32; MAXN];
    let (mut hxx, mut hxy, mut hyy, mut tmean) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for r in 0..ps {
        let b = (py + r) * w + px;
        for c in 0..ps {
            let (k, i) = (r * ps + c, b + c);
            t[k] = g0.data[i];
            tgx[k] = gx[i];
            tgy[k] = gy[i];
            hxx += gx[i] * gx[i];
            hxy += gx[i] * gy[i];
            hyy += gy[i] * gy[i];
            tmean += g0.data[i];
        }
    }
    tmean /= nf;
    hxx += 1e-4;
    hyy += 1e-4;
    let det = hxx * hyy - hxy * hxy;
    // Smallest eigenvalue of the structure tensor, per pixel.
    let texture = (0.5 * (hxx + hyy) - (0.25 * (hxx - hyy) * (hxx - hyy) + hxy * hxy).sqrt()) / nf;
    if texture < MIN_TEXTURE || det.abs() < 1e-12 {
        return None;
    }
    let mut buf = [0.0f32; MAXN];
    let residuals = |u: [f32; 2], buf: &mut [f32; MAXN]| {
        warp_patch(g1, px, py, ps, u, buf);
        let mut res = [0.0f32; MAXN];
        for k in 0..n {
            res[k] = (buf[k] - t[k]).abs();
        }
        res
    };
    let (ixx, ixy, iyy) = (hyy / det, -hxy / det, hxx / det);

    let mut u = u0;
    let (mut best_u, mut best_e) = (u0, f32::INFINITY);
    let mut stop = false;
    for it in 0..=iters {
        warp_patch(g1, px, py, ps, u, &mut buf);
        let dm = buf[..n].iter().sum::<f32>() / nf - tmean;
        let (mut bx, mut by, mut e) = (0.0f32, 0.0f32, 0.0f32);
        for k in 0..n {
            let d = buf[k] - t[k] - dm;
            bx += tgx[k] * d;
            by += tgy[k] * d;
            e += d * d;
        }
        if e < best_e {
            best_e = e;
            best_u = u;
        }
        if stop || it == iters {
            break;
        }
        let du = [ixx * bx + ixy * by, ixy * bx + iyy * by];
        u[0] -= du[0];
        u[1] -= du[1];
        if !(u[0].is_finite() && u[1].is_finite()) {
            break;
        }
        // Converged: evaluate this last step once more, then stop.
        stop = du[0] * du[0] + du[1] * du[1] < 1e-4;
    }
    // A refinement level should only adjust the coarser estimate; a jump of
    // more than a patch is a mismatch, not motion.
    let jump = (best_u[0] - u0[0]).hypot(best_u[1] - u0[1]);
    if jump > ps as f32 {
        return None;
    }
    Some((best_u, residuals(best_u, &mut buf), texture))
}