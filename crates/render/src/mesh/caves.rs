//! World-space cave terrain, rock details and held lights.

use super::*;
use alaz_sim::caves::{ORE_COAL, ORE_COPPER, ORE_IRON, ORE_TIN};
use alaz_sim::cave_terrain::CaveTile;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use super::shapes::{limb, rock, sphere};

/// A ball that shines by itself.
fn lamp(out: &mut Mesh, c: [f32; 3], r: [f32; 3], col: [f32; 3], seed: u32, glow: f32) {
    let start = out.len();
    sphere(out, c, r, col, 5, 3, 0.0, seed);
    for v in &mut out.verts[start..] {
        v.glow = glow;
        v.uv[1] = 0.0;
    }
}

fn unit(seed: u64, x: i32, y: i32, salt: i32) -> f32 {
    (alaz_sim::rng::hash2(seed ^ (salt as u64).wrapping_mul(0x9E37_79B9), x, y) % 10_000) as f32 / 10_000.0
}

fn ore_color(code: u8) -> ([f32; 3], f32) {
    match code {
        ORE_IRON => ([0.60, 0.30, 0.20], 0.0),
        ORE_COPPER => ([0.25, 0.78, 0.60], 0.6),
        ORE_TIN => ([0.72, 0.80, 0.92], 0.6),
        ORE_COAL => ([0.07, 0.07, 0.09], 0.0),
        _ => ([0.5, 0.5, 0.5], 0.0),
    }
}

/// A speck of ore: a rough lump that sits on a wall face.
fn nugget(out: &mut Mesh, c: [f32; 3], r: f32, code: u8, seed: u32) {
    let (col, glow) = ore_color(code);
    // (a glowing lump is drawn as a bright core with a lit crust round it)
    out.set_anchor(0.0, 0.0);
    rock(out, c, [r, r * 0.85, r], col, seed);
    if glow > 0.0 {
        lamp(out, c, [r * 0.55; 3], col, seed ^ 7, glow);
    }
}

pub(crate) fn collect_cave_entrances(world: &World, bounds: (i32, i32, i32, i32)) -> CaveSection {
    const REACH: i32 = 6;
    let (x0, y0, x1, y1) = bounds;
    let mut section = BTreeMap::new();
    // Read the same gallery on both sides of a chunk seam, then emit each floor once.
    for y in (y0 - REACH - 1).max(0)..(y1 + REACH + 1).min(world.map.h) {
        for x in (x0 - REACH - 1).max(0)..(x1 + REACH + 1).min(world.map.w) {
            let mut last = None;
            for mouth in world.map.cave_openings_at(x, y) {
                if last != Some(mouth.threshold) {
                    section.extend(world.cave_entrance_section(mouth.threshold, REACH));
                    last = Some(mouth.threshold);
                }
            }
        }
    }
    section
}

pub(crate) type CaveSection = BTreeMap<CaveTile, Option<i16>>;

fn gallery_neighbor(section: &CaveSection, tile: CaveTile, ceiling: Option<i16>, (dx, dy): (i32, i32)) -> Option<(CaveTile, Option<i16>)> {
    [0, -1, 1].into_iter().find_map(|dz| {
        let next = CaveTile { x: tile.x + dx, y: tile.y + dy, z: tile.z + dz };
        let &next_ceiling = section.get(&next)?;
        let lower_ceiling = if dz < 0 { next_ceiling } else { ceiling };
        let can_cross = dz == 0 || lower_ceiling != Some(tile.z.min(next.z) + 2);
        can_cross.then_some((next, next_ceiling))
    })
}

fn cave_face(out: &mut Mesh, points: [[f32; 3]; 3], color: [f32; 3], light: f32) {
    let vertices = points.map(|pos| {
        let mut v = out.v(pos, color, 0.0);
        v.kind = 1000.0;
        v.uv = [-2.0 - light, 3.0];
        v
    });
    out.extend_from_slice(&vertices);
}

