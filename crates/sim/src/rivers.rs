//! Rivers and lakes of the fourth generator, found by letting rain run downhill over the whole land.
//!
//! The land is filled in at its hollows (priority flood from the sea and the map edge), every tile drains to the
//! neighbour it was reached from, and the tiles that many others drain through become rivers. Hollows deeper than a
//! few height units become lakes. Integer-only and tie-broken by a hash, so every peer works out the same water.
//! The domain is the largest small world (256 x 256); a smaller world is a window onto it.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) const SIDE: i32 = 256;
const CELLS: usize = (SIDE * SIDE) as usize;
const SEA: i32 = 95;
/// Tiles draining through a tile for it to be a river.
const RIVER_AT: u32 = 420;
/// Rivers are only cut into the lowlands: water stands at sea level, so a river high in the hills would be a canyon.
const RIVER_BELOW: i32 = 165;
/// A ford is only laid where the water is under this many times RIVER_AT: past that a river is too big to wade.
const FORD_BELOW: u32 = 16;
/// Streams under this many times RIVER_AT are small: they get a ford every FORD_APART_SMALL tiles, a bigger river
/// one every FORD_APART_BIG, because a wide ford is a bigger work of the land.
const SMALL_BELOW: u32 = 4;
const FORD_APART_SMALL: i32 = 24;
const FORD_APART_BIG: i32 = 40;
/// A ford is at most this many tiles of water across.
const FORD_ACROSS: i32 = 9;
const NEIGHBOURS: [(i32, i32); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];

pub(crate) struct RiverMap {
    height: Vec<u8>,
    /// Which way the water runs on a river tile (index into NEIGHBOURS), 255 elsewhere.
    flow: Vec<u8>,
    /// How much of the current a river water tile has, in percent: full along the middle, weaker toward the banks.
    strength: Vec<u8>,
    /// Where the stream can be crossed on foot: the land just outside each bar of gravel laid across a ford.
    #[cfg_attr(not(test), allow(dead_code))]
    fords: Vec<((i32, i32), (i32, i32))>,
    /// Gravel laid across a ford: dry ground the stream still runs over, so its drainage carries on through it.
    #[cfg_attr(not(test), allow(dead_code))]
    gravel: Vec<bool>,
    /// Tiles to the nearest water (the sea, a lake or a river), up to 8.
    shore: Vec<u8>,
}

impl RiverMap {
    pub(crate) fn height(&self, seed: u64, x: i32, y: i32) -> i32 {
        match Self::at(x, y) {
            Some(i) => i32::from(self.height[i]),
            None => crate::map::raw_height(seed, x, y),
        }
    }

    /// How many tiles the nearest water is, up to 8 (and 8 beyond the map).
    pub(crate) fn shore(&self, x: i32, y: i32) -> i32 {
        Self::at(x, y).map_or(8, |i| i32::from(self.shore[i]))
    }

    /// The steepest rise to a neighbouring tile corner in raw height units.
    pub(crate) fn steepness(&self, seed: u64, x: i32, y: i32) -> i32 {
        let h = self.height(seed, x, y);
        [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().map(|(dx, dy)| (self.height(seed, x + dx, y + dy) - h).abs()).max().unwrap_or(0)
    }

    /// The way the water of a river tile runs, as a unit step (nothing for the sea, lakes and dry land).
    pub(crate) fn flow(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let code = self.flow[Self::at(x, y)?];
        NEIGHBOURS.get(usize::from(code)).copied()
    }

    /// The share of the current (percent) that the water of this tile carries.
    pub(crate) fn strength(&self, x: i32, y: i32) -> i32 {
        Self::at(x, y).map_or(100, |i| i32::from(self.strength[i]))
    }

    /// The two banks of every ford, as the dry tile on each side of the gravel bar.
    #[cfg(test)]
    pub(crate) fn is_gravel(&self, x: i32, y: i32) -> bool {
        Self::at(x, y).is_some_and(|i| self.gravel[i])
    }

    #[cfg(test)]
    pub(crate) fn fords(&self) -> &[((i32, i32), (i32, i32))] {
        &self.fords
    }

    fn at(x: i32, y: i32) -> Option<usize> {
        ((0..SIDE).contains(&x) && (0..SIDE).contains(&y)).then(|| (y * SIDE + x) as usize)
    }
}

static MAPS: OnceLock<Mutex<Vec<(u64, Arc<RiverMap>)>>> = OnceLock::new();

pub(crate) fn river_map(seed: u64) -> Arc<RiverMap> {
    let cache = MAPS.get_or_init(Default::default);
    if let Some((_, m)) = cache.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(s, _)| *s == seed) {
        return Arc::clone(m);
    }
    let built = Arc::new(build(seed));
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, m)) = c.iter().find(|(s, _)| *s == seed) {
        return Arc::clone(m);
    }
    if c.len() >= 4 {
        c.remove(0);
    }
    c.push((seed, Arc::clone(&built)));
    built
}

