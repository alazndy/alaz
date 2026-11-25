//! Walking: obstacles (water, buildings) and deterministic A* path finding on the tile grid.
//!
//! Agents go straight when the way ahead is clear and only plan a route when something blocks it.
//! Trees and rock outcrops are not obstacles (people walk between them); water and buildings are.
//! Everything is integer math with fixed tie-breaking, so every peer computes the same paths.

use crate::map::{Map, Terrain, TILE_MM};
use crate::agents::AgentKind;
use crate::world::Agent;
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) mod walking;

/// Tiny fast hasher for tile keys (std's SipHash dominates A* time). Only used for lookups,
/// never iterated, so its output cannot influence results.
#[derive(Default)]
pub(crate) struct TileHasher(u64);

/// A tile as one number, for any size of world.
fn tile_key(tx: i32, ty: i32) -> u64 {
    (ty as u32 as u64) << 32 | tx as u32 as u64
}

impl Hasher for TileHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0 ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 ^= self.0 >> 29;
    }
    fn write_u32(&mut self, v: u32) {
        self.0 = (self.0 ^ v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 ^= self.0 >> 29;
    }
}

/// Tiles blocked by buildings. Derived from the building list (never saved or hashed): rebuilt when
/// the list changes length, extended when a building goes up.
#[derive(Default, Clone, Debug)]
pub(crate) struct Obstacles {
    tiles: HashSet<u64, BuildHasherDefault<TileHasher>>,
    pub(crate) built_for: usize,
}