fn push_gallery_tile(out: &mut Mesh, world: &World, section: &CaveSection, site: GalleryTile) {
    const EDGES: [((i32, i32), [f32; 2], [f32; 2]); 4] = [
        ((0, -1), [0.0, 0.0], [1.0, 0.0]),
        ((1, 0), [1.0, 0.0], [1.0, 1.0]),
        ((0, 1), [1.0, 1.0], [0.0, 1.0]),
        ((-1, 0), [0.0, 1.0], [0.0, 0.0]),
    ];
    let GalleryTile { tile, ceiling, light, flooded } = site;
    let x = tile.x as f32;
    let z = tile.y as f32;
    let floor = f32::from(tile.z);
    let mouths = world.map.cave_openings_at(tile.x, tile.y)
        .filter(|m| m.threshold == tile).collect::<Vec<_>>();
    let open = !mouths.is_empty();
    let roof = if open { None } else { ceiling.map(f32::from) };
    let tint = if flooded { [0.12, 0.32, 0.43] } else if open { [0.41, 0.37, 0.32] } else { [0.31, 0.28, 0.25] };
    let wall = if open { [0.39, 0.35, 0.30] } else { [0.30, 0.27, 0.25] };
    let stone = [0.26, 0.23, 0.21];
    out.set_anchor(x + 0.5, z + 0.5);
    let pos = |[u, v]: [f32; 2], height: f32| [x + u, height, z + v];
    let surface = |[u, v]: [f32; 2]| ground_height(world.seed, x + u, z + v);
    let center = pos([0.5, 0.5], floor);
    let mut edges = Vec::with_capacity(4);

    for (side, (direction, a, b)) in EDGES.into_iter().enumerate() {
        let outside = mouths.iter().any(|m| m.direction == (-direction.0, -direction.1));
        let neighbor = gallery_neighbor(section, tile, ceiling, direction);
        let edge_y = |p| {
            if outside { surface(p) } else { neighbor.map_or(floor, |(next, _)| (floor + f32::from(next.z)) * 0.5) }
        };
        let underside = |p| match (roof, neighbor) {
            (Some(h), Some((_, Some(other)))) => (h + f32::from(other)) * 0.5,
            (Some(h), _) => h,
            (None, _) => surface(p),
        };
        let feet = [pos(a, edge_y(a)), pos(b, edge_y(b))];
        let cap = [pos(a, underside(a).max(feet[0][1])), pos(b, underside(b).max(feet[1][1]))];
        edges.push((feet, cap));
        cave_face(out, [center, feet[0], feet[1]], tint, light);
        if let Some(h) = roof {
            cave_face(out, [pos([0.5, 0.5], h), cap[1], cap[0]], stone, light);
        }
        if outside {
            continue;
        }
        if let Some((next, _)) = neighbor {
            // A roof ending at the lip exposes its thickness above the opening.
            let next_open = world.map.cave_openings_at(next.x, next.y).any(|m| m.threshold == next);
            if roof.is_some() && next_open {
                let [u, v] = [pos(a, surface(a).max(cap[0][1])), pos(b, surface(b).max(cap[1][1]))];
                cave_face(out, [cap[1], cap[0], u], wall, light);
                cave_face(out, [cap[1], u, v], wall, light);
            }
            continue;
        }

        // Break the retaining face into rock facets. Edge vertices stay at the terrain seam.
        let random = alaz_sim::rng::hash2(world.seed ^ 0x4E21, tile.x, tile.y).rotate_right(side as u32 * 13);
        let along = 0.35 + (random & 255) as f32 / 850.0;
        let middle = |outward: f32, y: f32| {
            let u = a[0] + (b[0] - a[0]) * along + direction.0 as f32 * outward;
            let v = a[1] + (b[1] - a[1]) * along + direction.1 as f32 * outward;
            pos([u, v], y)
        };
        let foot_y = feet[0][1] + along * (feet[1][1] - feet[0][1]);
        let cap_y = cap[0][1] + along * (cap[1][1] - cap[0][1]);
        let ridge = middle(0.09, foot_y + (cap_y - foot_y) * 0.46);
        if light < 0.6 && let Some(ore) = world.natural_cave_ore_at(CaveTile { x: tile.x + direction.0, y: tile.y + direction.1, z: tile.z }) {
            let start = out.len();
            for n in 0..3 {
                let c = middle(-0.04, foot_y + (cap_y - foot_y) * (0.38 + n as f32 * 0.13));
                let [u, v] = [c[0] + (b[0] - a[0]) * (n - 1) as f32 * 0.17, c[2] + (b[1] - a[1]) * (n - 1) as f32 * 0.17];
                nugget(out, [u, c[1], v], 0.09, ore, random as u32 ^ n as u32);
            }
            mark_cave_geometry(&mut out.verts[start..], light);
            out.set_anchor(x + 0.5, z + 0.5);
        }
        let rim = if open {
            let rim = middle(-0.12 - along * 0.17, (cap_y - 0.08).max(foot_y));
            cave_face(out, [cap[0], cap[1], rim], [0.45, 0.41, 0.36], light);
            rim
        } else {
            middle(0.0, cap_y)
        };
        for (triangle, color) in [
            ([feet[0], feet[1], ridge], wall),
            ([feet[1], cap[1], ridge], tint),
            ([cap[1], rim, ridge], wall),
            ([rim, cap[0], ridge], tint),
            ([cap[0], feet[0], ridge], stone),
        ] {
            cave_face(out, triangle, color, light);
        }
    }

    for n in 0..4 {
        let (feet, caps) = edges[n];
        let (next, next_caps) = edges[(n + 1) % 4];
        if feet[1][1] != next[0][1] {
            cave_face(out, [center, feet[1], next[0]], tint, light);
        }
        if let Some(h) = roof && caps[1][1] != next_caps[0][1] {
            cave_face(out, [pos([0.5, 0.5], h), next_caps[0], caps[1]], stone, light);
        }
    }
}

