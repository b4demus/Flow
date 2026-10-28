//! Shake: random/periodic camera shake of the whole frame (or per RGB channel).
//! The maths and its constants are described in spec/effects/shake.md; float
//! vs double steps follow the original so the offsets match to the last bit.

use crate::image::Image;
use rayon::prelude::*;
use std::sync::OnceLock;

// ───────────────────────────── tables ─────────────────────────────

struct Tables {
    /// 256 hash words.
    t: [u32; 256],
    /// Radial kernel indexed by r²·2048, quantised to 1/4096.
    kernel: Vec<f32>,
    /// r² at and beyond which the kernel is zero.
    kernel_r2: f32,
    /// Jump segments: (length, start, hash), lengths sum to 1024.
    seg: Vec<(f32, f32, u32)>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        // Hash words from a small LCG started at 0.
        let mut s: u32 = 0;
        let step = |s: &mut u32| {
            *s = (*s * 0x1000 + 0x24d69) % 0xae529;
            *s
        };
        let mut t = [0u32; 256];
        for w in t.iter_mut() {
            let a = step(&mut s);
            let b = step(&mut s);
            let c = step(&mut s);
            *w = c
                .wrapping_mul(0x40000)
                .wrapping_add(b * 0x100)
                .wrapping_add(a >> 2);
        }

        let mut kernel = vec![0f32; 0x1200];
        let mut zero_at = None;
        for (i, k) in kernel.iter_mut().enumerate() {
            let x = (i as f32 / 4608.0).sqrt();
            let v = if 0.5 <= x as f64 {
                let m = (1.0 - x as f64) as f32;
                ((m * m) as f64 * 2.0) as f32
            } else {
                (1.0 - (x * x) as f64 * 2.0) as f32
            };
            let q = ((v * 4096.0) as f64 + 0.5f64.copysign(v as f64)) as i32;
            *k = q as f32 * (1.0 / 4096.0);
            if zero_at.is_none() && q == 0 {
                zero_at = Some((i as i32) << 1);
            }
        }
        let kernel_r2 = zero_at.unwrap_or(0x2400) as f32 * (1.0 / 4096.0);

        let mut tb = Tables {
            t,
            kernel,
            kernel_r2,
            seg: Vec::new(),
        };
        let mut seg = vec![(0f32, 0f32, 0u32); 0x400];
        let mut r = tb.h4(0x1b9e);
        let mut sum = 0f32;
        for e in seg.iter_mut() {
            r = tb.h4(r.wrapping_add(0x1b9e));
            let f = unit(r) as f32;
            e.0 = f * f;
            e.2 = r;
            sum += e.0;
        }
        let norm = sum * (1.0 / 1024.0);
        let mut start = 0f32;
        for e in seg.iter_mut() {
            e.1 = start;
            e.0 /= norm;
            start += e.0;
        }
        tb.seg = seg;
        tb
    })
}

/// The original's 4-round table hash (also behind `random` in the time effects).
pub fn hash4(u: u32) -> u32 {
    tables().h4(u)
}

/// Native temporal value noise with linear interpolation (film transport and
/// scratch lifetimes/roughness). This is distinct from the spatial noise2.
pub(crate) fn linear_noise1(seed: u32, x: f32, octaves: usize) -> f32 {
    let norms = [
        0.0, 1.0, 0.6667, 0.5714, 0.5333, 0.5161, 0.5079, 0.5039, 0.5019, 0.501, 0.5005,
    ];
    let t = tables();
    let (mut sum, mut frequency, mut amplitude) = (0.0f32, 1.0f32, 1.0f32);
    for octave in 0..octaves.min(10) as u32 {
        let phase = (unit(t.h4(seed.wrapping_add(octave + 701))) * 0.5 - 0.25) as f32;
        let at = x * frequency + phase;
        let i = at.floor() as i32;
        let fraction = at - i as f32;
        let a = unit(t.h8(seed.wrapping_add(i as u32), octave)) as f32;
        let b = unit(t.h8(seed.wrapping_add(i.wrapping_add(1) as u32), octave)) as f32;
        sum = (sum as f64
            + (amplitude + amplitude) as f64 * ((a + fraction * (b - a)) as f64 - 0.5))
            as f32;
        frequency += frequency;
        amplitude *= 0.5;
    }
    sum * norms[octaves.min(10)]
}

/// Quadratic B-spline temporal noise used by the scratch centre line.
pub(crate) fn quadratic_noise1(seed: u32, x: f32, octaves: usize) -> f32 {
    let norms = [
        0.0, 1.0, 0.6667, 0.5714, 0.5333, 0.5161, 0.5079, 0.5039, 0.5019, 0.501, 0.5005,
    ];
    let (mut sum, mut frequency, mut amplitude) = (0.0f32, 1.0f32, 1.0f32);
    for octave in 0..octaves.min(10) as u32 {
        let phase = (unit(hash4(seed.wrapping_add(octave + 701))) * 0.5 - 0.25) as f32;
        let at = x * frequency + phase;
        let i = ((at as f64 + 0.5) as f32).floor() as i32;
        let u = ((at - i as f32) as f64 + 0.5) as f32;
        let a = unit(hash4(seed.wrapping_add(i.wrapping_sub(1) as u32))) as f32;
        let b = unit(hash4(seed.wrapping_add(i as u32))) as f32;
        let c = unit(hash4(seed.wrapping_add(i.wrapping_add(1) as u32))) as f32;
        let v = a * 0.5 * (1.0 - u) * (1.0 - u)
            + (b as f64 * ((-u * u + u) as f64 + 0.5)) as f32
            + c * 0.5 * u * u;
        sum += amplitude * ((v + v) - 1.0);
        frequency += frequency;
        amplitude *= 0.5;
    }
    sum * norms[octaves.min(10)]
}