fn build(seed: u64) -> RiverMap {
    let raw: Vec<i32> = (0..CELLS as i32).map(|i| crate::map::raw_height(seed, i % SIDE, i / SIDE)).collect();
    // a tie-break so flat ground still drains somewhere
    let key: Vec<i64> = (0..CELLS).map(|i| i64::from(raw[i]) * 64 + (crate::rng::hash2(seed ^ 0x5133, i as i32 % SIDE, i as i32 / SIDE) & 63) as i64).collect();
    let mut filled = vec![i64::MAX; CELLS];
    let mut parent = vec![u32::MAX; CELLS];
    let mut order: Vec<u32> = Vec::with_capacity(CELLS);
    let mut heap = BinaryHeap::new();
    for i in 0..CELLS {
        let (x, y) = (i as i32 % SIDE, i as i32 / SIDE);
        if raw[i] < SEA || x == 0 || y == 0 || x == SIDE - 1 || y == SIDE - 1 {
            filled[i] = key[i];
            heap.push(Reverse((key[i], i as u32)));
        }
    }
    while let Some(Reverse((level, c))) = heap.pop() {
        if level > filled[c as usize] {
            continue;
        }
        order.push(c);
        let (cx, cy) = (c as i32 % SIDE, c as i32 / SIDE);
        for (dx, dy) in NEIGHBOURS {
            let Some(n) = RiverMap::at(cx + dx, cy + dy) else { continue };
            if filled[n] != i64::MAX {
                continue;
            }
            parent[n] = c;
            filled[n] = key[n].max(level + 1);
            heap.push(Reverse((filled[n], n as u32)));
        }
    }
    let mut drain = vec![1u32; CELLS];
    for &c in order.iter().rev() {
        let p = parent[c as usize];
        if p != u32::MAX {
            drain[p as usize] += drain[c as usize];
        }
    }
    let mut height: Vec<u8> = raw.iter().map(|&h| h as u8).collect();
    let mut flow = vec![255u8; CELLS];
    let mut strength = vec![100u8; CELLS];
    // the nearest river middle (in thousandths of a tile) that has given a tile its current
    let mut nearest = vec![i32::MAX; CELLS];
    let mut lake = vec![false; CELLS];
    // lakes: hollows of the lower country that rain fills at least three units deep
    for i in 0..CELLS {
        let depth = ((filled[i] - key[i]) / 64) as i32;
        if depth >= 3 && (SEA + 5..150).contains(&raw[i]) {
            height[i] = 94;
            lake[i] = true;
        }
    }
    // rivers: cut into the lowlands, widening with the water they carry
    for i in 0..CELLS {
        if drain[i] < RIVER_AT || raw[i] < SEA || raw[i] > RIVER_BELOW {
            continue;
        }
        let (x, y) = (i as i32 % SIDE, i as i32 / SIDE);
        let p = parent[i];
        if p != u32::MAX {
            let step = (p as i32 % SIDE - x, p as i32 / SIDE - y);
            flow[i] = NEIGHBOURS.iter().position(|&d| d == step).map_or(255, |k| k as u8);
        }
        let code = flow[i];
        // half the water's width in thousandths of a tile: a stream is about two tiles across, a big river five
        let core = 700 + 500 * ((drain[i] / RIVER_AT).isqrt() as i32).min(3);
        let bank = core + 3000;
        let reach = bank / 1000 + 1;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let Some(n) = RiverMap::at(x + dx, y + dy) else { continue };
                let d = (((dx * dx + dy * dy) as u64) * 1_000_000).isqrt() as i32;
                if d >= bank {
                    continue;
                }
                let t = (d - core).max(0) * 1000 / (bank - core);
                let h = i32::from(height[n]);
                if h > 94 {
                    height[n] = (94 + (h - 94) * t / 1000).min(h) as u8;
                }
                // the whole width of the water runs with the river, the banks more slowly than the middle
                if code != 255 && !lake[n] && raw[n] >= SEA && height[n] < SEA as u8 && d < nearest[n] {
                    nearest[n] = d;
                    flow[n] = code;
                    strength[n] = (100 - 70 * d / (core + 1000)).clamp(30, 100) as u8;
                }
            }
        }
    }
    let before = height.clone();
    let fords = lay_fords(&drain, &flow, &raw, &lake, &mut height);
    let gravel: Vec<bool> = (0..CELLS).map(|i| height[i] != before[i]).collect();
    // distance to water, a few sweeps of a chamfer transform
    let mut shore = vec![8u8; CELLS];
    for i in 0..CELLS {
        if height[i] < SEA as u8 {
            shore[i] = 0;
        }
    }
    for pass in 0..2 {
        let range: Vec<usize> = if pass == 0 { (0..CELLS).collect() } else { (0..CELLS).rev().collect() };
        for i in range {
            let (x, y) = (i as i32 % SIDE, i as i32 / SIDE);
            for (dx, dy) in NEIGHBOURS {
                if let Some(n) = RiverMap::at(x + dx, y + dy) {
                    shore[i] = shore[i].min(shore[n].saturating_add(1));
                }
            }
        }
    }
    RiverMap { height, flow, strength, fords, gravel, shore }
}

