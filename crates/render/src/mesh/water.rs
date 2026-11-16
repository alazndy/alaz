use super::DetMath;
use super::*;

/// The height of the sea's swell at (x, z) metres right now, worked out exactly as the shader draws it (the same six
/// wave trains, the same weights, in double precision), so a swimmer rides the waves that are on screen.
pub(crate) fn swell_height(world: &World, x: f32, z: f32, alpha: f32) -> f32 {
    const DIRS: [f64; 6] = [0.0, 63.0, 118.0, 182.0, 238.0, 301.0];
    const LENS: [f64; 6] = [14.0, 11.0, 8.5, 6.5, 5.3, 3.1];
    const HS: [f64; 6] = [0.17, 0.14, 0.11, 0.08, 0.05, 0.03];
    let (tx, ty) = (x.floor() as i32, z.floor() as i32);
    // how far out from land, in tiles (the shader's shore distance, up to 8)
    let mut depth = 8.0f64;
    'ring: for r in 1..=8i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) == r && world.map.terrain(tx + dx, ty + dy).is_some_and(|t| t != Terrain::Water) {
                    depth = r as f64 - 0.5;
                    break 'ring;
                }
            }
        }
    }
    let smooth = |a: f64, b: f64, v: f64| {
        let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (heading, strength) = alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64);
    let wnd = 0.1 + 1.8 * strength as f64 / 1000.0;
    let wa = heading as f64 / 1000.0;
    let amp = smooth(0.0, 2.5, depth) * (0.5 + 1.1 * wnd) * (1.0 + 1.2 * smooth(1.5, 8.0, depth));
    let t = (world.tick as f64 + alpha as f64) / 60.0;
    let mut h = 0.0;
    for i in 0..6 {
        let a = DIRS[i].to_radians();
        let k = std::f64::consts::TAU / LENS[i];
        let w = (9.8 * k).sqrt();
        let ph = k * (a.dcos() * x as f64 + a.dsin() * z as f64) - w * t + i as f64 * 1.7;
        let along = (a.dcos() * wa.dcos() + a.dsin() * wa.dsin()).max(0.0);
        h += HS[i] * amp * (0.12 + 0.88 * along * along) * 1.6 * ph.dsin();
    }
    h as f32
}

/// Fish in deep water: small schools, placed by the seed, circling just under the surface and riding the swell;
/// now and then one leaps out, silver in the light. Only drawn, not simulated.
pub(crate) fn push_fish(out: &mut Mesh, world: &World, view: (f32, f32, f32, f32), t: f32, alpha: f32) {
    use shapes::limb;
    let step = 5;
    let (x0, z0) = ((view.0 as i32).div_euclid(step) * step, (view.1 as i32).div_euclid(step) * step);
    let (x1, z1) = (view.2 as i32, view.3 as i32);
    let mut ty = z0;
    while ty <= z1 {
        let mut tx = x0;
        while tx <= x1 {
            let h = alaz_sim::rng::hash2(world.seed ^ 0xF15F, tx, ty) as u32;
            // the same schools the game knows of (a fish can be caught where one is drawn)
            if alaz_sim::map::fish_school(&world.map, tx, ty) {
                let (cx, cz) = (tx as f32 + 0.5 + (h >> 8 & 7) as f32 * 0.3, ty as f32 + 0.5 + (h >> 11 & 7) as f32 * 0.3);
                let n = 2 + (h >> 14) % 3;
                let dir = if h >> 17 & 1 == 0 { 1.0 } else { -1.0 };
                let speed = 0.25 + (h >> 18 & 7) as f32 * 0.04;
                let big = 1.6 + (h >> 21 & 3) as f32 * 0.25;
                for k in 0..n {
                    // each one on its own ring, a little behind the one in front
                    let r = 1.0 + 0.35 * k as f32;
                    let a = t * speed * dir + k as f32 * 0.55 + (h & 255) as f32;
                    let (x, z) = (cx + a.dcos() * r, cz + a.dsin() * r);
                    if alaz_sim::map::water_depth(&world.map, x.floor() as i32, z.floor() as i32) == 0 {
                        continue;
                    }
                    // heading along the ring
                    let (hx, hz) = (-a.dsin() * dir, a.dcos() * dir);
                    let len = 0.22 * big;
                    let surface = swell_height(world, x, z, alpha);
                    // a leap: every so often, for a moment, out of the water and back in an arc
                    let cycle = 9.0 + (h >> 23 & 7) as f32;
                    let ph = ((t + k as f32 * 3.1 + (h >> 4 & 31) as f32) % cycle) / 0.7;
                    let leaping = k == 0 && ph < 1.0;
                    let (y, col) = if leaping {
                        (surface + 0.55 * (ph * std::f32::consts::PI).dsin(), [0.78, 0.82, 0.86])
                    } else {
                        (surface + 0.02, [0.10, 0.19, 0.25])
                    };
                    // a tail beat
                    let beat = (t * 9.0 + k as f32).dsin() * 0.04;
                    let head = [x + hx * len * 0.5, y, z + hz * len * 0.5];
                    let tail = [x - hx * len * 0.5 - hz * beat, y + if leaping { -0.06 } else { 0.0 }, z - hz * len * 0.5 + hx * beat];
                    out.set_anchor(x, z);
                    limb(out, head, tail, 0.05 * big, 0.025 * big, col, 5);
                    let fin = [tail[0] - hx * 0.08 - hz * 0.05, tail[1], tail[2] - hz * 0.08 + hx * 0.05];
                    limb(out, tail, fin, 0.03 * big, 0.005, col, 4);
                }
            }
            tx += step;
        }
        ty += step;
    }
    out.set_anchor(0.0, 0.0);
}
