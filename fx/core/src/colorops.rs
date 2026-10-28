//! Small colour operations shared by the time effects (spec/effects/time.md).

/// Luma used by saturation scaling.
pub const SAT_LUMA: [f32; 3] = [0.3125, 0.5, 0.1875];

#[inline]
pub fn sat_luma(c: [f32; 3]) -> f32 {
    c[0] * SAT_LUMA[0] + c[1] * SAT_LUMA[1] + c[2] * SAT_LUMA[2]
}

/// Saturation × `s` around the luma (s = 0: grey).
#[inline]
pub fn sat_scale(c: [f32; 3], s: f32) -> [f32; 3] {
    let l = sat_luma(c);
    if s == 0.0 {
        return [l; 3];
    }
    let k = l * (1.0 - s);
    [c[0] * s + k, c[1] * s + k, c[2] * s + k]
}

/// RGB -> (hue in turns, saturation, value). Colours dominated by negative
/// values are treated as their negation (value stays negative).
pub fn rgb_to_hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let (mut r, mut g, mut b) = (c[0], c[1], c[2]);
    let mn = r.min(g).min(b);
    let mut mx = r.max(g).max(b);
    let mut v = mx;
    if mx < -mn {
        v = mn;
        r = -r;
        g = -g;
        b = -b;
        mx = -mn;
    }
    let mn2 = r.min(g).min(b);
    let s = if mx != 0.0 { (mx - mn2) / mx } else { 0.0 };
    if s == 0.0 {
        return (0.0, s, v);
    }
    let d = mx - mn2;
    let h = if r == mx {
        let h = (g - b) / d;
        if h < 0.0 {
            h + 6.0
        } else {
            h
        }
    } else if g == mx {
        2.0 + (b - r) / d
    } else {
        4.0 + (r - g) / d
    };
    (h * (1.0 / 6.0), s, v)
}

#[inline]
fn frac(x: f32) -> f32 {
    x - x.floor()
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    if s == 0.0 {
        return [v; 3];
    }
    let (h, s) = if s < 0.0 {
        (frac(h + 0.5), -s)
    } else {
        (frac(h), s)
    };
    let mut h6 = h * 6.0;
    if h6 >= 6.0 {
        h6 = 0.0;
    }
    let i = h6 as i32;
    let f = h6 - i as f32;
    let vs = v * s;
    let p = v - vs;
    let q = v - vs * f;
    let t = v - vs * (1.0 - f);
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// Hue rotated by `turns`.
pub fn hue_shift(c: [f32; 3], turns: f32) -> [f32; 3] {
    let (h, s, v) = rgb_to_hsv(c);
    hsv_to_rgb(h + turns, s, v)
}