/// Smooth value noise (0..1) at (x, y) with `octaves` octaves; the original's
/// `warp_noise` (Flicker, Feedback Bubble) on one channel.
pub fn noise2(x: f32, y: f32, octaves: u32, seed: i32) -> f32 {
    tables().noise_oct(x, y, octaves, seed, 0)
}

/// Three-channel version (`warp_noise3`).
pub fn noise2_rgb(x: f32, y: f32, octaves: u32, seed: i32) -> [f32; 3] {
    tables().noise3(x, y, octaves, seed)
}

/// The recovered variable-duration random sawtooth used by damage blink gates.
pub fn random_sawtooth(
    seed: f32,
    x: f32,
    low: f32,
    high: f32,
    slope_low: f32,
    slope_high: f32,
    dwell: f32,
) -> f32 {
    tables().jumps(
        (seed * 4096.0) as i32,
        x,
        low,
        high,
        slope_low,
        slope_high,
        dwell,
    )
}

/// Projector weave uses a separate linear-octave noise and an asymmetric
/// sawtooth slope cutoff, unlike the Shake effect's centered slope deadzone.
pub fn projector_signal(
    seed: f32,
    time: f32,
    frequency: f32,
    random_amp: f32,
    jump_amp: f32,
    keep: f32,
) -> [f32; 2] {
    let seed = (seed * 4096.0) as i32;
    let t = tables();
    let offset = (unit(t.h4((seed as u32).wrapping_add(0x3fd))) * 100.0) as f32;
    let saw = |x: f32| {
        let p = x + offset;
        let k = (p / 1024.0).floor() as i32;
        let base = (k.wrapping_mul(1024)) as f32;
        let q = p - base;
        let index = t
            .seg
            .partition_point(|(_, start, _)| *start <= q)
            .saturating_sub(1)
            .min(1023);
        let (_, start, h) = t.seg[index];
        let h = t.h8(seed as u32, (k as u32).wrapping_add(h));
        let level = (unit(h) * 2.0 - 1.0) as f32;
        let slope = ((unit(t.h4(h)) as f32 - 0.15).max(0.0) * 2.0) - 1.0;
        level + (p - (start + base)) * slope
    };
    let signal = |x: f32| {
        let v = jump_amp * saw(x * frequency)
            + random_amp * linear_noise1(seed as u32, x * frequency * 5.0, 6);
        if v.abs() <= keep {
            0.0
        } else {
            v - v.signum() * keep
        }
    };
    [signal(time + offset), signal(time + offset - 1.0)]
}

/// Low 30 bits of a hash as a fraction in 0..1 (double, as in the original).
#[inline]
fn unit(h: u32) -> f64 {
    (h & 0x3fff_ffff) as f64 * (1.0 / 1073741824.0)
}

impl Tables {
    #[inline]
    fn round(&self, u: u32) -> u32 {
        (u >> 8).wrapping_add(self.t[(u & 0xff) as usize])
    }

    fn h4(&self, u: u32) -> u32 {
        let mut u = u;
        for _ in 0..4 {
            u = self.round(u);
        }
        u
    }

    fn h8(&self, u: u32, add: u32) -> u32 {
        let mut u = u;
        for _ in 0..3 {
            u = self.round(u);
        }
        u = self.round(u).wrapping_add(add);
        for _ in 0..4 {
            u = self.round(u);
        }
        u
    }

    /// Smooth value noise at (x, 0), one octave, result in 0..1.
    fn noise(&self, x: f32, seed: i32) -> f32 {
        let y = 0f32;
        let cx = x.round() as i32;
        let cy = y.round() as i32;
        let fx = x - cx as f32;
        let fy = y - cy as f32;
        let sq = |f: f32| {
            let s = f * f;
            [s + f + f + 1.0, s, (s - (f + f)) + 1.0]
        };
        let (ax, ay) = (sq(fx), sq(fy));
        let mut acc = 0f32;
        let mut wsum = 0f32;
        for (j, &dy) in ay.iter().enumerate() {
            let gy = cy - 1 + j as i32;
            for (i, &dx) in ax.iter().enumerate() {
                let r2 = dx + dy;
                if r2 < self.kernel_r2 {
                    let w = self.kernel[((r2 * 4096.0) as i32 >> 1) as usize];
                    let gx = cx - 1 + i as i32;
                    let h = self.h8(((gy << 4) as u32) ^ (seed as u32), gx as u32);
                    let v = ((h & 0x1fff) as i32 - 0x1000) as f32 * (1.0 / 4096.0);
                    acc += v * w;
                    wsum += w;
                }
            }
        }
        let out = acc * (1.0 / wsum);
        (out as f64 * 0.5 + 0.5) as f32
    }

