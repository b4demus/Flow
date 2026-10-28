//! Exact Euclidean distance transform (Felzenszwalb & Huttenlocher 2012):
//! squared distance from every pixel to the nearest pixel of a set, in two
//! separable passes of the 1-D lower-envelope algorithm. O(w·h).

use crate::blur::transpose;
use rayon::prelude::*;

const INF: f32 = 1e20;

/// Squared distance (in pixels²) to the nearest `true` pixel; INF-ish where the set is empty.
pub fn squared(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let mut f: Vec<f32> = mask.iter().map(|&m| if m { 0.0 } else { INF }).collect();
    // columns (via transpose so the work is row-contiguous), then rows
    let mut t = transpose(&f, w, h);
    t.par_chunks_mut(h.max(1))
        .for_each_init(Scratch::default, |s, col| line(col, s));
    f = transpose(&t, h, w);
    f.par_chunks_mut(w.max(1))
        .for_each_init(Scratch::default, |s, row| line(row, s));
    f
}

#[derive(Default)]
struct Scratch {
    d: Vec<f32>,
    v: Vec<usize>,
    z: Vec<f32>,
}

fn line(f: &mut [f32], s: &mut Scratch) {
    let n = f.len();
    if n == 0 {
        return;
    }
    s.d.clear();
    s.d.resize(n, 0.0);
    s.v.clear();
    s.v.resize(n, 0);
    s.z.clear();
    s.z.resize(n + 1, 0.0);
    let (d, v, z) = (&mut s.d, &mut s.v, &mut s.z);
    let mut k = 0usize;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        let fq = f[q] + (q * q) as f32;
        loop {
            let p = v[k];
            let sx = (fq - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
            if sx <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if sx <= z[k] {
                // k == 0 and the new parabola wins everywhere left of z[1]
                v[0] = q;
                z[0] = -INF;
                z[1] = INF;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = sx;
            z[k + 1] = INF;
            break;
        }
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f32 - p as f32;
        d[q] = dq * dq + f[p];
    }
    f.copy_from_slice(d);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_brute_force() {
        let (w, h) = (23, 17);
        let mut seed = 7u32;
        let mask: Vec<bool> = (0..w * h)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed % 13 == 0
            })
            .collect();
        let d = squared(&mask, w, h);
        for y in 0..h {
            for x in 0..w {
                let mut best = f32::MAX;
                for yy in 0..h {
                    for xx in 0..w {
                        if mask[yy * w + xx] {
                            let dx = x as f32 - xx as f32;
                            let dy = y as f32 - yy as f32;
                            best = best.min(dx * dx + dy * dy);
                        }
                    }
                }
                assert_eq!(d[y * w + x], best, "at {x},{y}");
            }
        }
    }
}
