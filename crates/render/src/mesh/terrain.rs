use super::*;

pub(crate) const HWIN: u32 = 256;

/// Distance from the centre of each tile to the shore, positive in water.
pub(crate) fn shore_field(world: &World, hx: i32, hy: i32) -> Vec<f32> {
    shore_distances(hx, hy, |tx, ty| world.map.terrain(tx, ty).is_none_or(|t| t == Terrain::Water))
}

fn shore_distances(hx: i32, hy: i32, wet: impl Fn(i32, i32) -> bool) -> Vec<f32> {
    // Compute the whole distance range outside the window as well so that shifting it never moves the shore.
    const PAD: i32 = 6;
    const CAP: f32 = PAD as f32 - 0.5;
    let size = HWIN as i32;
    let n = size + 2 * PAD;
    let mut d = vec![CAP; (n * n) as usize];
    let water: Vec<bool> = (0..n * n).map(|i| wet(hx - PAD + i % n, hy - PAD + i / n)).collect();
    let idx = |x: i32, y: i32| (y * n + x) as usize;
    for y in 0..n {
        for x in 0..n {
            let me = water[idx(x, y)];
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().any(|(dx, dy)| {
                let (a, b) = (x + dx, y + dy);
                a >= 0 && b >= 0 && a < n && b < n && water[idx(a, b)] != me
            });
            if edge {
                d[idx(x, y)] = 0.5;
            }
        }
    }
    let pass = |d: &mut [f32], order: &mut dyn Iterator<Item = (i32, i32)>, back: bool| {
        let s = if back { 1 } else { -1 };
        for (x, y) in order {
            let mut best = d[idx(x, y)];
            for (dx, dy, c) in [(s, 0, 1.0), (0, s, 1.0), (s, s, std::f32::consts::SQRT_2), (-s, s, std::f32::consts::SQRT_2)] {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < n && b < n {
                    best = best.min(d[idx(a, b)] + c);
                }
            }
            d[idx(x, y)] = best;
        }
    };
    pass(&mut d, &mut (0..n).flat_map(|y| (0..n).map(move |x| (x, y))), false);
    pass(&mut d, &mut (0..n).rev().flat_map(|y| (0..n).rev().map(move |x| (x, y))), true);
    (0..size * size)
        .map(|i| {
            let at = idx(i % size + PAD, i / size + PAD);
            if water[at] { d[at] } else { -d[at] }
        })
        .collect()
}

/// Flat colour tint of one ground tile: broad light and dark patches (close to 1).
pub(crate) fn tile_shade(world: &World, tx: i32, ty: i32) -> f32 {
    1.0 + 0.09 * noise(world, 0x7771, tx as f32 * 0.16, ty as f32 * 0.16) + 0.025 * noise(world, 0x7773, tx as f32 * 0.9, ty as f32 * 0.9)
}

pub(crate) fn tile_spr(art: &Art, t: Terrain, h: u32) -> Spr {
    let i = (h % 4) as usize;
    match t {
        Terrain::Water => art.water[i],
        Terrain::Sand => art.sand[i],
        Terrain::Grass => art.grass[i],
        Terrain::Forest => art.forest[i],
        Terrain::Rock => art.rock[i],
    }
}

pub(crate) fn build_terrain(out: &mut Mesh, art: &Art, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    for ty in y0..y1 {
        for tx in x0..x1 {
            let t = world.map.terrain(tx, ty).unwrap_or(Terrain::Water);
            let h = (alaz_sim::rng::hash2(world.seed ^ 0x7772, tx, ty) >> 20) as u32;
            let s = tile_spr(art, t, h);
            let k = if t == Terrain::Water { 1.0 } else { tile_shade(world, tx, ty) };
            let (fx, fz) = (tx as f32, ty as f32);
            let c = |dx: f32, dz: f32| Vertex { pos: [fx + dx, 0.0, fz + dz], color: [k, k, k], glow: 0.0, uv: [s.x as f32, s.y as f32], off: [if t == Terrain::Water { 1.0 } else { 0.0 }, 0.0], kind: 1.0 };
            // Four quads a tile: the smooth ground surface needs vertices closer than a metre to show.
            for (qx, qz) in [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (0.5, 0.5)] {
                let (a, b, cc, d) = (c(qx, qz), c(qx + 0.5, qz), c(qx + 0.5, qz + 0.5), c(qx, qz + 0.5));
                out.extend_from_slice(&[a, b, cc, a, cc, d]);
            }
            // (ground boundaries are drawn ragged by the shader now, from the tile window)
            let _ = edge_blend;
        }
    }
}

/// Higher-ranked ground creeps over lower-ranked neighbours with a ragged, dithered fringe.
pub(crate) fn edge_blend(out: &mut Mesh, art: &Art, world: &World, tx: i32, ty: i32, t: Terrain) {
    let rank = |t: Terrain| -> i32 { match t {
        Terrain::Water => 0,
        Terrain::Sand => 1,
        Terrain::Grass => 2,
        Terrain::Forest => 3,
        Terrain::Rock => 4,
    } };
    // (dx, dy, direction index): edges N E S W, then corners NE SE SW NW
    const DIRS: [(i32, i32, usize); 8] = [(0, -1, 0), (1, 0, 1), (0, 1, 2), (-1, 0, 3), (1, -1, 4), (1, 1, 5), (-1, 1, 6), (-1, -1, 7)];
    let at = |x: i32, y: i32| world.map.terrain(x, y);
    let mut found: [Option<(Terrain, usize)>; 8] = [None; 8];
    for (i, &(dx, dy, dir)) in DIRS.iter().enumerate() {
        let Some(n) = at(tx + dx, ty + dy) else { continue };
        if rank(n) <= rank(t) {
            continue;
        }
        if dir >= 4 {
            // a corner only matters when neither adjacent edge already carries the same ground
            if at(tx + dx, ty) == Some(n) || at(tx, ty + dy) == Some(n) {
                continue;
            }
        }
        found[i] = Some((n, dir));
    }
    found.sort_by_key(|f| f.map_or(-1, |(n, _)| rank(n)));
    for (n, dir) in found.into_iter().flatten() {
        let (fx, fz) = (tx as f32 + 0.5, ty as f32 + 0.5);
        let k = if n == Terrain::Water { 1.0 } else { tile_shade(world, tx, ty) };
        decal_sized(out, art.edge[rank(n) as usize * 8 + dir], fx, fz, 1.0, 1.0, [k, k, k], 0.0);
    }
}

#[cfg(test)]
mod shore_tests {
    use super::*;

    #[test]
    fn a_one_tile_stream_keeps_a_wet_centre() {
        let field = shore_distances(-2, 0, |x, _| x == 0);
        assert_eq!(&field[..5], &[-1.5, -0.5, 0.5, -0.5, -1.5]);
    }

    #[test]
    fn shoreline_distance_agrees_where_windows_overlap() {
        let wet = |x, y| x > y;
        let left = shore_distances(-12, -18, wet);
        let right = shore_distances(0, 0, wet);
        for y in 0..28 {
            for x in 0..28 {
                assert_eq!(left[(y + 18) * HWIN as usize + x + 12], right[y * HWIN as usize + x]);
            }
        }
    }
}