    /// Smooth value noise at (x, y), `octaves` octaves (coordinates ×2 and
    /// weight ÷2 per octave), all three channels (0..1); channel 0 is the
    /// one-channel noise.
    fn noise3(&self, x: f32, y: f32, octaves: u32, seed: i32) -> [f32; 3] {
        let mut out = [0f32; 3];
        let last = octaves.max(1) - 1;
        for o in 0..=last {
            let sc = (1u32 << o) as f32;
            let (xs, ys) = (x * sc, y * sc);
            let cx = xs.round() as i32;
            let cy = ys.round() as i32;
            let fx = xs - cx as f32;
            let fy = ys - cy as f32;
            let sq = |f: f32| {
                let s = f * f;
                [s + f + f + 1.0, s, (s - (f + f)) + 1.0]
            };
            let (ax, ay) = (sq(fx), sq(fy));
            let mut acc = [0f32; 3];
            let mut wsum = 0f32;
            for (j, &dy) in ay.iter().enumerate() {
                let gy = cy - 1 + j as i32;
                for (i, &dx) in ax.iter().enumerate() {
                    let r2 = dx + dy;
                    if r2 < self.kernel_r2 {
                        let w = self.kernel[((r2 * 4096.0) as i32 >> 1) as usize];
                        let gx = cx - 1 + i as i32;
                        let h = self.h8(((gy << 4) as u32) ^ (o ^ seed as u32), gx as u32);
                        let v = [
                            (h & 0x1fff) as i32 - 0x1000,
                            (h >> 16 & 0x1fff) as i32 - 0x1000,
                            ((self.t[(h & 0xff) as usize].wrapping_add(h) >> 8) & 0x1fff) as i32
                                - 0x1000,
                        ];
                        for c in 0..3 {
                            acc[c] += v[c] as f32 * (1.0 / 4096.0) * w;
                        }
                        wsum += w;
                    }
                }
            }
            let mut a = 1.0f32;
            if o > 0 {
                if o == last {
                    a = (octaves.max(1) - last) as f32;
                }
                a /= sc;
            }
            let k = a / wsum;
            for c in 0..3 {
                out[c] += acc[c] * k;
            }
        }
        out.map(|v| (v as f64 * 0.5 + 0.5) as f32)
    }

    fn noise_oct(&self, x: f32, y: f32, octaves: u32, seed: i32, channel: usize) -> f32 {
        self.noise3(x, y, octaves, seed)[channel]
    }

    /// Piecewise-linear jumps: random segment lengths, level in lo..hi, slope
    /// in slo..shi softened by `keep` (|slope| ≤ keep → flat).
    #[allow(clippy::too_many_arguments)]
    fn jumps(&self, seed: i32, x: f32, lo: f32, hi: f32, slo: f32, shi: f32, keep: f32) -> f32 {
        let off = (unit(self.h4((seed as u32).wrapping_add(0x3fd))) * 100.0) as f32;
        let p = x + off;
        let k = (p * (1.0 / 1024.0)).floor() as i32;
        let base = (k.wrapping_mul(0x400)) as f32;
        let q = p - base;
        // Binary search for the segment holding q.
        let (mut a, mut b) = (0usize, 0x400usize);
        let s = loop {
            if b <= a {
                break a;
            }
            let m = (a + b) >> 1;
            let (len, start, _) = self.seg[m];
            if start <= q && q < start + len {
                break m;
            }
            if start < q {
                a = m + 1;
            } else {
                b = m;
            }
        };
        let (_, start, sh) = self.seg[s];
        let h = self.h8(seed as u32, (k as u32).wrapping_add(sh));
        let level = (unit(h) * (hi - lo) as f64 + lo as f64) as f32;
        let h2 = self.h4(h);
        let raw = (unit(h2) * (shi - slo) as f64 + slo as f64) as f32;
        let slope = if raw.abs() <= keep {
            0.0
        } else {
            raw - (raw / raw.abs()) * keep
        };
        level + (p - (start + base)) * slope
    }
}

// ───────────────────────────── signal ─────────────────────────────

/// Random part of one axis: smooth noise, or jumps when `jumpy`.
fn rand(time: f32, amp: f32, freq: f32, seed: f32, twitch: f32, jumpy: bool, drift: f32) -> f32 {
    let t = tables();
    let k = (seed * 4096.0) as i32;
    if jumpy {
        let j = t.jumps(k, time * freq, -twitch, twitch, -1.0, 1.0, 1.0 - drift);
        (0.5 * amp as f64 * j as f64) as f32
    } else {
        let n = t.noise(time * freq * 2.0, k);
        (twitch + twitch) * amp * (n - 0.5)
    }
}

/// Offset of one axis at (already phase-shifted, frequency-scaled) time `time`.
#[allow(clippy::too_many_arguments)]
fn shift(
    seed: f32,
    time: f32,
    rand_amp: f32,
    rand_freq: f32,
    wave_amp: f32,
    wave_freq: f32,
    extra: (f32, f32, f32),
    jumpy: bool,
    drift: f32,
    twitch: f32,
) -> f32 {
    let r1 = rand(time, rand_amp, rand_freq, seed, twitch, jumpy, drift);
    let w = (time * wave_freq * std::f32::consts::TAU).sin();
    // The extra (RGB) term passes its seed where the frequency goes and vice
    // versa; kept that way to match.
    let r2 = rand(time, extra.0, extra.1, extra.2, twitch, jumpy, drift);
    r1 + wave_amp * w + r2
}

/// 1 if the jump signal (0..1, no slope) is above `threshold` at time·freq.
fn twitch(seed: f32, time: f32, freq: f32, threshold: f32) -> f32 {
    let v = tables().jumps((seed * 4096.0) as i32, time * freq, 0.0, 1.0, 0.0, 0.0, 0.0);
    if v > threshold {
        1.0
    } else {
        0.0
    }
}

fn smooth_twitch(seed: f32, time: f32, freq: f32, threshold: f32, fps: f32) -> f32 {
    let dt = 1.0 / fps;
    let v0 = twitch(seed, time - dt, freq, threshold);
    let v1 = twitch(seed, time, freq, threshold);
    let v2 = twitch(seed, time + dt, freq, threshold);
    (v0 + (v1 + v2)) / 3.0
}

