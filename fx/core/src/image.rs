//! Float image containers. Colour is straight (not premultiplied) unless a
//! function says otherwise; values outside 0..1 are kept (HDR).

use rayon::prelude::*;

/// RGBA image, `[r, g, b, a]` per pixel, row-major.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub data: Vec<[f32; 4]>,
}

/// One float channel.
#[derive(Clone, Debug)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            data: vec![[0.0; 4]; w * h],
        }
    }

    pub fn filled(w: usize, h: usize, px: [f32; 4]) -> Self {
        Self {
            w,
            h,
            data: vec![px; w * h],
        }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [f32; 4] {
        self.data[y * self.w + x]
    }

    pub fn channel(&self, c: usize) -> Plane {
        Plane {
            w: self.w,
            h: self.h,
            data: self.data.par_iter().map(|p| p[c]).collect(),
        }
    }

    pub fn set_channel(&mut self, c: usize, p: &Plane) {
        debug_assert!(p.w == self.w && p.h == self.h);
        self.data
            .par_iter_mut()
            .zip(p.data.par_iter())
            .for_each(|(o, v)| o[c] = *v);
    }

    pub fn premultiply(&mut self) {
        self.data.par_iter_mut().for_each(|p| {
            p[0] *= p[3];
            p[1] *= p[3];
            p[2] *= p[3];
        });
    }

    pub fn unpremultiply(&mut self) {
        self.data.par_iter_mut().for_each(|p| {
            if p[3] > 1e-6 {
                let k = 1.0 / p[3];
                p[0] *= k;
                p[1] *= k;
                p[2] *= k;
            } else {
                *p = [0.0; 4];
            }
        });
    }
}

impl Plane {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            data: vec![0.0; w * h],
        }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.w + x]
    }
}

/// Rec.709 luminance weights.
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[inline]
pub fn luma(p: [f32; 4]) -> f32 {
    LUMA[0] * p[0] + LUMA[1] * p[1] + LUMA[2] * p[2]
}
