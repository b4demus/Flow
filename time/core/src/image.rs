//! Plain float image containers shared by every stage of the pipeline.

use rayon::prelude::*;

/// RGBA image, premultiplied alpha, channels in 0..1 (values outside are kept).
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub data: Vec<[f32; 4]>,
}

/// Single channel float plane (luma for tracking, mattes, weights).
#[derive(Clone, Debug)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

/// Per-pixel displacement in pixels: `[dx, dy]`, +y points down.
#[derive(Clone, Debug)]
pub struct Flow {
    pub w: usize,
    pub h: usize,
    pub data: Vec<[f32; 2]>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, data: vec![[0.0; 4]; w * h] }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [f32; 4] {
        self.data[y * self.w + x]
    }

    /// Bilinear sample with clamp-to-edge. Returns the value and whether the
    /// position was inside the image (used for edge-aware blending).
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> ([f32; 4], bool) {
        let inside = x >= -0.5 && y >= -0.5 && x <= self.w as f32 - 0.5 && y <= self.h as f32 - 0.5;
        (bilinear4(&self.data, self.w, self.h, x, y), inside)
    }

    /// Rec.601 luma, optionally mixing alpha in (Track RGB+A).
    pub fn luma(&self, with_alpha: bool) -> Gray {
        let data = self
            .data
            .par_iter()
            .map(|p| {
                let l = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                if with_alpha { 0.75 * l + 0.25 * p[3] } else { l }
            })
            .collect();
        Gray { w: self.w, h: self.h, data }
    }

    pub fn scale_by(&self, m: &Gray) -> Image {
        let data = self
            .data
            .par_iter()
            .zip(m.data.par_iter())
            .map(|(p, &k)| [p[0] * k, p[1] * k, p[2] * k, p[3] * k])
            .collect();
        Image { w: self.w, h: self.h, data }
    }

    /// Premultiplied "over": `self` on top of `under`.
    pub fn over(&self, under: &Image) -> Image {
        let data = self
            .data
            .par_iter()
            .zip(under.data.par_iter())
            .map(|(a, b)| {
                let k = 1.0 - a[3].clamp(0.0, 1.0);
                [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2], a[3] + k * b[3]]
            })
            .collect();
        Image { w: self.w, h: self.h, data }
    }

    pub fn lerp(a: &Image, b: &Image, t: f32) -> Image {
        let data = a
            .data
            .par_iter()
            .zip(b.data.par_iter())
            .map(|(p, q)| {
                [
                    p[0] + (q[0] - p[0]) * t,
                    p[1] + (q[1] - p[1]) * t,
                    p[2] + (q[2] - p[2]) * t,
                    p[3] + (q[3] - p[3]) * t,
                ]
            })
            .collect();
        Image { w: a.w, h: a.h, data }
    }
}

impl Gray {
    pub fn new(w: usize, h: usize, v: f32) -> Self {
        Self { w, h, data: vec![v; w * h] }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.w + x]
    }

    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        bilinear1(&self.data, self.w, self.h, x, y)
    }

    pub fn map(&self, f: impl Fn(f32) -> f32 + Sync) -> Gray {
        Gray { w: self.w, h: self.h, data: self.data.par_iter().map(|&v| f(v)).collect() }
    }
}

impl Flow {
    pub fn zero(w: usize, h: usize) -> Self {
        Self { w, h, data: vec![[0.0; 2]; w * h] }
    }

    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> [f32; 2] {
        bilinear2(&self.data, self.w, self.h, x, y)
    }

    pub fn scaled(&self, k: f32) -> Flow {
        Flow { w: self.w, h: self.h, data: self.data.par_iter().map(|v| [v[0] * k, v[1] * k]).collect() }
    }
}

#[inline]
fn coords(w: usize, h: usize, x: f32, y: f32) -> (usize, usize, usize, usize, f32, f32) {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    (x0, y0, x1, y1, x - x0 as f32, y - y0 as f32)
}

#[inline]
pub(crate) fn bilinear1(d: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x0, y0, x1, y1, fx, fy) = coords(w, h, x, y);
    let a = d[y0 * w + x0] + (d[y0 * w + x1] - d[y0 * w + x0]) * fx;
    let b = d[y1 * w + x0] + (d[y1 * w + x1] - d[y1 * w + x0]) * fx;
    a + (b - a) * fy
}

#[inline]
pub(crate) fn bilinear2(d: &[[f32; 2]], w: usize, h: usize, x: f32, y: f32) -> [f32; 2] {
    let (x0, y0, x1, y1, fx, fy) = coords(w, h, x, y);
    let mut out = [0.0; 2];
    for c in 0..2 {
        let a = d[y0 * w + x0][c] + (d[y0 * w + x1][c] - d[y0 * w + x0][c]) * fx;
        let b = d[y1 * w + x0][c] + (d[y1 * w + x1][c] - d[y1 * w + x0][c]) * fx;
        out[c] = a + (b - a) * fy;
    }
    out
}

#[inline]
pub(crate) fn bilinear4(d: &[[f32; 4]], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let (x0, y0, x1, y1, fx, fy) = coords(w, h, x, y);
    let mut out = [0.0; 4];
    for c in 0..4 {
        let a = d[y0 * w + x0][c] + (d[y0 * w + x1][c] - d[y0 * w + x0][c]) * fx;
        let b = d[y1 * w + x0][c] + (d[y1 * w + x1][c] - d[y1 * w + x0][c]) * fx;
        out[c] = a + (b - a) * fy;
    }
    out
}