// ───────────────────────────── params ─────────────────────────────

/// The original keeps slider values in 16.16 fixed point (rounded); apply the
/// same to AE values so equal settings give equal frames.
#[inline]
pub fn fixed(v: f32) -> f32 {
    ((v as f64 * 65536.0).round() / 65536.0) as f32
}

/// Defaults as the original stores them (0.7 and 0.3 in 16.16).
pub const STILLNESS: f32 = 45875.0 / 65536.0;
pub const DRIFT: f32 = 19660.0 / 65536.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Normal,
    Twitchy,
    Jumpy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    No,
    Tile,
    Reflect,
}

pub use crate::glow::AlphaMode;

#[derive(Clone, Copy, Debug)]
pub struct Axis {
    pub rand_amp: f32,
    pub rand_freq: f32,
    pub wave_amp: f32,
    pub wave_freq: f32,
    pub phase: f32,
}

impl Axis {
    const fn new(rand_amp: f32, wave_amp: f32) -> Self {
        Self {
            rand_amp,
            rand_freq: 1.0,
            wave_amp,
            wave_freq: 0.5,
            phase: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShakeParams {
    pub style: Style,
    pub amplitude: f32,
    pub frequency: f32,
    pub phase: f32,
    pub stillness: f32,
    pub twitch_frequency: f32,
    pub drift: f32,
    pub center_bias: f32,
    pub z_dist: f32,
    pub motion_blur: bool,
    pub mo_blur_length: f32,
    pub seed: f32,
    pub wrap: [Wrap; 2],
    pub x: Axis,
    pub y: Axis,
    pub z: Axis,
    pub tilt: Axis,
    pub rgb_amp: [f32; 3],
    pub rgb_phase: [f32; 3],
    pub rgb_randomness: f32,
    pub rgb_frequency: f32,
    pub alpha_mode: AlphaMode,
}

impl Default for ShakeParams {
    fn default() -> Self {
        Self {
            style: Style::Normal,
            amplitude: 1.0,
            frequency: 8.0,
            phase: 0.0,
            stillness: STILLNESS,
            twitch_frequency: 2.0,
            drift: DRIFT,
            center_bias: 0.0,
            z_dist: 1.0,
            motion_blur: false,
            mo_blur_length: 1.0,
            seed: 0.0,
            wrap: [Wrap::Reflect; 2],
            x: Axis::new(0.2, 0.0),
            y: Axis::new(0.1, 0.0),
            z: Axis::new(0.0, 0.0),
            tilt: Axis::new(0.0, 0.0),
            rgb_amp: [1.0; 3],
            rgb_phase: [0.0; 3],
            rgb_randomness: 0.0,
            rgb_frequency: 2.0,
            alpha_mode: AlphaMode::Normal,
        }
    }
}

/// What a style feeds the shared shake: (frequency, jumpy, drift, twitchy, threshold, twitch freq).
struct Mode {
    freq: f32,
    jumpy: bool,
    drift: f32,
    twitchy: bool,
    threshold: f32,
    twitch_freq: f32,
}

impl ShakeParams {
    fn mode(&self) -> Mode {
        match self.style {
            Style::Normal => Mode {
                freq: self.frequency,
                jumpy: false,
                drift: 0.0,
                twitchy: false,
                threshold: 0.0,
                twitch_freq: 0.0,
            },
            Style::Twitchy => Mode {
                freq: 10.0 * self.frequency,
                jumpy: false,
                drift: 0.0,
                twitchy: true,
                threshold: self.stillness,
                twitch_freq: self.twitch_frequency,
            },
            Style::Jumpy => Mode {
                freq: self.frequency,
                jumpy: true,
                drift: self.drift,
                twitchy: true,
                threshold: self.center_bias,
                twitch_freq: 4.0 * self.frequency,
            },
        }
    }

    /// Whole frame shaken at once (no per-channel differences).
    pub fn uniform(&self) -> bool {
        self.rgb_amp == [1.0; 3] && self.rgb_phase == [0.0; 3] && self.rgb_randomness == 0.0
    }
}

/// x, y, z, tilt offsets at time `t` (seconds). `channel` 0 = whole frame,
/// 1..3 = red/green/blue in the per-channel mode.
pub fn offsets(p: &ShakeParams, t: f32, fps: f32, channel: usize) -> [f32; 4] {
    let m = p.mode();
    let (amp, phase, extra) = if channel == 0 {
        (p.amplitude, p.phase, (0.0, 0.0, 0.0))
    } else {
        let c = channel - 1;
        (
            p.rgb_amp[c] * p.amplitude,
            p.rgb_phase[c] + p.phase,
            (p.rgb_randomness, channel as f32, p.rgb_frequency),
        )
    };
    let tw = if m.twitchy {
        smooth_twitch(p.seed, t, m.twitch_freq, m.threshold, fps)
    } else {
        1.0
    };
    let axes = [(&p.x, 0.1f32), (&p.y, 0.2), (&p.z, 0.3), (&p.tilt, 0.4)];
    let mut out = [0f32; 4];
    for (o, (a, ds)) in out.iter_mut().zip(axes) {
        let time = (t - (a.phase + phase)) * m.freq;
        let ex = (extra.0 * a.rand_amp, extra.1, extra.2);
        *o = shift(
            p.seed + ds,
            time,
            amp * a.rand_amp,
            a.rand_freq,
            amp * a.wave_amp,
            a.wave_freq,
            ex,
            m.jumpy,
            m.drift,
            tw,
        );
    }
    out
}

// ───────────────────────────── geometry ─────────────────────────────

/// The layer frame the normalised coordinates refer to (current resolution)
/// and where the source buffer's (0,0) sits in it.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub w: f32,
    pub h: f32,
    pub src_x: i32,
    pub src_y: i32,
}

/// Normalised point -> frame pixel (pixel centres at integers).
#[inline]
fn to_px(fr: &Frame, x: f32, y: f32) -> (f32, f32) {
    let s = 0.5 * fr.w;
    (
        -0.5 + (x + 1.0) * s,
        (0.5 * fr.h - 0.5 - 0.5 * fr.w) + (-y + 1.0) * s,
    )
}

#[inline]
fn to_norm(fr: &Frame, px: f32, py: f32) -> (f32, f32) {
    let s = 2.0 / fr.w;
    ((px + 0.5) * s - 1.0, (0.5 * fr.h - (py + 0.5)) * s)
}

/// Positive remainder (as the original: fmod, plus |n| when negative).
#[inline]
fn pmod(p: f32, n: f32) -> f32 {
    if n == 0.0 {
        0.0
    } else if p >= 0.0 {
        p % n
    } else {
        p % n + n.abs()
    }
}

#[inline]
pub(crate) fn wrap(p: f32, n: i32, mode: Wrap) -> f32 {
    match mode {
        Wrap::No => p,
        Wrap::Tile => pmod(p, n as f32),
        Wrap::Reflect => {
            let q = (p as i32) / n;
            let odd = if p >= 0.0 { q & 1 != 0 } else { q & 1 == 0 };
            if odd {
                n as f32 - pmod(p, n as f32)
            } else {
                pmod(p, n as f32)
            }
        }
    }
}

/// Bilinear sampling of a premultiplied image at normalised points, with the
/// original's wrap modes and GPU-style 8-bit filter fractions (shared by the
/// warp effects).
pub struct Sampler {
    buf: Bordered,
    wrap: [Wrap; 2],
}

impl Sampler {
    pub fn new(premult: &Image, wrap: [Wrap; 2]) -> Self {
        Self {
            buf: Bordered::new(premult, wrap),
            wrap,
        }
    }

    /// Sample at normalised point (x, y) of frame `fr`.
    #[inline]
    pub fn at(&self, fr: &Frame, x: f32, y: f32) -> [f32; 4] {
        let (sx, sy) = to_px(fr, x, y);
        self.at_px(sx - fr.src_x as f32, sy - fr.src_y as f32)
    }

    /// Sample at buffer pixel (px, py) (pixel centres at integers), wrapping first.
    #[inline]
    pub fn at_px(&self, px: f32, py: f32) -> [f32; 4] {
        let bx = wrap(px, self.buf.w, self.wrap[0]);
        let by = wrap(py, self.buf.h, self.wrap[1]);
        self.buf.sample(bx, by, self.wrap)
    }

    /// Coordinates already wrapped against the full-resolution source.
    pub(crate) fn at_px_wrapped(&self, px: f32, py: f32) -> [f32; 4] {
        self.buf.sample(px, py, self.wrap)
    }

    /// Weighted native blur_warp walk. Start wraps first; the displacement
    /// remains intact across seams. Three weights meet at `center` of the walk.
    pub fn line(
        &self,
        from: [f32; 2],
        to: [f32; 2],
        weights: [f32; 3],
        center: f32,
        subpixel: bool,
    ) -> [f32; 4] {
        let snap = |v: f32| {
            if (v - v.round()).abs() < 0.0001 {
                v.round()
            } else {
                v
            }
        };
        let from = from.map(snap);
        let to = to.map(snap);
        if from == to {
            return self.at_px(from[0], from[1]);
        }
        let start = [
            wrap(from[0], self.buf.w, self.wrap[0]),
            wrap(from[1], self.buf.h, self.wrap[1]),
        ];
        let end = [start[0] + to[0] - from[0], start[1] + to[1] - from[1]];
        let major = (end[1] - start[1]).abs() <= (end[0] - start[0]).abs();
        let (a0, b0, a1, b1, na, nb, wa, wb) = if major {
            (
                start[0],
                start[1],
                end[0],
                end[1],
                self.buf.w,
                self.buf.h,
                self.wrap[0],
                self.wrap[1],
            )
        } else {
            (
                start[1],
                start[0],
                end[1],
                end[0],
                self.buf.h,
                self.buf.w,
                self.wrap[1],
                self.wrap[0],
            )
        };
        let get = |a, b| {
            if major {
                self.buf.px(a, b)
            } else {
                self.buf.px(b, a)
            }
        };
        let (mut ia, ie, mut step, fs, fe, mut b, mut slope);
        if subpixel {
            if a1 <= a0 {
                ia = a0.ceil() as i32;
                ie = a1.floor() as i32;
                step = -1;
                fs = ia as f32 - a0;
                fe = a1 - ie as f32;
            } else {
                ia = a0.floor() as i32;
                ie = a1.ceil() as i32;
                step = 1;
                fs = a0 - ia as f32;
                fe = ie as f32 - a1;
            }
            slope = (b1 - b0) / (a1 - a0).abs();
            b = b0 - fs * slope;
        } else {
            ia = (a0 + 0.5) as i32;
            ie = (a1 + 0.5) as i32;
            step = if ie >= ia { 1 } else { -1 };
            fs = 0.0;
            fe = 0.0;
            let n = ((ie - ia).abs() + 1) as f32;
            slope = (b1 - b0) / n;
            b = b0 + 0.5;
        }
        let n = ((ie - ia).abs() + 1).min(1000);
        let split = center.clamp(1e-6, 1.0);
        let mut exposure = weights[0];
        let mut de = (weights[1] - weights[0]) / (n as f32 * split);
        let mut switched = false;
        let (mut sum, mut total) = ([0.0; 4], 0.0);
        for i in 0..n {
            if !switched && i as f32 >= n as f32 * split {
                de = (weights[2] - weights[1]) / (n as f32 * (1.0 - split));
                exposure = weights[1] + (split - (i / n) as f32) * de;
                switched = true;
            }
            if ia < 0 || ia >= na {
                match wa {
                    Wrap::Reflect if ia < 0 => {
                        ia = 0;
                        step = 1;
                    }
                    Wrap::Reflect => {
                        ia = na - 1;
                        step = -1;
                    }
                    Wrap::Tile if ia < 0 => ia = na - 1,
                    Wrap::Tile => ia = 0,
                    Wrap::No => {}
                }
            }
            if b < 0.0 || b >= nb as f32 {
                match wb {
                    Wrap::Reflect if b < 0.0 => {
                        b = -b;
                        slope = -slope;
                    }
                    Wrap::Reflect => {
                        b = 2.0 * nb as f32 - b - 1.0;
                        slope = -slope;
                    }
                    Wrap::Tile if b < 0.0 => b += nb as f32,
                    Wrap::Tile => b -= nb as f32,
                    Wrap::No => {}
                }
            }
            let w = exposure
                * if i == 0 {
                    1.0 - fs
                } else if i == n - 1 {
                    1.0 - fe
                } else {
                    1.0
                };
            let a = if wa == Wrap::No { ia.clamp(-1, na) } else { ia };
            let bb = if wb == Wrap::No {
                b.clamp(-1.0, nb as f32)
            } else {
                b
            };
            if subpixel {
                let k = (bb + 1.0).floor() as i32;
                let f = (bb + 1.0) - k as f32;
                let p = get(a, k - 1);
                let q = get(a, k);
                for c in 0..4 {
                    sum[c] += p[c] * (w * (1.0 - f)) + q[c] * (w * f);
                }
            } else {
                let p = get(a, bb as i32);
                for c in 0..4 {
                    sum[c] += p[c] * w;
                }
            }
            total += w;
            ia += step;
            b += slope;
            exposure += de;
        }
        if total.abs() > 1e-10 {
            sum.map(|v| v / total)
        } else {
            [0.0; 4]
        }
    }
}

/// Normalised coordinates of frame pixel (px, py) (pixel centres at integers).
#[inline]
pub fn pixel_to_norm(fr: &Frame, px: f32, py: f32) -> (f32, f32) {
    to_norm(fr, px, py)
}

/// Premultiplied source with a one-pixel border that matches the wrap mode.
struct Bordered {
    w: i32,
    h: i32,
    data: Vec<[f32; 4]>,
}

impl Bordered {
    fn new(src: &Image, mode: [Wrap; 2]) -> Self {
        let (w, h) = (src.w as i32, src.h as i32);
        let bw = (w + 2) as usize;
        let mut data = vec![[0f32; 4]; bw * (h + 2) as usize];
        let pick = |i: i32, n: i32, m: Wrap| -> Option<i32> {
            if (0..n).contains(&i) {
                Some(i)
            } else {
                match m {
                    Wrap::No => None,
                    Wrap::Tile => Some(i.rem_euclid(n)),
                    Wrap::Reflect => Some(if i < 0 { 0 } else { n - 1 }),
                }
            }
        };
        for by in 0..h + 2 {
            for bx in 0..w + 2 {
                if let (Some(x), Some(y)) = (pick(bx - 1, w, mode[0]), pick(by - 1, h, mode[1])) {
                    data[by as usize * bw + bx as usize] = src.data[(y * w + x) as usize];
                }
            }
        }
        Self { w, h, data }
    }

    /// Bilinear sample at buffer pixel (px, py), already wrapped.
    #[inline]
    fn sample(&self, px: f32, py: f32, mode: [Wrap; 2]) -> [f32; 4] {
        let (mut px, mut py) = (px, py);
        if mode[0] == Wrap::No {
            px = px.clamp(-1.0, self.w as f32);
        }
        if mode[1] == Wrap::No {
            py = py.clamp(-1.0, self.h as f32);
        }
        let ix = (px + 1.0) as i32;
        let iy = (py + 1.0) as i32;
        // The original renders on the GPU, whose texture filtering keeps the
        // bilinear fractions in 8 bits: round them the same way.
        let fx = (((px + 1.0) - ix as f32) * 256.0).round() / 256.0;
        let fy = (((py + 1.0) - iy as f32) * 256.0).round() / 256.0;
        let bw = self.w + 2;
        let at = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x >= bw || y >= self.h + 2 {
                [0.0; 4]
            } else {
                self.data[(y * bw + x) as usize]
            }
        };
        let (p00, p10, p01, p11) = (
            at(ix, iy),
            at(ix + 1, iy),
            at(ix, iy + 1),
            at(ix + 1, iy + 1),
        );
        let fxy = fx * fy;
        let (w00, w10, w11, w01) = (((1.0 - fx) - fy) + fxy, fx - fxy, fxy, fy - fxy);
        let mut o = [0f32; 4];
        for c in 0..4 {
            o[c] = w00 * p00[c] + w10 * p10[c] + w11 * p11[c] + w01 * p01[c];
        }
        o
    }
}

impl Bordered {
    #[inline]
    fn px(&self, x: i32, y: i32) -> [f32; 4] {
        let bw = self.w + 2;
        let (bx, by) = (x + 1, y + 1);
        if bx < 0 || by < 0 || bx >= bw || by >= self.h + 2 {
            [0.0; 4]
        } else {
            self.data[(by * bw + bx) as usize]
        }
    }

