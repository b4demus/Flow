//! Coarse-to-fine TV-L1 flow and inverse maps. Native iteration counts,
//! five-point derivatives, primal/dual updates, projection and median passes.
use crate::Image;
use rayon::prelude::*;
#[derive(Clone)]
struct Field {
    w: usize,
    h: usize,
    a: Vec<f32>,
}
impl Field {
    fn zero(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            a: vec![0.0; w * h],
        }
    }
    fn at(&self, x: i32, y: i32) -> f32 {
        self.a[y.clamp(0, self.h as i32 - 1) as usize * self.w
            + x.clamp(0, self.w as i32 - 1) as usize]
    }
    fn sample(&self, x: f32, y: f32, black: bool) -> f32 {
        if black
            && !(x > 0.0001
                && y > 0.0001
                && x < self.w as f32 - 1.0001
                && y < self.h as f32 - 1.0001)
        {
            return 0.0;
        }
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (dx, dy) = (x - ix as f32, y - iy as f32);
        let a = self.at(ix, iy) + (self.at(ix + 1, iy) - self.at(ix, iy)) * dx;
        let b = self.at(ix, iy + 1) + (self.at(ix + 1, iy + 1) - self.at(ix, iy + 1)) * dx;
        a + (b - a) * dy
    }
    fn resize(&self, w: usize, h: usize, mult: f32) -> Self {
        let weights = |old: usize, new: usize| -> Vec<Vec<(usize, f32)>> {
            if old == 1 {
                return vec![vec![(0, 1.0)]; new];
            }
            let step = (old - 1) as f32 / (new - 1).max(1) as f32;
            let shrink = new < old;
            let scale = if shrink { 1.0 / step } else { 1.0 };
            let radius = 2.0 / scale;
            let mut pos = 0.0f32;
            let mut lo = -radius;
            let mut hi = radius;
            (0..new)
                .map(|_| {
                    let begin = (lo.ceil() as i32).max(0);
                    let end = (hi.floor() as i32 + 1).min(old as i32);
                    let mut a = Vec::new();
                    let mut x = (begin as f32 - pos) * scale;
                    for i in begin..end {
                        let t = x.abs();
                        let k = if t >= 2.0 {
                            0.0
                        } else if shrink {
                            if t < 1.0 {
                                t * t * (3.0 * t - 6.0) + 4.0
                            } else {
                                t * (t * (-t + 6.0) - 12.0) + 8.0
                            }
                        } else if t < 1.0 {
                            t * t * (1.5 * t - 2.5) + 1.0
                        } else {
                            t * (t * (-0.5 * t + 2.5) - 4.0) + 2.0
                        };
                        a.push((i as usize, k));
                        x += scale;
                    }
                    let sum: f32 = a.iter().map(|v| v.1).sum();
                    for v in &mut a {
                        v.1 /= sum;
                    }
                    pos += step;
                    lo += step;
                    hi += step;
                    a
                })
                .collect()
        };
        let (wx, wy) = (weights(self.w, w), weights(self.h, h));
        let mut tmp = Self::zero(w, self.h);
        tmp.a.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                row[x] = wx[x].iter().map(|(i, k)| self.a[y * self.w + i] * k).sum();
            }
        });
        let mut out = Self::zero(w, h);
        out.a.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                row[x] = wy[y].iter().map(|(i, k)| tmp.a[i * w + x] * k).sum::<f32>() * mult;
            }
        });
        out
    }
    fn normalized(&mut self) {
        let lo = self.a.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = self.a.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if hi - lo > 1e-5 {
            for v in &mut self.a {
                *v = 2.0 / (hi - lo) * (*v - lo) - 1.0;
            }
        }
    }
    fn centered(&mut self, other: &mut Self) {
        let a = self.a.iter().sum::<f32>() / self.a.len() as f32;
        let b = other.a.iter().sum::<f32>() / other.a.len() as f32;
        let t = (a + b) * 0.5 - a;
        for v in &mut self.a {
            *v += t;
        }
        for v in &mut other.a {
            *v -= t;
        }
    }
    fn derivative(&self, vertical: bool) -> Self {
        let mut out = Self::zero(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let (x, y) = (x as i32, y as i32);
                let v = |d| {
                    if vertical {
                        self.at(x, y + d)
                    } else {
                        self.at(x + d, y)
                    }
                };
                out.a[y as usize * self.w + x as usize] =
                    (-v(-2) + 8.0 * v(-1) - 8.0 * v(1) + v(2)) * (-1.0 / 12.0);
            }
        }
        out
    }
    fn median(&mut self) {
        let old = self.clone();
        self.a.par_iter_mut().enumerate().for_each(|(i, v)| {
            let (x, y) = ((i % old.w) as i32, (i / old.w) as i32);
            let mut a = [0.0; 9];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    a[((dy + 1) * 3 + dx + 1) as usize] = old.at(x + dx, y + dy);
                }
            }
            a.sort_unstable_by(f32::total_cmp);
            *v = a[4];
        });
    }
    fn emphasize(&mut self, amount: f32) {
        if amount <= 0.01 {
            return;
        }
        // Native preprocessing subtracts an ROF component, not a Gaussian.
        // Thirty primal/dual steps, sigma=.2, dual step=.245/sigma.
        let mut p = Self::zero(self.w, self.h);
        let mut q = p.clone();
        let mut smooth = self.clone();
        for _ in 0..30 {
            for y in 0..self.h {
                for x in 0..self.w {
                    let i = y * self.w + x;
                    let dx = if x == 0 { p.a[i] } else if x + 1 == self.w {
                        -p.a[i - 1]
                    } else { p.a[i] - p.a[i - 1] };
                    let dy = if y == 0 { q.a[i] } else if y + 1 == self.h {
                        -q.a[i - self.w]
                    } else { q.a[i] - q.a[i - self.w] };
                    smooth.a[i] = 0.2f32.mul_add(dx + dy, self.a[i]);
                }
            }
            for y in 0..self.h {
                for x in 0..self.w {
                    let i = y * self.w + x;
                    let dx = if x + 1 < self.w { smooth.a[i + 1] - smooth.a[i] } else { 0.0 };
                    let dy = if y + 1 < self.h { smooth.a[i + self.w] - smooth.a[i] } else { 0.0 };
                    let px = (0.245f32 / 0.2).mul_add(dx, p.a[i]);
                    let py = (0.245f32 / 0.2).mul_add(dy, q.a[i]);
                    let norm = px.mul_add(px, py * py).sqrt().max(1.0);
                    p.a[i] = px / norm;
                    q.a[i] = py / norm;
                }
            }
        }
        for (i, v) in self.a.iter_mut().enumerate() {
            *v -= amount * smooth.a[i];
        }
    }
    fn zero_border(&mut self) {
        for y in 0..self.h {
            self.a[y * self.w] = 0.0;
            self.a[y * self.w + self.w - 1] = 0.0;
        }
        self.a[..self.w].fill(0.0);
        self.a[(self.h - 1) * self.w..].fill(0.0);
    }
}
pub fn compute(src: &Image, dst: &Image, lambda: f32, emphasize: f32) -> Image {
    let mono = |s: &Image| Field {
        w: s.w,
        h: s.h,
        a: s.data
            .iter()
            .map(|p| (p[0] * 0.3125 + p[1] * 0.5) + p[2] * 0.1875)
            .collect(),
    };
    let (mut a, mut b) = (mono(src), mono(dst));
    a.emphasize(emphasize);
    b.emphasize(emphasize);
    a.normalized();
    b.normalized();
    a.centered(&mut b);
    let mut pyr = vec![(a, b)];
    loop {
        let (a, b) = pyr.last().unwrap();
        let (w, h) = (
            (a.w as f32 * 0.707 + 0.5) as usize,
            (a.h as f32 * 0.707 + 0.5) as usize,
        );
        if w < 4 || h < 4 || w == a.w && h == a.h {
            break;
        }
        let (mut aa, mut bb) = (a.resize(w, h, 1.0), b.resize(w, h, 1.0));
        aa.emphasize(emphasize);
        bb.emphasize(emphasize);
        aa.normalized();
        bb.normalized();
        aa.centered(&mut bb);
        pyr.push((aa, bb));
        if pyr.len() >= 38 {
            break;
        }
    }
    let (mut u, mut v) = (Field::zero(1, 1), Field::zero(1, 1));
    let mut pu = Field::zero(1, 1);
    let mut qu = pu.clone();
    let mut pv = pu.clone();
    let mut qv = pu.clone();
    for (a, b) in pyr.into_iter().rev() {
        let (w, h) = (a.w, a.h);
        let (sx, sy) = (w as f32 / u.w as f32, h as f32 / u.h as f32);
        u = u.resize(w, h, sx);
        v = v.resize(w, h, sy);
        let (bx, by) = (b.derivative(false), b.derivative(true));
        let mut weights = vec![1.0; w * h];
        {
            let (ax, ay) = (a.derivative(false), a.derivative(true));
            for i in 0..w * h {
                weights[i] =
                    (-5.0 * (ax.a[i] * ax.a[i] + ay.a[i] * ay.a[i]).sqrt().powf(0.9)).exp();
            }
        }
        pu.zero_border();
        qu.zero_border();
        pv.zero_border();
        qv.zero_border();
        pu = pu.resize(w, h, 1.0);
        qu = qu.resize(w, h, 1.0);
        pv = pv.resize(w, h, 1.0);
        qv = qv.resize(w, h, 1.0);
        for _ in 0..5 {
            let mut gx = vec![0.0; w * h];
            let mut gy = gx.clone();
            let mut rho = gx.clone();
            for i in 0..w * h {
                let (x, y) = ((i % w) as f32 + u.a[i], (i / w) as f32 + v.a[i]);
                gx[i] = bx.sample(x, y, true);
                gy[i] = by.sample(x, y, true);
                rho[i] = (b.sample(x, y, true) - (u.a[i] * gx[i] + v.a[i] * gy[i])) - a.a[i];
            }
            for _ in 0..3 {
                let mut au = vec![0.0; w * h];
                let mut av = au.clone();
                let lt = lambda * 0.225;
                for i in 0..w * h {
                    let g = gx[i] * gx[i] + gy[i] * gy[i];
                    let r = rho[i] + u.a[i] * gx[i] + v.a[i] * gy[i];
                    let k = if r < -lt * g {
                        lt
                    } else if r > lt * g {
                        -lt
                    } else if g >= 1e-10 {
                        -r / g
                    } else {
                        0.0
                    };
                    au[i] = u.a[i] + k * gx[i];
                    av[i] = v.a[i] + k * gy[i];
                }
                let div = |p: &Field, q: &Field, x: usize, y: usize| {
                    let i = y * w + x;
                    let dx = if x == 0 {
                        p.a[i]
                    } else if x == w - 1 {
                        -p.a[i - 1]
                    } else {
                        p.a[i] - p.a[i - 1]
                    };
                    let dy = if y == 0 {
                        q.a[i]
                    } else if y == h - 1 {
                        -q.a[i - w]
                    } else {
                        q.a[i] - q.a[i - w]
                    };
                    dx + dy
                };
                let (mut nu, mut nv) = (Field::zero(w, h), Field::zero(w, h));
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        nu.a[i] = au[i] + 0.225 * weights[i] * div(&pu, &qu, x, y);
                        nv.a[i] = av[i] + 0.225 * weights[i] * div(&pv, &qv, x, y);
                    }
                }
                let step = 1.0 / (4.0 + 0.02);
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        for (f, p, q) in [(&nu, &mut pu, &mut qu), (&nv, &mut pv, &mut qv)] {
                            let dx = if x + 1 < w { f.a[i + 1] - f.a[i] } else { 0.0 };
                            let dy = if y + 1 < h { f.a[i + w] - f.a[i] } else { 0.0 };
                            let px = p.a[i] + step * (weights[i] * dx - 0.02 * p.a[i]);
                            let py = q.a[i] + step * (weights[i] * dy - 0.02 * q.a[i]);
                            let norm = (px * px + py * py).sqrt().max(1.0);
                            p.a[i] = px / norm;
                            q.a[i] = py / norm;
                        }
                    }
                }
                u = nu;
                v = nv;
            }
            u.median();
            v.median();
        }
    }
    let (ux, uy, vx, vy) = (
        u.derivative(false),
        u.derivative(true),
        v.derivative(false),
        v.derivative(true),
    );
    let (mut a, mut b) = (mono(src), mono(dst));
    a.emphasize(emphasize);
    b.emphasize(emphasize);
    a.normalized();
    b.normalized();
    a.centered(&mut b);
    let mut out = Image::new(src.w, src.h);
    for i in 0..out.data.len() {
        let d = (a.a[i]
            - b.sample(
                (i % src.w) as f32 + u.a[i],
                (i / src.w) as f32 + v.a[i],
                true,
            ))
        .abs();
        let confidence = 1.0
            / ((ux.a[i] * ux.a[i] + uy.a[i] * uy.a[i]).sqrt()
                + (vx.a[i] * vx.a[i] + vy.a[i] * vy.a[i]).sqrt()
                + lambda * d
                + 1.0);
        out.data[i] = [u.a[i], -v.a[i], confidence, 1.0];
    }
    out
}
/// Nearest-pixel scatter, photometric collision selection and 25 hole-fill passes.
pub fn inverse(src: &Image, dst: &Image, flow: &Image) -> Image {
    let (w, h) = (src.w, src.h);
    let mut uv = vec![[0.0; 2]; w * h];
    let mut valid = vec![false; w * h];
    let mut cost = vec![f32::INFINITY; w * h];
    for y in (0..h).rev() {
        for x in 0..w {
            let i = y * w + x;
            let f = flow.data[i];
            let xx = (x as f32 + f[0]).round_ties_even() as i32;
            let yy = (y as f32 - f[1]).round_ties_even() as i32;
            if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                continue;
            }
            let j = yy as usize * w + xx as usize;
            // CUDA resolve_collisions evaluates its residual with +v even
            // though find_target_locations scatters using -v. Keep that quirk.
            let b = super::mosh::sample(dst, x as f32 + f[0], y as f32 + f[1], false, false);
            let c = (0..3).fold(0.0, |sum, c| {
                let d = src.data[i][c] - b[c];
                d.mul_add(d, sum)
            });
            if c < cost[j] {
                cost[j] = c;
                uv[j] = [f[0], f[1]];
                valid[j] = true;
            }
        }
    }
    let original_valid = valid.clone();
    let mut current = uv.clone();
    let mut new = vec![[0.0; 2]; w * h];
    let mut vnew = vec![false; w * h];
    for _ in 0..25 {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if original_valid[i] {
                    continue;
                }
                let (mut sum, mut count) = ([0.0; 2], 0.0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                        if xx >= 0 && yy >= 0 && xx < w as i32 && yy < h as i32 {
                            let j = yy as usize * w + xx as usize;
                            if valid[j] {
                                sum[0] += current[j][0];
                                sum[1] += current[j][1];
                                count += 1.0;
                            }
                        }
                    }
                }
                if count > 0.0 {
                    new[i] = [sum[0] / count, sum[1] / count];
                    vnew[i] = true;
                }
            }
        }
        current.clone_from(&new);
        valid.clone_from(&vnew);
    }
    for i in 0..w * h {
        if !original_valid[i] {
            uv[i] = current[i];
        }
    }
    let mut out = Image::new(w, h);
    for i in 0..w * h {
        out.data[i] = [
            -uv[i][0] * 2.0 / w as f32,
            -uv[i][1] * 2.0 / w as f32,
            0.0,
            1.0,
        ];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translation_inverse_uses_sapphire_normalized_axes() {
        let src = Image::filled(64, 48, [0.4, 0.4, 0.4, 1.0]);
        let f = Image::filled(64, 48, [2.0, -1.0, 1.0, 1.0]);
        let a = inverse(&src, &src, &f);
        assert_eq!(a.at(32, 24), [-4.0 / 64.0, 2.0 / 64.0, 0.0, 1.0]);
    }
}
