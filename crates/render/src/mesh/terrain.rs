use super::*;

pub(crate) const HWIN: u32 = 256;

/// Ground height in metres at a point, bilinear between tile corners (what the shader lifts everything by).
/// Distance to the shore at every tile corner of the height window, in tiles: positive out on the water (how deep
/// it is, as far as the eye can tell), negative on land, 0 along the water's edge. Two chamfer passes over the window.
pub(crate) fn shore_field(world: &World, hx: i32, hy: i32) -> Vec<f32> {
    const CAP: f32 = 6.0;
    let n = HWIN as i32 + 1;
    let wet = |tx: i32, ty: i32| world.map.terrain(tx, ty).is_none_or(|t| t == Terrain::Water);
    // per tile (one wider than the window, so every corner has its four tiles): distance to the other kind
    let mut d = vec![CAP; (n * n) as usize];
    let water: Vec<bool> = (0..n * n).map(|i| wet(hx - 1 + i % n, hy - 1 + i / n)).collect();
    let idx = |x: i32, y: i32| (y * n + x) as usize;
    for y in 0..n {
        for x in 0..n {
            let me = water[idx(x, y)];
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let (a, b) = (x + dx, y + dy);
                a >= 0 && b >= 0 && a < n && b < n && water[idx(a, b)] != me
            });
            if edge {
                d[idx(x, y)] = 1.0;
            }
        }
    }
    let pass = |d: &mut Vec<f32>, order: &mut dyn Iterator<Item = (i32, i32)>, back: bool| {
        let s = if back { 1 } else { -1 };
        for (x, y) in order {
            let mut best = d[idx(x, y)];
            for (dx, dy, c) in [(s, 0, 1.0), (0, s, 1.0), (s, s, 1.414), (-s, s, 1.414)] {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < n && b < n {
                    best = best.min(d[idx(a, b)] + c);
                }
            }
            d[idx(x, y)] = best.min(CAP);
        }
    };
    pass(&mut d, &mut (0..n).flat_map(|y| (0..n).map(move |x| (x, y))), false);
    pass(&mut d, &mut (0..n).rev().flat_map(|y| (0..n).rev().map(move |x| (x, y))), true);
    let signed = |x: i32, y: i32| if water[idx(x, y)] { d[idx(x, y)] - 0.5 } else { -(d[idx(x, y)] - 0.5) };
    (0..HWIN as i32 * HWIN as i32)
        .map(|i| {
            let (x, y) = (i % HWIN as i32 + 1, i / HWIN as i32 + 1);
            // a corner sits between four tiles
            (signed(x - 1, y - 1) + signed(x, y - 1) + signed(x - 1, y) + signed(x, y)) * 0.25
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
            let (a, b, cc, d) = (c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 1.0));
            out.extend_from_slice(&[a, b, cc, a, cc, d]);
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