    /// Motion blur: average along the segment (x0,y0)→(x1,y1) (buffer pixels,
    /// start already wrapped). One sample per pixel step of the longer axis,
    /// linear across it; the end steps weigh by how much of them the segment
    /// covers. Leaving the image wraps the walk (Tile) or turns it back (Reflect).
    fn streak(&self, x0: f32, y0: f32, x1: f32, y1: f32, wrap: [Wrap; 2]) -> [f32; 4] {
        // Major axis = the longer one; `get(major, minor_row)` hides the swap.
        let x_major = (y1 - y0).abs() <= (x1 - x0).abs();
        let (a0, b0, a1, b1, na, nb, wa, wb) = if x_major {
            (x0, y0, x1, y1, self.w, self.h, wrap[0], wrap[1])
        } else {
            (y0, x0, y1, x1, self.h, self.w, wrap[1], wrap[0])
        };
        let get = |a: i32, b: i32| {
            if x_major {
                self.px(a, b)
            } else {
                self.px(b, a)
            }
        };
        let mut da = a1 - a0;
        let (mut ia, fs, ie, fe, mut step);
        if da <= 0.0 {
            let s = a0.ceil();
            ia = s as i32;
            fs = s - a0;
            let e = a1.floor();
            ie = e as i32;
            fe = a1 - e;
            step = -1;
            da = -da;
        } else {
            let s = a0.floor();
            ia = s as i32;
            fs = a0 - s;
            let e = a1.ceil();
            ie = e as i32;
            fe = e - a1;
            step = 1;
        }
        let n = ((ie - ia) * step + 1).clamp(0, 1000);
        let mut slope = (b1 - b0) / da;
        let mut b = b0 - fs * slope;
        let mut acc = [0f32; 4];
        let mut total = 0f32;
        for i in 0..n {
            // Wrap the walk where it leaves the image.
            if ia < 0 || ia >= na {
                match wa {
                    Wrap::Reflect if ia < 0 => {
                        ia = 0;
                        step = 1;
                    }
                    Wrap::Reflect => {
                        ia = na - 1;
                        step = -1;
                    }
                    Wrap::Tile if ia < 0 => ia = na - 1,
                    Wrap::Tile => ia = 0,
                    Wrap::No => {}
                }
            }
            if b < 0.0 || b >= nb as f32 {
                match wb {
                    Wrap::Reflect if b < 0.0 => {
                        b = -b;
                        slope = -slope;
                    }
                    Wrap::Reflect => {
                        b = ((nb as f32 - b) + nb as f32) - 1.0;
                        slope = -slope;
                    }
                    Wrap::Tile if b < 0.0 => b += nb as f32,
                    Wrap::Tile => b -= nb as f32,
                    Wrap::No => {}
                }
            }
            let w = if i == 0 {
                1.0 - fs
            } else if i == n - 1 {
                1.0 - fe
            } else {
                1.0
            };
            let a = if wa == Wrap::No { ia.clamp(-1, na) } else { ia };
            let bb = if wb == Wrap::No {
                b.clamp(-1.0, nb as f32)
            } else {
                b
            };
            let r = (bb + 1.0) as i32;
            let f = (bb + 1.0) - r as f32;
            let (p0, p1) = (get(a, r - 1), get(a, r));
            let (w0, w1) = (w * (1.0 - f), w * f);
            for c in 0..4 {
                acc[c] += p0[c] * w0;
                acc[c] += p1[c] * w1;
            }
            total += w;
            ia += step;
            b += slope;
        }
        let k = 1.0 / total;
        [acc[0] * k, acc[1] * k, acc[2] * k, acc[3] * k]
    }
}

/// Where (normalised) output pixel (px, py) (frame pixels) samples the source for offsets `s`.
#[inline]
fn source_point(fr: &Frame, p: &ShakeParams, s: [f32; 4], px: f32, py: f32) -> (f32, f32) {
    let (x, y) = to_norm(fr, px, py);
    let (x, y) = (x + s[0], y + s[1]);
    let a = s[3] / 360.0 * std::f32::consts::TAU;
    let (sn, cs) = (a.sin(), a.cos());
    let (rx, ry) = (x * cs - y * sn, x * sn + y * cs);
    let z = p.z_dist + s[2];
    (rx * z, ry * z)
}

/// Renders the shaken frame at time `t`. `src` is straight colour placed at
/// (`fr.src_x`, `fr.src_y`) in the frame; the result covers the same pixels.
pub fn render(src: &Image, fr: &Frame, p: &ShakeParams, t: f32, fps: f32) -> Image {
    let mut pre = src.clone();
    match p.alpha_mode {
        AlphaMode::Normal => pre.premultiply(),
        AlphaMode::AllOpaque => pre.data.par_iter_mut().for_each(|px| px[3] = 1.0),
        AlphaMode::AsPremult => {}
    }
    let buf = Bordered::new(&pre, p.wrap);
    let (w, h) = (src.w, src.h);
    let mut out = Image::new(w, h);
    let half = 0.5 * fr.w;
    // `s`: offsets at t; `s2`: at t + 1/fps (motion blur only).
    let look = |s: [f32; 4], s2: Option<[f32; 4]>, x: usize, y: usize| -> [f32; 4] {
        let (fx, fy) = (x as f32 + fr.src_x as f32, y as f32 + fr.src_y as f32);
        let (nx, ny) = source_point(fr, p, s, fx, fy);
        let (sx, sy) = to_px(fr, nx, ny);
        let bx = wrap(sx - fr.src_x as f32, buf.w, p.wrap[0]);
        let by = wrap(sy - fr.src_y as f32, buf.h, p.wrap[1]);
        if let Some(s2) = s2 {
            let (mx, my) = source_point(fr, p, s2, fx, fy);
            // Segment end: start moved by the motion over one frame × length.
            let (dx, dy) = (
                nx + (mx - nx) * p.mo_blur_length - nx,
                ny + (my - ny) * p.mo_blur_length - ny,
            );
            let (ex, ey) = (bx + dx * half, by + -dy * half);
            if ex != bx || ey != by {
                return buf.streak(bx, by, ex, ey, p.wrap);
            }
        }
        buf.sample(bx, by, p.wrap)
    };
    let blur = p.motion_blur;
    if p.uniform() {
        let s = offsets(p, t, fps, 0);
        let s2 = blur.then(|| offsets(p, t + 1.0 / fps, fps, 0));
        out.data
            .par_chunks_mut(w.max(1))
            .enumerate()
            .for_each(|(y, row)| {
                for (x, o) in row.iter_mut().enumerate() {
                    *o = look(s, s2, x, y);
                }
            });
    } else {
        let s = [
            offsets(p, t, fps, 1),
            offsets(p, t, fps, 2),
            offsets(p, t, fps, 3),
        ];
        let s2 = [1, 2, 3].map(|c| blur.then(|| offsets(p, t + 1.0 / fps, fps, c)));
        out.data
            .par_chunks_mut(w.max(1))
            .enumerate()
            .for_each(|(y, row)| {
                for (x, o) in row.iter_mut().enumerate() {
                    let r = look(s[0], s2[0], x, y);
                    let g = look(s[1], s2[1], x, y);
                    let b = look(s[2], s2[2], x, y);
                    *o = [r[0], g[1], b[2], 0.33 * (r[3] + (g[3] + b[3]))];
                }
            });
    }
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    let _ = h;
    out
}

// ───────────────────────────── Dissolve Shake ─────────────────────────────

#[derive(Clone, Debug)]
pub struct DissolveParams {
    /// "Dissolve On from Bg": the background is what shakes away.
    pub on_from_bg: bool,
    /// Amount (0..1): Dissolve Percent, or the share of the layer's time when automatic.
    pub amount: f32,
    pub speed: f32,
}

impl Default for DissolveParams {
    fn default() -> Self {
        Self {
            on_from_bg: false,
            amount: 0.0,
            speed: 3.0,
        }
    }
}

/// Shake defaults of the transition (stronger and blurred).
pub fn dissolve_shake_defaults() -> ShakeParams {
    ShakeParams {
        amplitude: 3.0,
        frequency: 10.0,
        motion_blur: true,
        mo_blur_length: 0.5,
        ..ShakeParams::default()
    }
}

/// Transition: the layer dissolves into `bg` (straight colour, same size, or
/// transparent when absent) while both shake hardest at the midpoint.
/// `p.style` is ignored (always Normal).
pub fn render_dissolve(
    src: &Image,
    bg: Option<&Image>,
    fr: &Frame,
    p: &ShakeParams,
    d: &DissolveParams,
    t: f32,
    fps: f32,
) -> Image {
    let a = d.amount;
    let ramp = if a > 0.5 { 2.0 - 2.0 * a } else { 2.0 * a };
    let k = (0.5 + d.speed * (a - 0.5)).clamp(0.0, 1.0);
    let pre = |px: [f32; 4]| match p.alpha_mode {
        AlphaMode::AsPremult => px,
        AlphaMode::AllOpaque => [px[0], px[1], px[2], 1.0],
        AlphaMode::Normal => [px[0] * px[3], px[1] * px[3], px[2] * px[3], px[3]],
    };
    let mut mix = Image::new(src.w, src.h);
    mix.data.par_iter_mut().enumerate().for_each(|(i, o)| {
        let s = pre(src.data[i]);
        let b = bg.map(|b| pre(b.data[i])).unwrap_or([0.0; 4]);
        let (from, to) = if d.on_from_bg { (b, s) } else { (s, b) };
        for c in 0..4 {
            o[c] = from[c] + (to[c] - from[c]) * k;
        }
    });
    let q = ShakeParams {
        style: Style::Normal,
        amplitude: p.amplitude * ramp,
        alpha_mode: AlphaMode::AsPremult,
        ..p.clone()
    };
    let mut out = render(&mix, fr, &q, t, fps);
    if p.alpha_mode != AlphaMode::AsPremult {
        out.unpremultiply();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_in_range_and_smooth() {
        let t = tables();
        let mut prev = t.noise(0.0, 409);
        for i in 1..2000 {
            let v = t.noise(i as f32 * 0.01, 409);
            assert!((0.0..=1.0).contains(&v));
            assert!((v - prev).abs() < 0.05);
            prev = v;
        }
    }

    #[test]
    fn segments_cover_period() {
        let t = tables();
        let last = t.seg[0x3ff];
        assert!((last.1 + last.0 - 1024.0).abs() < 0.1);
    }
}