struct GalleryTile {
    tile: CaveTile,
    ceiling: Option<i16>,
    light: f32,
    flooded: bool,
}

pub(crate) fn cave_daylight(world: &World, section: &CaveSection) -> BTreeMap<CaveTile, f32> {
    let mut open = VecDeque::new();
    let mut light = BTreeMap::new();
    for &tile in section.keys() {
        if world.natural_cave_exit_at(tile).is_some() {
            light.insert(tile, 1.0);
            open.push_back((tile, 0));
        }
    }
    while let Some((tile, steps)) = open.pop_front() {
        let next_steps = steps + 1;
        if next_steps >= 7 {
            continue;
        }
        for direction in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
            if let Some((next, _)) = gallery_neighbor(section, tile, section[&tile], direction)
                && let std::collections::btree_map::Entry::Vacant(entry) = light.entry(next) {
                let intensity = 1.0 - next_steps as f32 / 7.0;
                entry.insert(intensity * intensity);
                open.push_back((next, next_steps));
            }
        }
    }
    light
}

fn mark_cave_geometry(vertices: &mut [Vertex], light: f32) {
    for vertex in vertices {
        vertex.kind = 1000.0;
        vertex.uv[0] = -2.0 - light;
    }
}

pub(crate) fn build_cave_scene(world: &World, section: &CaveSection, light: &BTreeMap<CaveTile, f32>, flooded: &BTreeSet<CaveTile>) -> Vec<Vertex> {
    let mut out = Mesh::new();
    for (&tile, &ceiling) in section {
        let light = light.get(&tile).copied().unwrap_or(0.0);
        push_gallery_tile(&mut out, world, section, GalleryTile { tile, ceiling, light, flooded: flooded.contains(&tile) });
        if world.natural_cave_rubble_at(tile) {
            let start = out.len();
            let (x, z) = (tile.x as f32, tile.y as f32);
            for k in 0..7 {
                let u = |salt| unit(world.seed, tile.x, tile.y, salt + 7 * k);
                let radius = 0.14 + 0.17 * u(30);
                let y = f32::from(tile.z) + if k < 4 { 0.12 } else { 0.30 };
                rock(&mut out, [x + 0.22 + 0.56 * u(31), y, z + 0.22 + 0.56 * u(32)],
                    [radius, radius * 0.86, radius], [0.38, 0.32, 0.26], (tile.x * 31 + tile.y * 17 + k) as u32);
            }
            mark_cave_geometry(&mut out.verts[start..], light);
        }
    }
    out.verts
}