impl Obstacles {
    pub(crate) fn block_footprint(&mut self, map: &Map, x: i64, y: i64, r: i64) {
        let reach = r + TILE_MM / 2;
        let (t0x, t1x) = ((x - reach).div_euclid(TILE_MM) as i32, (x + reach).div_euclid(TILE_MM) as i32);
        let (t0y, t1y) = ((y - reach).div_euclid(TILE_MM) as i32, (y + reach).div_euclid(TILE_MM) as i32);
        for ty in t0y..=t1y {
            for tx in t0x..=t1x {
                if !map.in_bounds(tx, ty) {
                    continue;
                }
                let (cx, cy) = (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
                if (cx - x).pow(2) + (cy - y).pow(2) < reach * reach {
                    self.tiles.insert(tile_key(tx, ty));
                }
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.tiles.clear();
    }

    /// Does a building stand on this tile?
    pub(crate) fn blocked(&self, tx: i32, ty: i32) -> bool {
        self.tiles.contains(&tile_key(tx, ty))
    }
}

/// Read-only view of what blocks walking, plus this tick's path-planning allowance.
pub(crate) struct Nav<'a> {
    pub map: &'a Map,
    pub obstacles: &'a Obstacles,
    /// How many A* runs may still happen this tick (an agent over budget just waits a tick).
    pub budget: u32,
    /// The wet and the cold underfoot (mud slows, puddles slow a little).
    pub ground: crate::surface::Ground,
}

pub(crate) const PATHS_PER_TICK: u32 = 3;
/// A straight walk is trusted this far ahead (mm); beyond that it is re-checked as the agent goes.
const LOOKAHEAD_MM: i64 = 10_000;
const SEARCH_MARGIN_TILES: i32 = 40;
const MAX_EXPANSIONS: usize = 5_000;

#[derive(PartialEq, Eq)]
pub(crate) enum Step {
    Moved,
    Arrived,
    Blocked,
}

impl Nav<'_> {
    fn passable(&self, tx: i32, ty: i32) -> bool {
        match self.map.terrain(tx, ty) {
            None | Some(Terrain::Water) => false,
            _ => !self.obstacles.tiles.contains(&tile_key(tx, ty)) && !self.map.surface_cave_gap(tx, ty),
        }
    }

    /// Is the straight segment (a to b, at most `limit` mm of it) free? The tile the walker stands on is
    /// always allowed, so someone caught inside a fresh building can walk out.
    fn line_clear(&self, (ax, ay): (i64, i64), (bx, by): (i64, i64), limit: i64) -> bool {
        let (dx, dy) = (bx - ax, by - ay);
        let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        let reach = len.min(limit);
        if len == 0 {
            return true;
        }
        let start = (ax.div_euclid(TILE_MM) as i32, ay.div_euclid(TILE_MM) as i32);
        let mut d = 0;
        let mut last: Option<i64> = None;
        while d <= reach {
            let (x, y) = (ax + dx * d / len, ay + dy * d / len);
            let t = (x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32);
            if t != start && !self.passable(t.0, t.1) {
                return false;
            }
            // a sheer rock face across the way: better to plan a route round it than to climb it
            if self.map.terrain(t.0, t.1) == Some(Terrain::Rock) {
                let e = crate::map::elevation_at_mm(self.map.seed, x, y) as i64;
                if last.is_some_and(|l| (e - l).abs() > TILE_MM / 3 * 2) {
                    return false;
                }
                last = Some(e);
            } else {
                last = None;
            }
            d += TILE_MM / 3;
        }
        true
    }

    /// A* from tile `s` to tile `g` (8-way, no corner cutting). Returns the waypoint tiles from the first
    /// step to the end, or the best partial route if the search is cut short. None when nothing helps.
    fn find_path(&self, s: (i32, i32), g: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        let (min_x, max_x) = (s.0.min(g.0) - SEARCH_MARGIN_TILES, s.0.max(g.0) + SEARCH_MARGIN_TILES);
        let (min_y, max_y) = (s.1.min(g.1) - SEARCH_MARGIN_TILES, s.1.max(g.1) + SEARCH_MARGIN_TILES);
        let w = self.map.w;
        let key = |t: (i32, i32)| (t.1 * w + t.0) as u32;
        let h = |t: (i32, i32)| {
            let (dx, dy) = ((t.0 - g.0).abs(), (t.1 - g.1).abs());
            10 * (dx + dy) - 6 * dx.min(dy)
        };
        let ok = |t: (i32, i32)| {
            t.0 >= min_x && t.0 <= max_x && t.1 >= min_y && t.1 <= max_y && (t == g || t == s || self.passable(t.0, t.1))
        };
        let entrances: Vec<_> = self.map.cave_openings_at(g.0, g.1)
            .map(|mouth| (g.0 - mouth.direction.0, g.1 - mouth.direction.1)).collect();
        let tile = |k: u32| ((k % w as u32) as i32, (k / w as u32) as i32);
        let mut route = walking::search(key(s), |k| k == key(g), |k| h(tile(k)), |k, edges| {
            let cur = tile(k);
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)] {
                let n = (cur.0 + dx, cur.1 + dy);
                if !ok(n) {
                    continue;
                }
                if n == g && !entrances.is_empty() && !entrances.contains(&cur) {
                    continue;
                }
                let step = if dx != 0 && dy != 0 {
                    // no cutting corners between two blocked neighbours
                    if !ok((cur.0 + dx, cur.1)) || !ok((cur.0, cur.1 + dy)) {
                        continue;
                    }
                    14
                } else {
                    10
                };
                // a rock face can be climbed, but a way round it is better if there is one: each metre up or down
                // a ledge costs as much as a tile walked. Only rock has ledges, so the heights are only looked up there.
                let climb = if self.map.terrain(cur.0, cur.1) == Some(Terrain::Rock) && self.map.terrain(n.0, n.1) == Some(Terrain::Rock) {
                    let d = (crate::map::elevation_mm(self.map.seed, cur.0, cur.1) - crate::map::elevation_mm(self.map.seed, n.0, n.1)).abs();
                    if d > 700 { d / 100 } else { 0 }
                } else {
                    0
                };
                edges.push((key(n), step + climb));
            }
        }, MAX_EXPANSIONS, true)?;
        route.reverse();
        Some(route.into_iter().map(tile).collect())
    }

    /// String-pull a tile route into fewer waypoints: skip ahead while the straight line stays clear.
    fn smooth(&self, from: (i64, i64), route: &[(i32, i32)]) -> Vec<(i32, i32)> {
        let center = |t: (i32, i32)| (t.0 as i64 * TILE_MM + TILE_MM / 2, t.1 as i64 * TILE_MM + TILE_MM / 2);
        let mut out = Vec::new();
        let mut at = from;
        let mut i = 0;
        while i < route.len() {
            let mut j = i;
            while j + 1 < route.len() && self.line_clear(at, center(route[j + 1]), i64::MAX) {
                j += 1;
            }
            out.push(route[j]);
            at = center(route[j]);
            i = j + 1;
        }
        out
    }
}

/// Most height a climber gains or loses in one tick on a rock face (mm): a 2.5 m ledge takes about five seconds.
const CLIMB_MM: i64 = 12;

/// What a step taken straight ahead (no route planning) came to.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum Free {
    Moved,
    Arrived,
    Blocked,
    /// The ground fell away underfoot: the walker went over the edge with this velocity (mm per tick).
    Launch { vx: i64, vy: i64 },
}