/// Fords where a small stream runs: a bar of gravel two tiles thick laid straight across the water, so that a walker
/// (and a route) can cross dry-shod. Streams are chosen by their water (small ones only, where a crossing is natural)
/// and kept apart. Returns the dry tile on each side of every ford.
fn lay_fords(drain: &[u32], flow: &[u8], raw: &[i32], lake: &[bool], height: &mut [u8]) -> Vec<((i32, i32), (i32, i32))> {
    // gravel stands just above the water, as sand
    const BAR: u8 = 97;
    let wet = |height: &[u8], x: i32, y: i32| RiverMap::at(x, y).is_some_and(|i| height[i] < SEA as u8);
    let mut fords: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for i in 0..CELLS {
        let Some(&(fx, fy)) = NEIGHBOURS.get(usize::from(flow[i])) else { continue };
        if drain[i] >= RIVER_AT * FORD_BELOW || height[i] >= SEA as u8 {
            continue;
        }
        let (x, y) = (i as i32 % SIDE, i as i32 / SIDE);
        let apart = if drain[i] < RIVER_AT * SMALL_BELOW { FORD_APART_SMALL } else { FORD_APART_BIG };
        if fords.iter().any(|&((ax, ay), _)| (ax - x).abs().max((ay - y).abs()) < apart) {
            continue;
        }
        // across the stream, at right angles to its current
        let across = (-fy, fx);
        let reach = |sign: i32| -> Option<i32> {
            (1..=FORD_ACROSS + 1).find(|&k| !wet(height, x + sign * k * across.0, y + sign * k * across.1))
        };
        let (Some(a), Some(b)) = (reach(1), reach(-1)) else { continue };
        if a + b - 1 > FORD_ACROSS {
            continue;
        }
        // along a slanting current the bar is a 2 x 2 block at every step, so that its tiles touch by their sides and do
        // not form a chequerboard that meets only at corners
        let thick: Vec<(i32, i32)> = if fx != 0 && fy != 0 { vec![(0, 0), (fx, 0), (0, fy), (fx, fy)] } else { vec![(0, 0), (fx, fy)] };
        let line: Vec<usize> = (-(b - 1)..a)
            .flat_map(|k| thick.iter().map(move |&(ox, oy)| (x + k * across.0 + ox, y + k * across.1 + oy)))
            .filter_map(|(tx, ty)| RiverMap::at(tx, ty))
            .collect();
        // never across a lake or the sea, and both banks must be dry land
        let (lo, hi) = ((x - b * across.0, y - b * across.1), (x + a * across.0, y + a * across.1));
        let dry = |(tx, ty): (i32, i32)| RiverMap::at(tx, ty).is_some_and(|n| height[n] >= SEA as u8);
        if line.iter().any(|&n| lake[n] || raw[n] < SEA) || !dry(lo) || !dry(hi) {
            continue;
        }
        for n in line {
            height[n] = height[n].max(BAR);
        }
        fords.push((lo, hi));
    }
    fords
}
