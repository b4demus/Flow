//! AA&N forward / LL&M inverse, adapted from IJG 6b (jfdctfst/jidctint).
//! Copyright (C) 1991-1998, Thomas G. Lane. IJG terms in third_party/ijg/README.
//! This Rust adaptation shares the fixed-point rounding of the JPEG codec.

fn descale(x: i64, bits: u32) -> i64 {
    (x + (1 << (bits - 1))) >> bits
}

fn forward(a: [i64; 8]) -> [i64; 8] {
    let t0 = a[0] + a[7];
    let t7 = a[0] - a[7];
    let t1 = a[1] + a[6];
    let t6 = a[1] - a[6];
    let t2 = a[2] + a[5];
    let t5 = a[2] - a[5];
    let t3 = a[3] + a[4];
    let t4 = a[3] - a[4];
    let e0 = t0 + t3;
    let e3 = t0 - t3;
    let e1 = t1 + t2;
    let e2 = t1 - t2;
    let z1 = ((e2 + e3) * 181) >> 8;
    let o0 = t4 + t5;
    let o1 = t5 + t6;
    let o2 = t6 + t7;
    let z5 = ((o0 - o2) * 98) >> 8;
    let z2 = ((o0 * 139) >> 8) + z5;
    let z4 = ((o2 * 334) >> 8) + z5;
    let z3 = (o1 * 181) >> 8;
    let z11 = t7 + z3;
    let z13 = t7 - z3;
    [
        e0 + e1,
        z11 + z4,
        e3 + z1,
        z13 - z2,
        e0 - e1,
        z13 + z2,
        e3 - z1,
        z11 - z4,
    ]
}

const AAN: [i64; 64] = [
    16384, 22725, 21407, 19266, 16384, 12873, 8867, 4520, 22725, 31521, 29692, 26722, 22725, 17855,
    12299, 6270, 21407, 29692, 27969, 25172, 21407, 16819, 11585, 5906, 19266, 26722, 25172, 22654,
    19266, 15137, 10426, 5315, 16384, 22725, 21407, 19266, 16384, 12873, 8867, 4520, 12873, 17855,
    16819, 15137, 12873, 10114, 6967, 3552, 8867, 12299, 11585, 10426, 8867, 6967, 4799, 2446,
    4520, 6270, 5906, 5315, 4520, 3552, 2446, 1247,
];

pub(super) fn fdct(input: &[u8; 64], quant: &[i32; 64]) -> [i32; 64] {
    let mut temp = [0i64; 64];
    for y in 0..8 {
        let row = forward(std::array::from_fn(|x| input[y * 8 + x] as i64 - 128));
        temp[y * 8..y * 8 + 8].copy_from_slice(&row);
    }
    let mut out = [0; 64];
    for x in 0..8 {
        let col = forward(std::array::from_fn(|y| temp[y * 8 + x]));
        for y in 0..8 {
            let i = y * 8 + x;
            let divisor = descale(quant[i] as i64 * AAN[i], 11);
            let value = col[y];
            out[i] = ((value.abs() + divisor / 2) / divisor * value.signum()) as i32;
        }
    }
    out
}

fn inverse(a: [i64; 8], shift: u32) -> [i64; 8] {
    let z1 = (a[2] + a[6]) * 4433;
    let t2 = z1 - a[6] * 15137;
    let t3 = z1 + a[2] * 6270;
    let t0 = (a[0] + a[4]) << 13;
    let t1 = (a[0] - a[4]) << 13;
    let e0 = t0 + t3;
    let e3 = t0 - t3;
    let e1 = t1 + t2;
    let e2 = t1 - t2;
    let z5 = (a[7] + a[3] + a[5] + a[1]) * 9633;
    let z1 = (a[7] + a[1]) * -7373;
    let z2 = (a[5] + a[3]) * -20995;
    let z3 = (a[7] + a[3]) * -16069 + z5;
    let z4 = (a[5] + a[1]) * -3196 + z5;
    let o0 = a[7] * 2446 + z1 + z3;
    let o1 = a[5] * 16819 + z2 + z4;
    let o2 = a[3] * 25172 + z2 + z3;
    let o3 = a[1] * 12299 + z1 + z4;
    [
        e0 + o3,
        e1 + o2,
        e2 + o1,
        e3 + o0,
        e3 - o0,
        e2 - o1,
        e1 - o2,
        e0 - o3,
    ]
    .map(|v| descale(v, shift))
}

/// Input is already dequantized and may contain corruption outside i16.
pub(super) fn idct(input: &[i32; 64]) -> [u8; 64] {
    let mut temp = [0i64; 64];
    for x in 0..8 {
        let col = inverse(std::array::from_fn(|y| input[y * 8 + x] as i64), 11);
        for y in 0..8 {
            temp[y * 8 + x] = col[y];
        }
    }
    let mut out = [0; 64];
    for y in 0..8 {
        let row = inverse(std::array::from_fn(|x| temp[y * 8 + x]), 18);
        for x in 0..8 {
            // JPEG's masked range-limit table, including wrap for extreme data.
            let index = (row[x] & 1023) as i32;
            let signed = if index >= 512 { index - 1024 } else { index };
            out[y * 8 + x] = (signed + 128).clamp(0, 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_blocks_have_expected_dc_and_round_trip() {
        for value in [0, 1, 57, 128, 219, 255] {
            let coef = fdct(&[value; 64], &[1; 64]);
            assert_eq!(coef[0], (value as i32 - 128) * 8);
            assert!(coef[1..].iter().all(|&v| v == 0));
            assert_eq!(idct(&coef), [value; 64]);
        }
    }
}