/// Walk straight toward (tx, ty), the way the keys push: no route planning, and no care at an edge either. Where
/// the ground drops away more steeply than legs can follow, the walker goes over and falls.
pub(crate) fn walk_free(a: &mut Agent, nav: &Nav, tx: i64, ty: i64) -> Free {
    let mut speed = walking::walk_speed(a, 0);
    let gear = a.inventory.gear();
    if !a.fly {
        speed = (speed * nav.ground.walk_permille(nav.map, a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32, gear.grip) as i64 / 1000).max(1);
    }
    let (dx, dy) = (tx - a.x, ty - a.y);
    let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
    if len == 0 {
        return Free::Arrived;
    }
    if a.fly {
        return match step_toward(a, nav, tx, ty, true) {
            Step::Moved => Free::Moved,
            Step::Arrived => Free::Arrived,
            Step::Blocked => Free::Blocked,
        };
    }
    let step = len.min(speed);
    let (nx, ny) = walking::move_toward((a.x, a.y), (tx, ty), speed);
    let here = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
    let next = (nx.div_euclid(TILE_MM) as i32, ny.div_euclid(TILE_MM) as i32);
    // people may walk on into water on their own (wading, then swimming); routes still go round it
    let into_water = a.kind == AgentKind::Human && nav.map.terrain(next.0, next.1) == Some(Terrain::Water);
    // ...and into the open lip of a cave, which they simply walk down into
    let into_cave = a.kind == AgentKind::Human && nav.map.cave_openings_at(next.0, next.1).any(|mouth| {
        next == here || (next.0 - mouth.direction.0, next.1 - mouth.direction.1) == here
    });
    if next != here && !nav.passable(next.0, next.1) && !into_water && !into_cave {
        return Free::Blocked;
    }
    let drop = (crate::map::elevation_at_mm(nav.map.seed, a.x, a.y) - crate::map::elevation_at_mm(nav.map.seed, nx, ny)) as i64;
    if !into_cave && drop > step * 3 / 2 + 5 {
        return Free::Launch { vx: nx - a.x, vy: ny - a.y };
    }
    match step_toward(a, nav, tx, ty, true) {
        Step::Moved => Free::Moved,
        Step::Arrived => Free::Arrived,
        Step::Blocked => Free::Blocked,
    }
}

