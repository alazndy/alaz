//! Contracts for walking on the surface and underground in the same coordinates.

use crate::cave_terrain::WorldPoint;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::hash::Hash;

/// Search a walkable graph. Returned waypoints are ordered with the next step last.
pub(crate) fn search<K: Copy + Eq + Hash + Ord>(
    start: K,
    mut goal: impl FnMut(K) -> bool,
    heuristic: impl Fn(K) -> i32,
    mut neighbors: impl FnMut(K, &mut Vec<(K, i32)>),
    limit: usize,
    partial: bool,
) -> Option<Vec<K>> {
    let mut open = BinaryHeap::from([Reverse((heuristic(start), start, 0))]);
    let mut known = HashMap::from([(start, (0, start))]);
    let mut best = start;
    let mut expanded = 0;
    let mut edges = Vec::new();

    while let Some(Reverse((_, at, cost))) = open.pop() {
        if known[&at].0 != cost {
            continue;
        }
        if goal(at) {
            return Some(backtrack(at, start, &known));
        }
        if expanded == limit {
            break;
        }
        expanded += 1;
        if heuristic(at) < heuristic(best) {
            best = at;
        }
        neighbors(at, &mut edges);
        for (next, step) in edges.drain(..) {
            let total = cost + step;
            if known
                .get(&next)
                .is_none_or(|&(previous, _)| total < previous)
            {
                known.insert(next, (total, at));
                open.push(Reverse((total + heuristic(next), next, total)));
            }
        }
    }

    (partial && best != start).then(|| backtrack(best, start, &known))
}

fn backtrack<K: Copy + Eq + Hash>(mut at: K, start: K, known: &HashMap<K, (i32, K)>) -> Vec<K> {
    let mut route = Vec::new();
    while at != start {
        route.push(at);
        at = known[&at].1;
    }
    route
}

pub(crate) fn move_toward(from: (i64, i64), to: (i64, i64), speed: i64) -> (i64, i64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = ((dx * dx + dy * dy) as u64).isqrt() as i64;
    if length <= speed {
        return to;
    }
    (from.0 + dx * speed / length, from.1 + dy * speed / length)
}

pub(crate) fn walk_speed(a: &crate::world::Agent, water_depth: u8) -> i64 {
    let mut speed = a.kind.speed_mm();
    if a.sprint && a.stamina > 0 && water_depth < 2 {
        speed = speed * 17 / 10;
    }
    speed = match water_depth {
        1 => speed * 55 / 100,
        2 => speed * 45 / 100,
        _ => speed,
    }
    .max(1);
    if a.broken > 0 {
        speed = (speed * 55 / 100).max(1);
    }
    (speed * (1_000 - i64::from(a.inventory.gear().burden)) / 1_000).max(1)
}

/// A floor anchored at a tile centre. The height distinguishes stacked floors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct WalkFloor {
    pub x: i32,
    pub y: i32,
    pub z_mm: i64,
}

impl WalkFloor {
    pub(crate) fn center(self) -> WorldPoint {
        use crate::map::TILE_MM;
        WorldPoint {
            x_mm: i64::from(self.x) * TILE_MM + TILE_MM / 2,
            y_mm: i64::from(self.y) * TILE_MM + TILE_MM / 2,
            z_mm: self.z_mm,
        }
    }
}

impl From<crate::cave_terrain::CaveTile> for WalkFloor {
    fn from(tile: crate::cave_terrain::CaveTile) -> Self {
        Self {
            x: tile.x,
            y: tile.y,
            z_mm: i64::from(tile.z) * 1_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WalkRoute {
    pub goal: WorldPoint,
    /// The next floor is at the end of the list.
    pub remaining: Vec<WalkFloor>,
}
