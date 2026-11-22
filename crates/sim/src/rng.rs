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

/// Gradient (Perlin-style) noise in 0..=255 with lattice spacing `cell` tiles. Unlike value noise it has no bumps on
/// the lattice points, so it shows no grid lines. Integer-only: the quintic fade and the gradient dots are fixed point.
pub fn gradient_noise(seed: u64, x: i32, y: i32, cell: i32) -> i64 {
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let fx = (x.rem_euclid(cell) as i64) * 1024 / cell as i64;
    let fy = (y.rem_euclid(cell) as i64) * 1024 / cell as i64;
    // 6t^5 - 15t^4 + 10t^3 in 0..=1024
    let fade = |t: i64| {
        let t2 = t * t / 1024;
        let t3 = t2 * t / 1024;
        t3 * ((t * (t * 6 - 15 * 1024) / 1024) + 10 * 1024) / 1024
    };
    // eight directions of length 1024
    const G: [(i64, i64); 8] = [(1024, 0), (-1024, 0), (0, 1024), (0, -1024), (724, 724), (-724, 724), (724, -724), (-724, -724)];
    let dot = |ix: i32, iy: i32, dx: i64, dy: i64| {
        let (gx, gy) = G[(hash2(seed, ix, iy) & 7) as usize];
        (gx * dx + gy * dy) / 1024
    };
    let (sx, sy) = (fade(fx), fade(fy));
    let lerp = |a: i64, b: i64, t: i64| a + (b - a) * t / 1024;
    let top = lerp(dot(cx, cy, fx, fy), dot(cx + 1, cy, fx - 1024, fy), sx);
    let bot = lerp(dot(cx, cy + 1, fx, fy - 1024), dot(cx + 1, cy + 1, fx - 1024, fy - 1024), sx);
    let v = lerp(top, bot, sy);
    (128 + v * 190 / 1024).clamp(0, 255)
}

/// Signed value noise in -1024..=1023 with independent lattice spacing on each axis.
pub(crate) fn value_noise_3d(seed: u64, point: [i64; 3], spacing: [i64; 3]) -> i32 {
    let axis = |p: i64, step: i64| {
        let fraction = p.rem_euclid(step) * 1024 / step;
        let weight = fraction * fraction * (3072 - 2 * fraction) / (1024 * 1024);
        (p.div_euclid(step), weight as i32)
    };
    let (x, sx) = axis(point[0], spacing[0]);
    let (y, sy) = axis(point[1], spacing[1]);
    let (z, sz) = axis(point[2], spacing[2]);
    let lerp = |a: i32, b: i32, t: i32| a + (((b - a) * t) >> 10);
    let plane = |z: i64| {
        let layer = seed ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        let corner = |dx: i64, dy: i64| {
            (hash2(layer, (x + dx) as i32, (y + dy) as i32) & 2047) as i32 - 1024
        };
        let north = lerp(corner(0, 0), corner(1, 0), sx);
        let south = lerp(corner(0, 1), corner(1, 1), sx);
        lerp(north, south, sy)
    };
    lerp(plane(z), plane(z + 1), sz)
}

/// Three-octave fractal noise in 0..=255.
pub fn fbm(seed: u64, x: i32, y: i32) -> i64 {
    (value_noise(seed, x, y, 64) * 4
        + value_noise(seed ^ 0xA5A5, x, y, 32) * 2
        + value_noise(seed ^ 0x5A5A, x, y, 16))
        / 7
}
