//! How an effect layer (glow, rays, glints) is put onto the background.

/// Combine modes, in the order of the effects' popup menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    Mult,
    Add,
    Screen,
    Difference,
    Overlay,
}

impl Combine {
    /// 1-based popup index.
    pub fn from_popup(v: i32) -> Self {
        match v {
            1 => Self::Mult,
            2 => Self::Add,
            4 => Self::Difference,
            5 => Self::Overlay,
            _ => Self::Screen,
        }
    }

    /// One premultiplied colour channel: background `b`, effect `g`. Screen
    /// never goes below either input, which keeps it sane for HDR and negative
    /// values; Overlay falls back to max(b, g) above 1 (measured, see
    /// spec/effects/glow-observations.md). Mult also scales the alpha; that part
    /// lives with the caller.
    #[inline]
    pub fn apply(self, b: f32, g: f32) -> f32 {
        let inside = |v: f32| (0.0..=1.0).contains(&v);
        match self {
            Self::Mult => b * g,
            Self::Add => b + g,
            Self::Screen => (b + g - b * g).max(b).max(g),
            Self::Difference => (b - g).abs(),
            Self::Overlay => {
                if b < 0.5 {
                    2.0 * b * g
                } else if !inside(b) || !inside(g) {
                    b.max(g)
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - g)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_measured() {
        assert!((Combine::Screen.apply(0.4995, 1.998) - 1.998).abs() < 1e-6);
        assert!((Combine::Screen.apply(0.2, 0.8) - 0.84).abs() < 1e-6);
        assert!((Combine::Screen.apply(-0.14, 0.0) - 0.0).abs() < 1e-6);
        // negative background, HDR glow (c1.json noise_hdr #357, blue)
        assert!((Combine::Screen.apply(-0.4513, 1.1874) - 1.272).abs() < 1e-3);
        assert!((Combine::Mult.apply(1.2, 4.8) - 5.76).abs() < 1e-5);
        assert!((Combine::Overlay.apply(0.8, 0.8) - 0.92).abs() < 1e-6);
        assert!((Combine::Overlay.apply(0.8, 3.2) - 3.2).abs() < 1e-6);
        assert!((Combine::Overlay.apply(0.4995, 1.998) - 1.996).abs() < 1e-3);
    }
}