/// `swim`: walking on by hand, a person may go into water; a planned route only leaves it, never enters it.
fn step_toward(a: &mut Agent, nav: &Nav, tx: i64, ty: i64, swim: bool) -> Step {
    if a.fly {
        // sandbox flight: straight over water, rock and buildings, four times a walker's pace
        let speed = a.kind.speed_mm() * 4;
        let (nx, ny) = walking::move_toward((a.x, a.y), (tx, ty), speed);
        if nav.map.terrain_mm(nx, ny).is_none() {
            return Step::Blocked;
        }
        a.x = nx;
        a.y = ny;
        a.z = i64::from(crate::map::elevation_at_mm(nav.map.seed, nx, ny));
        return if (nx, ny) == (tx, ty) { Step::Arrived } else { Step::Moved };
    }
    let depth = crate::map::water_depth(nav.map, a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
    let mut speed = walking::walk_speed(a, depth);
    let gear = a.inventory.gear();
    // mud and standing water drag at the feet (the soles' grip helps in mud)
    if depth == 0 {
        speed = (speed * nav.ground.walk_permille(nav.map, a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32, gear.grip) as i64 / 1000).max(1);
    }
    let (dx, dy) = (tx - a.x, ty - a.y);
    {
        // uphill is slower, downhill a little quicker: grade over the intended step, in permille
        let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        if len > 0 {
            let (px, py) = walking::move_toward((a.x, a.y), (tx, ty), speed);
            let rise = (crate::map::elevation_at_mm(nav.map.seed, px, py) - crate::map::elevation_at_mm(nav.map.seed, a.x, a.y)) as i64;
            let grade = rise * 1000 / len.min(speed).max(1);
            // good soles keep their grip: less of the climb's slowing
            let slow = (grade * 2).clamp(-200, 1000);
            let slow = if slow > 0 { crate::rules::Gear::cut(slow as i32, gear.grip) as i64 } else { slow };
            speed = (speed * 1000 / (1000 + slow)).max(1);
            // a rock face is climbed, up or down, hand over hand: only so much height a tick
            if grade.abs() * speed / 1000 > CLIMB_MM {
                speed = (CLIMB_MM * 1000 + grade.abs() - 1) / grade.abs();
            }
        }
    }
    let (nx, ny) = walking::move_toward((a.x, a.y), (tx, ty), speed);
    let next_tile = (nx.div_euclid(TILE_MM) as i32, ny.div_euclid(TILE_MM) as i32);
    let current_tile = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
    if next_tile != current_tile {
        let mut mouths = nav.map.cave_openings_at(next_tile.0, next_tile.1).peekable();
        if mouths.peek().is_some() && (a.kind != AgentKind::Human || !mouths.any(|mouth| {
            (next_tile.0 - mouth.direction.0, next_tile.1 - mouth.direction.1) == current_tile
        })) {
            return Step::Blocked;
        }
    }
    match nav.map.terrain_mm(nx, ny) {
        None => return Step::Blocked,
        // people may go into water; nothing else does
        Some(Terrain::Water) if a.kind != AgentKind::Human => return Step::Blocked,
        Some(Terrain::Water) if !swim && nav.map.terrain_mm(a.x, a.y) != Some(Terrain::Water) => return Step::Blocked,
        _ => {}
    }
    a.x = nx;
    a.y = ny;
    a.z = i64::from(crate::map::elevation_at_mm(nav.map.seed, nx, ny));
    if (nx, ny) == (tx, ty) { Step::Arrived } else { Step::Moved }
}

/// Move one step toward (tx, ty), walking around water and buildings.
pub(crate) fn advance(a: &mut Agent, nav: &mut Nav, tx: i64, ty: i64) -> Step {
    if a.fly {
        return step_toward(a, nav, tx, ty, true);
    }
    let goal = (tx.div_euclid(TILE_MM) as i32, ty.div_euclid(TILE_MM) as i32);
    if a.path_goal != goal {
        a.path.clear();
        a.walk_route = None;
        a.path_goal = goal;
    }
    let center = |t: (i32, i32)| (t.0 as i64 * TILE_MM + TILE_MM / 2, t.1 as i64 * TILE_MM + TILE_MM / 2);
    // a waypoint that has become blocked (a building went up) means the plan is stale
    if let Some(&next) = a.path.last()
        && !nav.passable(next.0, next.1) && next != goal {
            a.path.clear();
        a.walk_route = None;
        }
    if a.path.is_empty() {
        a.walk_route = None;
        // caught inside a building footprint (it went up on top of us, or a worker came out of its door):
        // there is no route from a blocked tile, so just walk straight out
        let here = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
        if !nav.passable(here.0, here.1) {
            return step_toward(a, nav, tx, ty, false);
        }
        if nav.line_clear((a.x, a.y), (tx, ty), LOOKAHEAD_MM) {
            return match step_toward(a, nav, tx, ty, false) {
                Step::Blocked => Step::Blocked,
                s => s,
            };
        }
        if nav.budget == 0 {
            return Step::Moved; // wait for a planning slot
        }
        nav.budget -= 1;
        let start = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
        let Some(route) = nav.find_path(start, goal) else { return Step::Blocked };
        let mut wp = nav.smooth((a.x, a.y), &route);
        wp.reverse();
        a.path = wp;
    }
    let dest = crate::cave_terrain::WorldPoint {
        x_mm: tx, y_mm: ty, z_mm: i64::from(crate::map::elevation_at_mm(nav.map.seed, tx, ty)),
    };
    let route = a.walk_route.get_or_insert_with(|| walking::WalkRoute {
        goal: dest,
        remaining: a.path.iter().map(|&(x, y)| walking::WalkFloor {
            x, y, z_mm: i64::from(crate::map::elevation_mm(nav.map.seed, x, y)),
        }).collect(),
    });
    route.goal = dest;
    let Some(next) = route.remaining.last().copied() else { return Step::Blocked };
    // the very last waypoint is the goal tile: aim at the exact target instead of its center
    let (wx, wy) = if route.remaining.len() == 1 && (next.x, next.y) == goal { (tx, ty) } else { center((next.x, next.y)) };
    match step_toward(a, nav, wx, wy, false) {
        Step::Arrived => {
            a.path.pop();
            if let Some(route) = &mut a.walk_route { route.remaining.pop(); }
            if (a.x, a.y) == (tx, ty) { Step::Arrived } else { Step::Moved }
        }
        s => s,
    }
}
