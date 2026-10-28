//! Transitions recovered from the effect graph. spec/effects/dissolve-blur.md.
use crate::boxes::box_passes;
use crate::glow::AlphaMode;
use crate::{Edge, Image};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct DissolveBlurParams {
    pub amount: f32,
    pub on_from_bg: bool,
    pub width: f32,
    pub rel_xy: [f32; 2],
    pub rel_from: f32,
    pub rel_to: f32,
    pub passes: usize,
    pub alpha_mode: AlphaMode,
}

fn blurred(
    src: Option<&Image>,
    w: usize,
    h: usize,
    radius: [f32; 2],
    p: &DissolveBlurParams,
) -> Image {
    let Some(src) = src else {
        return Image::new(w, h);
    };
    let mut out = src.clone();
    match p.alpha_mode {
        AlphaMode::Normal => out.premultiply(),
        AlphaMode::AllOpaque => out.data.par_iter_mut().for_each(|v| v[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    for c in 0..4 {
        let mut pl = out.channel(c);
        box_passes(&mut pl, radius[0], radius[1], Edge::Reflect, p.passes);
        out.set_channel(c, &pl);
    }
    out
}

pub fn dissolve_blur(src: &Image, bg: Option<&Image>, p: &DissolveBlurParams) -> Image {
    let t = p.amount.clamp(0.0, 1.0);
    let (from, to) = if p.on_from_bg {
        (bg, Some(src))
    } else {
        (Some(src), bg)
    };
    if t == 0.0 || t == 1.0 {
        let chosen = if t == 0.0 { from } else { to };
        let mut out = chosen.cloned().unwrap_or_else(|| Image::new(src.w, src.h));
        if chosen.is_some() && p.alpha_mode == AlphaMode::AllOpaque {
            out.data.par_iter_mut().for_each(|v| v[3] = 1.0);
        }
        return out;
    }
    let radius = |v: f32, rel: f32| {
        p.rel_xy
            .map(|r| p.width.max(0.0) * v * rel.max(0.0) * r.max(0.0))
    };
    let mut out = blurred(from, src.w, src.h, radius(t, p.rel_from), p);
    let to = blurred(to, src.w, src.h, radius(1.0 - t, p.rel_to), p);
    out.data.par_iter_mut().zip(&to.data).for_each(|(a, b)| {
        for c in 0..4 {
            a[c] = crate::stylize::dissolve(a[c], b[c], t);
        }
    });
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_direction_preserve_inputs() {
        let a = Image::filled(32, 16, [0.8, 0.2, 0.1, 0.5]);
        let b = Image::filled(32, 16, [0.1, 0.3, 0.9, 0.75]);
        let mut p = DissolveBlurParams {
            amount: 0.0,
            on_from_bg: false,
            width: 40.0,
            rel_xy: [1.0, 1.0],
            rel_from: 1.0,
            rel_to: 1.0,
            passes: 3,
            alpha_mode: AlphaMode::Normal,
        };
        assert_eq!(dissolve_blur(&a, Some(&b), &p).data, a.data);
        p.amount = 1.0;
        assert_eq!(dissolve_blur(&a, Some(&b), &p).data, b.data);
        p.on_from_bg = true;
        assert_eq!(dissolve_blur(&a, Some(&b), &p).data, a.data);
        p.on_from_bg = false;
        assert!(dissolve_blur(&a, None, &p)
            .data
            .iter()
            .all(|v| *v == [0.0; 4]));
    }
}
