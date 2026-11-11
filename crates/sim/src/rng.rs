//! Integer-only hashing and value noise: bit-exact on every platform (lockstep-safe).

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn hash2(seed: u64, x: i32, y: i32) -> u64 {
    splitmix(seed ^ (x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87) ^ (y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
}

/// Value noise in 0..=255 with lattice spacing `cell` tiles.
pub fn value_noise(seed: u64, x: i32, y: i32, cell: i32) -> i64 {
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let fx = (x.rem_euclid(cell) as i64) * 1024 / cell as i64;
    let fy = (y.rem_euclid(cell) as i64) * 1024 / cell as i64;
    let smooth = |t: i64| t * t * (3 * 1024 - 2 * t) / (1024 * 1024);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let c = |ix: i32, iy: i32| (hash2(seed, ix, iy) & 0xFF) as i64;
    let lerp = |a: i64, b: i64, t: i64| a + (b - a) * t / 1024;
    let top = lerp(c(cx, cy), c(cx + 1, cy), sx);
    let bot = lerp(c(cx, cy + 1), c(cx + 1, cy + 1), sx);
    lerp(top, bot, sy)
}

/// Three-octave fractal noise in 0..=255.
pub fn fbm(seed: u64, x: i32, y: i32) -> i64 {
    (value_noise(seed, x, y, 64) * 4
        + value_noise(seed ^ 0xA5A5, x, y, 32) * 2
        + value_noise(seed ^ 0x5A5A, x, y, 16))
        / 7
}