/// A burning torch held at (x, z): a stick and a flame that flickers and shrinks as it burns down. `frac` is the
/// share of the light left (0..1; below 0.2 the flame is low).
pub(crate) fn push_torch(out: &mut Mesh, x: f32, z: f32, secs: f32, frac: f32) {
    out.set_anchor(x, z);
    limb(out, [x, 1.0, z], [x, 1.42, z], 0.03, 0.026, [0.30, 0.20, 0.12], 5);
    if frac <= 0.0 {
        // burnt out: a dark head
        sphere(out, [x, 1.44, z], [0.04; 3], [0.1, 0.08, 0.07], 5, 3, 0.0, 1);
        return;
    }
    let low = (frac / 0.2).min(1.0);
    let flick = 1.0 + 0.12 * (secs * 17.0).dsin() + 0.08 * (secs * 29.0).dsin();
    let r = (0.05 + 0.04 * low) * flick;
    lamp(out, [x, 1.50 + 0.03 * (secs * 11.0).dsin(), z], [r, r * 1.7, r], [1.0, 0.62, 0.20], 2, 1.0);
    lamp(out, [x, 1.48, z], [r * 0.55, r, r * 0.55], [1.0, 0.92, 0.55], 3, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_sim::map::tag_seed_natural;

    #[test]
    fn natural_portals_join_the_surface_and_show_the_gallery() {
        for (seed, (tx, ty)) in [(4, (38, 230)), (42, (222, 150))] {
            let world = World::with_size(tag_seed_natural(seed, 256), 256, 256);
            let mouth = world.map.natural_cave_mouth(tx, ty).unwrap();
            let tile = mouth.threshold;
            let section = collect_cave_entrances(&world, (tile.x - 2, tile.y - 2, tile.x + 3, tile.y + 3));
            let light = cave_daylight(&world, &section);
            let mesh = build_cave_scene(&world, &section, &light, &BTreeSet::new());
            let x = tile.x as f32 + 0.5;
            let z = tile.y as f32 + 0.5;
            let (dx, dz) = (mouth.direction.0 as f32, mouth.direction.1 as f32);
            let at = |v: &Vertex| (v.pos[0] - x) * dx + (v.pos[2] - z) * dz;

            assert!(mesh.iter().any(|v| v.pos == [x, f32::from(tile.z), z]));
            let (lx, ly) = mouth.landing.xy();
            assert!(mesh.iter().any(|v| v.pos == [lx as f32 + 0.5, f32::from(mouth.landing.z), ly as f32 + 0.5]));
            for side in [-0.5, 0.5] {
                let front_x = x - dx * 0.5 + side * dz;
                let front_z = z - dz * 0.5 - side * dx;
                let surface = ground_height(world.seed, front_x, front_z);
                assert!(mesh.iter().any(|v| v.pos == [front_x, surface, front_z]));
            }

            // A full-height face across the shared edge would hide the landing from the approach.
            let seam_floor = f32::from(tile.z + mouth.landing.z) * 0.5;
            for face in mesh.chunks_exact(3).filter(|face| face.iter().all(|v| {
                at(v) == 0.5 && ((v.pos[0] - x) * dz - (v.pos[2] - z) * dx).abs() <= 0.5
            })) {
                let low = face.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
                let high = face.iter().map(|v| v.pos[1]).fold(f32::NEG_INFINITY, f32::max);
                assert!(low >= seam_floor + 1.0 || high <= seam_floor + 0.1);
            }
        }
    }
}
