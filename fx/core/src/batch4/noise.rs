//! Own simplex and multiscale block-noise implementation from native math.
#[path = "noise_constants.rs"]
mod constants;
use constants::{GRAD2, GRAD3, PERM};
fn hash2(x: i32, y: i32, seed: i32) -> usize {
    let s = (((seed >> 8) ^ seed) & 255) as usize;
    PERM[((x & 255) as usize + PERM[((y & 255) as usize + s) & 255] + s) & 255]
}
fn hash3(x: i32, y: i32, z: i32, seed: i32) -> usize {
    let s = (((seed >> 8) ^ seed) & 255) as usize;
    PERM[((x & 255) as usize
        + PERM[((y & 255) as usize + PERM[((z & 255) as usize + s) & 255] + s) & 255]
        + s)
        & 255]
}
pub fn simplex2(mut x: f32, mut y: f32, seed: i32) -> f32 {
    x += 0.77 * ((seed >> 5) & 31) as f32;
    y += 0.37 * (seed & 31) as f32;
    let s = (x + y) * 0.3660254;
    let i = (x + s).floor() as i32;
    let j = (y + s).floor() as i32;
    let t = (i + j) as f32 * 0.21132487;
    let x = x - (i as f32 - t);
    let y = y - (j as f32 - t);
    let i1 = if x > y { 1 } else { 0 };
    let j1 = 1 - i1;
    let mut result = 0.0;
    for (di, dj, px, py) in [
        (0, 0, x, y),
        (
            i1,
            j1,
            x - i1 as f32 + 0.21132487,
            y - j1 as f32 + 0.21132487,
        ),
        (1, 1, x - 1.0 + 2.0 * 0.21132487, y - 1.0 + 2.0 * 0.21132487),
    ] {
        let t = (0.5 - px * px - py * py).max(0.0);
        let g = GRAD2[hash2(i + di, j + dj, seed)];
        result += t * t * t * t * (g[0] * px + g[1] * py);
    }
    100.0 * result
}
pub fn simplex3(mut x: f32, mut y: f32, z: f32, seed: i32, radius: f32, norm: f32) -> f32 {
    x += 0.77 * ((seed >> 5) & 31) as f32;
    y += 0.37 * (seed & 31) as f32;
    let s = (x + y + z) * (1.0 / 3.0);
    let i = (x + s).floor() as i32;
    let j = (y + s).floor() as i32;
    let k = (z + s).floor() as i32;
    let t = (i + j + k) as f32 * (1.0 / 6.0);
    let v = [x - (i as f32 - t), y - (j as f32 - t), z - (k as f32 - t)];
    let (a, b) = if v[0] >= v[1] {
        if v[1] >= v[2] {
            ([1, 0, 0], [1, 1, 0])
        } else if v[0] >= v[2] {
            ([1, 0, 0], [1, 0, 1])
        } else {
            ([0, 0, 1], [1, 0, 1])
        }
    } else if v[1] < v[2] {
        ([0, 0, 1], [0, 1, 1])
    } else if v[0] < v[2] {
        ([0, 1, 0], [0, 1, 1])
    } else {
        ([0, 1, 0], [1, 1, 0])
    };
    let mut sum = 0.0;
    for (level, off) in [[0, 0, 0], a, b, [1, 1, 1]].into_iter().enumerate() {
        let d: [f32; 3] =
            std::array::from_fn(|c| v[c] - off[c] as f32 + level as f32 * (1.0 / 6.0));
        let t = (radius - d[0] * d[0] - d[1] * d[1] - d[2] * d[2]).max(0.0);
        let g = GRAD3[hash3(i + off[0], j + off[1], k + off[2], seed)];
        sum += t * t * t * t * (g[0] * d[0] + g[1] * d[1] + g[2] * d[2]);
    }
    norm * sum
}
pub fn fractal3(x: f32, y: f32, z: f32, seed: i32, n: usize) -> f32 {
    // Native weights are stored at four decimal places.
    let norms = [
        0.0, 1.0, 0.6667, 0.5714, 0.5333, 0.5161, 0.5079, 0.5039, 0.5019,
    ];
    let n = n.clamp(1, 8);
    let mut amp = norms[n];
    let mut freq = 1.0;
    let mut out = 0.0;
    for _ in 0..n {
        out += amp * simplex3(x * freq, y * freq, z * freq, seed, 0.4, 296.82397);
        amp *= 0.5;
        freq *= 2.0;
    }
    out
}
#[derive(Clone, Copy)]
pub struct Blocks {
    pub grid: [f32; 2],
    pub freq: [f32; 3],
    pub time: f32,
    pub drift: f32,
    pub drift_speed: f32,
    pub layers: usize,
    pub decay: f32,
    pub grid_scale: f32,
    pub noise_scale: f32,
    pub phase: [f32; 2],
    pub seed: i32,
}
impl Blocks {
    pub fn at(self, x: f32, y: f32) -> f32 {
        let mut grid = self.grid;
        let mut freq = self.freq;
        let mut amp = 1.0;
        let mut out = 0.0;
        for layer in 0..self.layers.min(16) {
            let dx = simplex2(layer as f32, self.time * self.drift_speed, self.seed);
            let dy = simplex2(
                layer as f32,
                self.time * self.drift_speed,
                self.seed.wrapping_add(1),
            );
            let px = ((x - self.phase[0]) - self.drift * dx / grid[0]) * grid[0];
            let py = ((y - self.phase[1]) - self.drift * dy / grid[1]) * grid[1];
            let n = fractal3(
                px.floor() * freq[0] / grid[0],
                py.floor() * freq[1] / grid[1],
                self.time * freq[2],
                self.seed.wrapping_add(123 + layer as i32),
                if layer < 4 { 3 } else { 2 },
            );
            out += n * amp;
            grid = grid.map(|v| v / self.grid_scale);
            freq[0] *= self.noise_scale;
            freq[1] *= self.noise_scale;
            amp *= self.decay;
            if amp < 0.001 {
                break;
            }
        }
        out
    }
}
pub fn atmosphere(x: f32, y: f32, t: f32, seed: f32, detail: f32) -> f32 {
    let mut sum = 0.0;
    let mut weight = 1.0;
    let mut total = 0.0;
    let mut freq = 1.0;
    for _ in 0..8 {
        sum += weight
            * simplex3(
                x * freq,
                y * freq,
                t * freq,
                (seed * 4096.0) as i32,
                0.6,
                32.0,
            );
        total += weight;
        weight *= detail;
        freq *= 2.0;
    }
    if total > 0.0 {
        sum / total
    } else {
        0.0
    }
}
