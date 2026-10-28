use super::{Context, Settings};
use crate::{colorops::sat_scale, Image};
use rayon::prelude::*;

pub fn render(src: &Image, p: &Settings, ctx: &Context) -> Image {
    let radius = p.f("Blur Input");
    let mut out = crate::batch4::blur(src, [radius * ctx.scale[0], radius * ctx.scale[1]], 3);
    let color = p.c("Threshold Add Color");
    let threshold = color.map(|c| c + p.f("Threshold"));
    let gain =
        p.f("Brightness") / (1.0 - threshold[0].min(threshold[1]).min(threshold[2])).max(0.01);
    out.data.par_iter_mut().enumerate().for_each(|(i, v)| {
        let excess = (v[0] - threshold[0]).max(0.0)
            + (v[1] - threshold[1]).max(0.0)
            + (v[2] - threshold[2]).max(0.0);
        let sum = v[0].max(0.0) + v[1].max(0.0) + v[2].max(0.0);
        let k = if sum > 0.0 { excess / sum } else { 0.0 };
        let rgb = sat_scale(
            [v[0] * k * gain, v[1] * k * gain, v[2] * k * gain],
            p.f("Saturation"),
        );
        *v = [rgb[0], rgb[1], rgb[2], src.data[i][3]];
    });
    out
}
