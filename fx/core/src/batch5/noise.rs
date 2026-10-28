//! Lattice noise, quadratic refinement and centroid dots used by PixelSort.
//! Equations recovered from the native CPU refinement and CUDA kernels.
use crate::shake::hash4;

fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    let row = hash4((y.wrapping_mul(-16) as u32) ^ seed);
    ((hash4(row.wrapping_add(x as u32)) & 8191) as i32 - 4096) as f32 / 4096.0
}
fn expand(a: &[f32]) -> Vec<f32> {
    if a.len() < 3 {
        return a.to_vec();
    }
    let n = 2 * a.len() - 3;
    let mut b = vec![0.0; n];
    for i in 0..a.len() - 1 {
        b[2 * i] = (a[i] + a[i + 1]) * 0.5;
    }
    for i in 0..a.len() - 2 {
        b[2 * i + 1] = (a[i + 1] * 2.0 + b[2 * i] + b[2 * i + 2] + 1.0 / 4096.0) * 0.25;
    }
    b
}
fn resize(a: &[f32], lo: f32, hi: f32, n: usize) -> Vec<f32> {
    let mut x = lo;
    (0..n)
        .map(|_| {
            let i = (x as usize).min(a.len().saturating_sub(2));
            let v = a[i] + (x - i as f32) * (a[(i + 1).min(a.len() - 1)] - a[i]);
            x += (hi - lo) / n as f32;
            v
        })
        .collect()
}
fn along_x(a: &[f32], w: usize, h: usize, shift: usize, n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n * h);
    for y in 0..h {
        let row = expand(&a[y * w..(y + 1) * w]);
        out.extend_from_slice(&row[shift..shift + n]);
    }
    out
}
fn along_y(a: &[f32], w: usize, h: usize, shift: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0.0; w * n];
    for x in 0..w {
        let col: Vec<_> = (0..h).map(|y| a[y * w + x]).collect();
        let b = expand(&col);
        for y in 0..n {
            out[y * w + x] = b[y + shift];
        }
    }
    out
}
pub fn field(w: usize, h: usize, frequency: f32, detail: usize, seed: f32) -> Vec<f32> {
    if w < 5 || h < 5 {
        return vec![0.5; w * h];
    }
    // The CPU primitive uses an integer right shift for this upper bound.
    let fx = (frequency.abs() * 2.0).min(((w - 4) / 2) as f32);
    let fy = (frequency.abs() * 2.0).min((h as f32 - 4.0) * 0.5 / (h as f32 / w as f32));
    let (mut x0, mut x1, mut y0, mut y1) =
        (-fx, fx, -fy * h as f32 / w as f32, fy * h as f32 / w as f32);
    let (mut ox, mut oy) = (x0.floor() as i32 - 1, y0.floor() as i32 - 1);
    let (mut nx, mut ny) = (
        (x1.ceil() as i32 + 2 - ox) as usize,
        (y1.ceil() as i32 + 2 - oy) as usize,
    );
    let seed = (seed * 4096.0) as i32 as u32;
    let mut a: Vec<_> = (0..ny)
        .flat_map(|y| (0..nx).map(move |x| lattice(x as i32 + ox, y as i32 + oy, seed)))
        .collect();
    let mut octave = 1;
    loop {
        let ex = nx * 2 - 3 <= w && x1 > x0;
        let ey = ny * 2 - 3 <= h && y1 > y0;
        if !ex && !ey {
            break;
        }
        if ex {
            x0 *= 2.0;
            x1 *= 2.0;
            let o = x0.floor() as i32 - 1;
            let n = (x1.ceil() as i32 + 2 - o) as usize;
            a = along_x(&a, nx, ny, (o - (2 * ox + 1)) as usize, n);
            nx = n;
            ox = o;
        }
        if ey {
            y0 *= 2.0;
            y1 *= 2.0;
            let o = y0.floor() as i32 - 1;
            let n = (y1.ceil() as i32 + 2 - o) as usize;
            a = along_y(&a, nx, ny, (o - (2 * oy + 1)) as usize, n);
            ny = n;
            oy = o;
        }
        if ex && ey && octave < detail {
            let k = 1.0 / (1u32 << octave.min(30)) as f32;
            for y in 0..ny {
                for x in 0..nx {
                    a[y * nx + x] +=
                        lattice(ox + x as i32, oy + y as i32, seed ^ octave as u32) * k;
                }
            }
        }
        octave += 1;
    }
    for v in &mut a {
        *v = *v * 0.5 + 0.5;
    }
    let mut b = vec![0.0; w * ny];
    for y in 0..ny {
        let row = expand(&a[y * nx..(y + 1) * nx]);
        let v = resize(
            &row,
            x0 * 2.0 - (ox * 2 + 1) as f32,
            x1 * 2.0 - (ox * 2 + 1) as f32,
            w,
        );
        b[y * w..(y + 1) * w].copy_from_slice(&v);
    }
    let mut out = vec![0.0; w * h];
    for x in 0..w {
        let col: Vec<_> = (0..ny).map(|y| b[y * w + x]).collect();
        let row = expand(&col);
        let v = resize(
            &row,
            y0 * 2.0 - (oy * 2 + 1) as f32,
            y1 * 2.0 - (oy * 2 + 1) as f32,
            h,
        );
        for y in 0..h {
            out[y * w + x] = v[y];
        }
    }
    out
}
pub fn dots(a: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut seen = vec![false; a.len()];
    // CPU component extraction consumes the bright regions before drawing dots.
    let mut out = vec![0.0; a.len()];
    let mut stack = Vec::new();
    for i in 0..a.len() {
        if seen[i] || a[i] <= 0.8 {
            continue;
        }
        seen[i] = true;
        stack.push(i);
        let (mut sum, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32);
        while let Some(j) = stack.pop() {
            let (x, y) = (j % w, j / w);
            let v = a[j];
            sum += v;
            sx += x as f32 * v;
            sy += y as f32 * v;
            for (xx, yy) in [
                (x as i32 - 1, y as i32),
                (x as i32 + 1, y as i32),
                (x as i32, y as i32 - 1),
                (x as i32, y as i32 + 1),
            ] {
                if xx >= 0 && xx < w as i32 && yy >= 0 && yy < h as i32 {
                    let k = yy as usize * w + xx as usize;
                    if !seen[k] && a[k] > 0.8 {
                        seen[k] = true;
                        stack.push(k);
                    }
                }
            }
        }
        if sum > 0.0 {
            let (x, y) = (sx / sum, sy / sum);
            let (ix, iy) = (x as usize, y as usize);
            let (dx, dy) = (x - ix as f32, y - iy as f32);
            for (xx, yy, v) in [
                (ix, iy, (1.0 - dx) * (1.0 - dy)),
                ((ix + 1).min(w - 1), iy, dx * (1.0 - dy)),
                (ix, (iy + 1).min(h - 1), (1.0 - dx) * dy),
                ((ix + 1).min(w - 1), (iy + 1).min(h - 1), dx * dy),
            ] {
                out[yy * w + xx] += v;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_noise_is_finite_repeatable_and_seeded() {
        let a = field(123, 77, 18.0, 5, 0.273);
        assert_eq!(a, field(123, 77, 18.0, 5, 0.273));
        assert_ne!(a, field(123, 77, 18.0, 5, 0.4));
        assert!(a.iter().all(|v| v.is_finite()));
        assert_eq!(field(1, 1, 100.0, 5, 0.0).len(), 1);
    }
}
