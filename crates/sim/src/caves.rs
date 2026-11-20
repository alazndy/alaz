//! Walkable caves. Every cave mouth on the surface opens onto a stack of underground levels, each a 64x64 tile grid cut
//! out of the rock: rooms, winding passages, a stair down to the next level, ore veins in the walls, and (on the third
//! level) a sump around the stair that a working pump keeps dry. The levels come from the seed and the mouth's tile,
//! so they are the same on every machine and never saved; only what people change (a dug wall, a fall of rock) is.
//!
//! Walkers in a cave keep their surface position (the mouth they went in by) and carry a second one, in millimetres on
//! the level they are on (`CaveSelf`). Everything here is integer math.

use crate::map::ItemKind;
use crate::rng::hash2;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

pub const CAVE_W: i32 = 64;
pub const CAVE_H: i32 = 64;
/// Levels under a mouth. Level 1 is the first below the surface; the last is the one that floods.
pub const CAVE_LEVELS: u8 = 4;
/// Half the width of a walker, in mm: the body is a square this far each way from its centre.
pub const BODY_R_MM: i64 = 280;
const TILE: i64 = crate::map::TILE_MM;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cell {
    /// Solid rock (diggable except at the very edge).
    Rock = 0,
    Floor = 1,
    /// Wading depth: slow going.
    Shallow = 2,
    /// Swimming depth.
    Deep = 3,
    /// Steps down to the next level.
    Down = 4,
    /// Steps up to the level above.
    Up = 5,
    /// The way out to the surface (level 1 only).
    Exit = 6,
    /// Fallen stone, loose enough to clear by hand.
    Rubble = 7,
}

impl Cell {
    pub fn from_u8(v: u8) -> Cell {
        match v {
            1 => Cell::Floor,
            2 => Cell::Shallow,
            3 => Cell::Deep,
            4 => Cell::Down,
            5 => Cell::Up,
            6 => Cell::Exit,
            7 => Cell::Rubble,
            _ => Cell::Rock,
        }
    }

    pub fn walkable(self) -> bool {
        !matches!(self, Cell::Rock | Cell::Rubble)
    }
}

/// Ore codes in `Level::vein`.
pub const ORE_IRON: u8 = 1;
pub const ORE_COPPER: u8 = 2;
pub const ORE_TIN: u8 = 3;
pub const ORE_COAL: u8 = 4;

pub fn ore_item(code: u8) -> Option<ItemKind> {
    match code {
        ORE_IRON => Some(ItemKind::IronOre),
        ORE_COPPER => Some(ItemKind::CopperOre),
        ORE_TIN => Some(ItemKind::TinOre),
        ORE_COAL => Some(ItemKind::Coal),
        _ => None,
    }
}

/// One level of a cave.
#[derive(Clone, Debug)]
pub struct Level {
    pub cells: Vec<Cell>,
    /// Per tile: ore code of the vein in that wall (0 none).
    pub vein: Vec<u8>,
    /// Per tile: swings of ore a vein still holds.
    pub left: Vec<u8>,
    /// The way back up: the exit (level 1) or the stair up.
    pub entry: (i32, i32),
    /// The stair down, on every level but the last.
    pub down: Option<(i32, i32)>,
    /// Tiles that are under water until a pump dries them (the third level's room round the stair down).
    pub sump: Vec<u16>,
}

impl Level {
    fn blank() -> Level {
        let n = (CAVE_W * CAVE_H) as usize;
        Level { cells: vec![Cell::Rock; n], vein: vec![0; n], left: vec![0; n], entry: (0, 0), down: None, sump: Vec::new() }
    }

    pub fn idx(x: i32, y: i32) -> usize {
        (y * CAVE_W + x) as usize
    }

    pub fn inside(x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < CAVE_W && y < CAVE_H
    }

    pub fn cell(&self, x: i32, y: i32) -> Cell {
        if Self::inside(x, y) { self.cells[Self::idx(x, y)] } else { Cell::Rock }
    }

    pub fn walkable(&self, x: i32, y: i32) -> bool {
        self.cell(x, y).walkable()
    }

    /// The ore (code, swings left) of the vein in a wall tile, if it holds one.
    pub fn ore_at(&self, x: i32, y: i32) -> Option<(u8, u8)> {
        if !Self::inside(x, y) {
            return None;
        }
        let i = Self::idx(x, y);
        (self.vein[i] != 0 && self.left[i] > 0 && self.cells[i] == Cell::Rock).then_some((self.vein[i], self.left[i]))
    }

    /// Can this wall be dug? Rock and rubble, except the outermost ring of the level.
    pub fn diggable(&self, x: i32, y: i32) -> bool {
        x >= 1 && y >= 1 && x < CAVE_W - 1 && y < CAVE_H - 1 && matches!(self.cell(x, y), Cell::Rock | Cell::Rubble)
    }

    /// Is a walker's body at (x, y) mm in the rock? (Its square must lie wholly on walkable tiles.)
    pub fn blocked(&self, x: i64, y: i64) -> bool {
        let r = BODY_R_MM;
        [(-r, -r), (r, -r), (-r, r), (r, r)].iter().any(|&(dx, dy)| !self.walkable((x + dx).div_euclid(TILE) as i32, (y + dy).div_euclid(TILE) as i32))
    }

    /// A walkable tile next to a stair or a doorway to stand on when arriving there.
    pub fn landing(&self, at: (i32, i32)) -> (i32, i32) {
        const RING: [(i32, i32); 8] = [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)];
        for (dx, dy) in RING {
            let (x, y) = (at.0 + dx, at.1 + dy);
            if matches!(self.cell(x, y), Cell::Floor | Cell::Shallow) && !self.blocked(x as i64 * TILE + TILE / 2, y as i64 * TILE + TILE / 2) {
                return (x, y);
            }
        }
        at
    }

    /// Is the straight way from a to b (mm) clear for a body? Sampled every fifth of a metre. A stair is not walked
    /// over on the way (it would be taken), unless it is where the way ends.
    pub fn line_clear(&self, a: (i64, i64), b: (i64, i64)) -> bool {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        let end = (b.0.div_euclid(TILE) as i32, b.1.div_euclid(TILE) as i32);
        let start = (a.0.div_euclid(TILE) as i32, a.1.div_euclid(TILE) as i32);
        let mut d = 0;
        while d < len {
            let (x, y) = (a.0 + dx * d / len, a.1 + dy * d / len);
            if self.blocked(x, y) {
                return false;
            }
            let t = (x.div_euclid(TILE) as i32, y.div_euclid(TILE) as i32);
            if t != end && t != start && self.is_stair(t.0, t.1) {
                return false;
            }
            d += 200;
        }
        !self.blocked(b.0, b.1)
    }

    pub fn is_stair(&self, x: i32, y: i32) -> bool {
        matches!(self.cell(x, y), Cell::Down | Cell::Up | Cell::Exit)
    }

    /// A* over the tiles from `s` to `g` (8-way, no corner cutting; wading costs more, swimming most). The tiles from
    /// the first step to the goal, or None if there is no way.
    pub fn find_path(&self, s: (i32, i32), g: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        if !self.walkable(g.0, g.1) || !self.walkable(s.0, s.1) {
            return None;
        }
        let n = (CAVE_W * CAVE_H) as usize;
        let key = |t: (i32, i32)| Self::idx(t.0, t.1);
        let h = |t: (i32, i32)| {
            let (dx, dy) = ((t.0 - g.0).abs(), (t.1 - g.1).abs());
            10 * (dx + dy) - 6 * dx.min(dy)
        };
        let mut cost = vec![i32::MAX; n];
        let mut from = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        cost[key(s)] = 0;
        open.push(Reverse((h(s), key(s))));
        let mut found = false;
        while let Some(Reverse((_, k))) = open.pop() {
            let cur = ((k as i32) % CAVE_W, (k as i32) / CAVE_W);
            if cur == g {
                found = true;
                break;
            }
            let gc = cost[k];
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)] {
                let nb = (cur.0 + dx, cur.1 + dy);
                // (a stair is taken when stepped on: only as the end of the way)
                if !self.walkable(nb.0, nb.1) || (nb != g && self.is_stair(nb.0, nb.1)) {
                    continue;
                }
                let step = if dx != 0 && dy != 0 {
                    if !self.walkable(cur.0 + dx, cur.1) || !self.walkable(cur.0, cur.1 + dy) {
                        continue;
                    }
                    14
                } else {
                    10
                };
                let wet = match self.cell(nb.0, nb.1) {
                    Cell::Shallow => 6,
                    Cell::Deep => 22,
                    _ => 0,
                };
                let ng = gc + step + wet;
                let nk = key(nb);
                if ng < cost[nk] {
                    cost[nk] = ng;
                    from[nk] = k;
                    open.push(Reverse((ng + h(nb), nk)));
                }
            }
        }
        if !found {
            return None;
        }
        let mut rev = Vec::new();
        let mut k = key(g);
        while k != key(s) {
            rev.push(((k as i32) % CAVE_W, (k as i32) / CAVE_W));
            k = from[k];
        }
        rev.reverse();
        Some(rev)
    }

    /// String-pull a tile route into fewer waypoints: skip ahead while the straight way stays clear for a body.
    pub fn smooth(&self, from: (i64, i64), route: &[(i32, i32)]) -> Vec<(i32, i32)> {
        let center = |t: (i32, i32)| (t.0 as i64 * TILE + TILE / 2, t.1 as i64 * TILE + TILE / 2);
        let mut out = Vec::new();
        let mut at = from;
        let mut i = 0;
        while i < route.len() {
            let mut j = i;
            while j + 1 < route.len() && self.line_clear(at, center(route[j + 1])) {
                j += 1;
            }
            out.push(route[j]);
            at = center(route[j]);
            i = j + 1;
        }
        out
    }

    /// The nearest walkable tile to a tile (searching rings out to 3), for a click that landed in the rock.
    pub fn nearest_walkable(&self, t: (i32, i32)) -> Option<(i32, i32)> {
        if self.walkable(t.0, t.1) {
            return Some(t);
        }
        for r in 1..=3i32 {
            let mut best: Option<(i32, (i32, i32))> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let c = (t.0 + dx, t.1 + dy);
                    if self.walkable(c.0, c.1) && best.is_none_or(|b| dx * dx + dy * dy < b.0) {
                        best = Some((dx * dx + dy * dy, c));
                    }
                }
            }
            if let Some((_, c)) = best {
                return Some(c);
            }
        }
        None
    }
}

/// A whole cave: its levels and what has been changed in them since they were cut.
#[derive(Clone, Debug)]
pub struct Cave {
    pub levels: Vec<Level>,
    /// Bumped whenever a tile changes: renderers rebuild the level.
    pub version: u64,
    /// The sump stands in water (no working pump near the mouth). Derived each tick; not saved.
    pub wet: bool,
    /// Tiles that were changed: (level, x, y) -> (cell, ore swings left).
    pub changed: BTreeMap<(u8, u16, u16), (u8, u8)>,
}

struct Rng {
    base: u64,
    n: i32,
}

impl Rng {
    fn next(&mut self) -> u64 {
        self.n += 1;
        hash2(self.base, self.n, 0x51)
    }
    /// Uniform in lo..hi.
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo).max(1) as u64) as i32
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
}

fn carve(cells: &mut [Cell], x: i32, y: i32) {
    if x >= 3 && y >= 3 && x < CAVE_W - 3 && y < CAVE_H - 3 {
        cells[Level::idx(x, y)] = Cell::Floor;
    }
}

fn carve_ellipse(cells: &mut [Cell], cx: i32, cy: i32, rx: i32, ry: i32) {
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            if dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry {
                carve(cells, cx + dx, cy + dy);
            }
        }
    }
}

/// A winding passage from one point to another, a metre or two wide.
fn wander(cells: &mut [Cell], rng: &mut Rng, from: (i32, i32), to: (i32, i32)) {
    let (mut x, mut y) = from;
    for _ in 0..600 {
        if (x, y) == to {
            break;
        }
        let (rx, ry) = (to.0 - x, to.1 - y);
        let horiz = if rx.abs() > ry.abs() { rng.chance(72) } else { !rng.chance(72) };
        if rng.chance(14) {
            let side = if rng.chance(50) { 1 } else { -1 };
            if horiz { y += side } else { x += side }
        } else if horiz && rx != 0 {
            x += rx.signum();
        } else if ry != 0 {
            y += ry.signum();
        } else {
            x += rx.signum();
        }
        x = x.clamp(3, CAVE_W - 4);
        y = y.clamp(3, CAVE_H - 4);
        carve(cells, x, y);
        if rng.chance(55) {
            carve(cells, x + 1, y);
        }
        if rng.chance(55) {
            carve(cells, x, y + 1);
        }
    }
}

/// Distances in steps from `from` over floor-like tiles (-1 where unreachable).
fn flood(cells: &[Cell], from: (i32, i32)) -> Vec<i32> {
    let mut dist = vec![-1; cells.len()];
    let mut q = VecDeque::new();
    dist[Level::idx(from.0, from.1)] = 0;
    q.push_back(from);
    while let Some((x, y)) = q.pop_front() {
        let d = dist[Level::idx(x, y)];
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if Level::inside(nx, ny) && cells[Level::idx(nx, ny)].walkable() && dist[Level::idx(nx, ny)] < 0 {
                dist[Level::idx(nx, ny)] = d + 1;
                q.push_back((nx, ny));
            }
        }
    }
    dist
}

/// The ore a vein on depth `depth` (0 = level 1) turns out to be: the same mix the old chambers gave, without the
/// share that was plain stone.
pub(crate) fn vein_distribution(depth: u8) -> &'static [(u8, u64)] {
    match depth {
        0 => &[(ORE_IRON, 35)],
        1 => &[(ORE_IRON, 30), (ORE_COPPER, 30)],
        2 => &[(ORE_COPPER, 25), (ORE_TIN, 25), (ORE_IRON, 15), (ORE_COAL, 20)],
        _ => &[(ORE_TIN, 30), (ORE_COPPER, 20), (ORE_IRON, 15), (ORE_COAL, 35)],
    }
}

pub(crate) fn vein_ore(depth: u8, roll: u64) -> u8 {
    let table = vein_distribution(depth);
    let total: u64 = table.iter().map(|t| t.1).sum();
    let mut r = roll % total;
    for &(code, w) in table {
        if r < w {
            return code;
        }
        r -= w;
    }
    table[0].0
}

fn gen_level(seed: u64, tx: i32, ty: i32, lv: u8, entry: (i32, i32)) -> Level {
    let mut l = Level::blank();
    let mut rng = Rng { base: hash2(seed ^ 0xCA7E_BEEF ^ (lv as u64) << 40, tx, ty), n: 0 };
    let mut rooms: Vec<(i32, i32)> = vec![entry];
    carve_ellipse(&mut l.cells, entry.0, entry.1, rng.range(3, 5), rng.range(3, 5));
    for _ in 0..rng.range(7, 10) {
        let (cx, cy) = (rng.range(9, CAVE_W - 9), rng.range(9, CAVE_H - 9));
        carve_ellipse(&mut l.cells, cx, cy, rng.range(3, 7), rng.range(3, 7));
        rooms.push((cx, cy));
    }
    for i in 1..rooms.len() {
        wander(&mut l.cells, &mut rng, rooms[i - 1], rooms[i]);
    }
    // a couple of loops, so the way back is not always the way in
    for _ in 0..2 {
        let (a, b) = (rng.range(0, rooms.len() as i32) as usize, rng.range(0, rooms.len() as i32) as usize);
        if a != b {
            wander(&mut l.cells, &mut rng, rooms[a], rooms[b]);
        }
    }
    // one pass of smoothing: spikes of floor go, notches in the wall fill
    let before = l.cells.clone();
    for y in 3..CAVE_H - 3 {
        for x in 3..CAVE_W - 3 {
            let n = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))).filter(|&(dx, dy)| (dx, dy) != (0, 0) && before[Level::idx(x + dx, y + dy)].walkable()).count();
            let i = Level::idx(x, y);
            if before[i].walkable() && n <= 2 {
                l.cells[i] = Cell::Rock;
            } else if !before[i].walkable() && n >= 6 {
                l.cells[i] = Cell::Floor;
            }
        }
    }
    carve_ellipse(&mut l.cells, entry.0, entry.1, 2, 2);
    // whatever the passages do not reach is solid rock
    let dist = flood(&l.cells, entry);
    for i in 0..l.cells.len() {
        if l.cells[i].walkable() && dist[i] < 0 {
            l.cells[i] = Cell::Rock;
        }
    }
    l.entry = entry;
    // the stair down: the farthest roomy spot from the way in
    if lv + 1 < CAVE_LEVELS {
        let open = |x: i32, y: i32| (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))).filter(|&(dx, dy)| l.cells[Level::idx(x + dx, y + dy)].walkable()).count();
        let mut best: Option<(i32, i32, i32)> = None;
        for y in 4..CAVE_H - 4 {
            for x in 4..CAVE_W - 4 {
                let d = dist[Level::idx(x, y)];
                if d > 0 && l.cells[Level::idx(x, y)] == Cell::Floor && open(x, y) >= 8 && best.is_none_or(|b| d > b.0) {
                    best = Some((d, x, y));
                }
            }
        }
        // (a level too cramped for a roomy spot takes any floor)
        if best.is_none() {
            for y in 4..CAVE_H - 4 {
                for x in 4..CAVE_W - 4 {
                    let d = dist[Level::idx(x, y)];
                    if d > 0 && best.is_none_or(|b| d > b.0) {
                        best = Some((d, x, y));
                    }
                }
            }
        }
        if let Some((_, x, y)) = best {
            l.cells[Level::idx(x, y)] = Cell::Down;
            l.down = Some((x, y));
        }
    }
    l.cells[Level::idx(entry.0, entry.1)] = if lv == 0 { Cell::Exit } else { Cell::Up };
    // wading pools in the middle levels
    if lv == 1 || lv == 2 {
        let room = rooms[rng.range(2, rooms.len() as i32) as usize];
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                let (x, y) = (room.0 + dx, room.1 + dy);
                let far = |p: (i32, i32)| (p.0 - x).abs().max((p.1 - y).abs()) > 3;
                if dx * dx + dy * dy <= 5 && Level::inside(x, y) && l.cells[Level::idx(x, y)] == Cell::Floor && far(entry) && l.down.is_none_or(far) {
                    l.cells[Level::idx(x, y)] = Cell::Shallow;
                }
            }
        }
    }
    // the sump: the room round the stair down on the third level
    if lv == 2
        && let Some((sx, sy)) = l.down {
            for dy in -4..=4i32 {
                for dx in -4..=4i32 {
                    let (x, y) = (sx + dx, sy + dy);
                    if dx * dx + dy * dy <= 18 && Level::inside(x, y) && matches!(l.cells[Level::idx(x, y)], Cell::Floor | Cell::Shallow) {
                        l.sump.push(Level::idx(x, y) as u16);
                        l.cells[Level::idx(x, y)] = Cell::Deep;
                    }
                }
            }
        }
    // ore veins in the walls beside open ground
    let mut exposed: Vec<usize> = Vec::new();
    for y in 2..CAVE_H - 2 {
        for x in 2..CAVE_W - 2 {
            if l.cells[Level::idx(x, y)] == Cell::Rock && [(1, 0), (0, 1), (-1, 0), (0, -1)].iter().any(|&(dx, dy)| matches!(l.cells[Level::idx(x + dx, y + dy)], Cell::Floor | Cell::Shallow)) {
                exposed.push(Level::idx(x, y));
            }
        }
    }
    let want = 9 + 2 * lv as usize;
    let mut placed = 0;
    for _ in 0..want * 8 {
        if placed >= want || exposed.is_empty() {
            break;
        }
        let i = exposed[(rng.next() % exposed.len() as u64) as usize];
        if l.vein[i] != 0 {
            continue;
        }
        let code = vein_ore(lv, rng.next());
        l.vein[i] = code;
        l.left[i] = 3 + (rng.next() % 3) as u8;
        placed += 1;
        // a vein runs on into the next bit of wall now and then
        if rng.chance(55) {
            let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][(rng.next() % 4) as usize];
            let (x, y) = ((i as i32 % CAVE_W) + dx, (i as i32 / CAVE_W) + dy);
            if x >= 2 && y >= 2 && x < CAVE_W - 2 && y < CAVE_H - 2 && l.cells[Level::idx(x, y)] == Cell::Rock && l.vein[Level::idx(x, y)] == 0 {
                l.vein[Level::idx(x, y)] = code;
                l.left[Level::idx(x, y)] = 2 + (rng.next() % 3) as u8;
            }
        }
    }
    l
}

impl Cave {
    /// Cut the cave under the mouth on tile (tx, ty) of a world with this seed.
    pub fn generate(seed: u64, tx: i32, ty: i32) -> Cave {
        let mut levels: Vec<Level> = Vec::new();
        let mut entry = (CAVE_W / 2, CAVE_H - 9);
        for lv in 0..CAVE_LEVELS {
            let l = gen_level(seed, tx, ty, lv, entry);
            if let Some(d) = l.down {
                entry = d;
            }
            levels.push(l);
        }
        Cave { levels, version: 0, wet: true, changed: BTreeMap::new() }
    }

    /// Level `level` (1-based).
    pub fn level(&self, level: u8) -> Option<&Level> {
        self.levels.get((level as usize).wrapping_sub(1))
    }

    /// Change a tile and remember it. `left` is the ore swings a vein still holds.
    pub fn set_tile(&mut self, level: u8, x: i32, y: i32, cell: Cell, left: u8) {
        let Some(l) = self.levels.get_mut((level as usize).wrapping_sub(1)) else { return };
        if !Level::inside(x, y) {
            return;
        }
        let i = Level::idx(x, y);
        l.cells[i] = cell;
        l.left[i] = left;
        if left == 0 {
            l.vein[i] = 0;
        }
        self.changed.insert((level, x as u16, y as u16), (cell as u8, left));
        self.version += 1;
    }

    /// Put saved changes back on a freshly cut cave.
    pub fn apply_changes(&mut self, changes: &[(u8, u16, u16, u8, u8)]) {
        for &(level, x, y, cell, left) in changes {
            self.set_tile(level, x as i32, y as i32, Cell::from_u8(cell), left);
        }
    }

    /// Set the sump dry or flooded.
    pub fn set_wet(&mut self, wet: bool) {
        if self.wet == wet {
            return;
        }
        self.wet = wet;
        let to = if wet { Cell::Deep } else { Cell::Floor };
        if let Some(l) = self.levels.get_mut(2) {
            for &i in &l.sump.clone() {
                l.cells[i as usize] = to;
            }
        }
        self.version += 1;
    }
}

/// A walker underground: where it went in, the level it is on, where it stands there (mm), and its light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaveSelf {
    /// The mouth it went in by (a node index).
    pub node: u32,
    /// 1 = the first level under the mouth.
    pub level: u8,
    pub x: i64,
    pub y: i64,
    /// Ticks of light left, and what the light was when it was lit (a torch, a lamp).
    pub light: u16,
    pub full: u16,
    /// Ticks spent in the dark so far.
    pub dark: u16,
    /// Stairs let the walker be for this many ticks (it has only just come by one).
    pub cool: u16,
    /// Making its own way out: after each stair it goes on to the next, to the surface.
    pub leaving: bool,
}

impl CaveSelf {
    /// Natural caves store the elevation in the same byte older caves used for the level number.
    pub fn elevation_m(&self) -> Option<i16> {
        (self.level > CAVE_LEVELS).then_some(i16::from(self.level) - 100)
    }

    pub(crate) fn set_elevation(&mut self, metres: i16) {
        self.level = (metres + 100) as u8;
    }

    pub(crate) fn natural_tile(&self) -> Option<crate::cave_terrain::CaveTile> {
        let (x, y) = self.tile();
        self.elevation_m().map(|z| crate::cave_terrain::CaveTile { x, y, z })
    }

    pub fn tile(&self) -> (i32, i32) {
        (self.x.div_euclid(TILE) as i32, self.y.div_euclid(TILE) as i32)
    }

    /// Depth in the old sense: 0 on the first level.
    pub fn depth(&self) -> u8 {
        self.elevation_m().map_or_else(|| self.level.saturating_sub(1), Self::natural_depth_at)
    }

    pub(crate) fn natural_depth_at(z: i16) -> u8 {
        match z {
            ..=-60 => 3,
            -59..=-35 => 2,
            -34..=-16 => 1,
            _ => 0,
        }
    }
}

/// What a step made of the way: where the walker ends up, and whether it got anywhere.
pub fn slide(l: &Level, x: i64, y: i64, nx: i64, ny: i64) -> Option<(i64, i64)> {
    if !l.blocked(nx, ny) {
        return Some((nx, ny));
    }
    if nx != x && !l.blocked(nx, y) {
        return Some((nx, y));
    }
    if ny != y && !l.blocked(x, ny) {
        return Some((x, ny));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caves_are_the_same_everywhere_and_connected() {
        let a = Cave::generate(7, 40, 50);
        let b = Cave::generate(7, 40, 50);
        let c = Cave::generate(7, 41, 50);
        assert_eq!(a.levels.len(), CAVE_LEVELS as usize);
        for (la, lb) in a.levels.iter().zip(&b.levels) {
            assert_eq!(la.cells, lb.cells);
            assert_eq!(la.vein, lb.vein);
        }
        assert_ne!(a.levels[0].cells, c.levels[0].cells);
        for (i, l) in a.levels.iter().enumerate() {
            // every stair can be reached from the way in
            let dist = flood(&l.cells, l.entry);
            let floor = l.cells.iter().filter(|c| c.walkable()).count();
            assert!(floor > 400, "level {i} has room to walk: {floor}");
            if let Some(d) = l.down {
                assert!(dist[Level::idx(d.0, d.1)] > 0, "stair down reachable on level {i}");
                // and the next level starts at the same spot
                assert_eq!(a.levels[i + 1].entry, d);
            } else {
                assert_eq!(i, CAVE_LEVELS as usize - 1);
            }
            assert!(l.cells.iter().zip(&dist).all(|(c, &d)| !c.walkable() || d >= 0), "no walkable tile cut off");
            assert!(l.vein.iter().any(|&v| v != 0), "level {i} has ore");
        }
        assert_eq!(a.levels[0].cells[Level::idx(a.levels[0].entry.0, a.levels[0].entry.1)], Cell::Exit);
        assert!(!a.levels[2].sump.is_empty());
    }

    #[test]
    fn paths_and_walls() {
        let c = Cave::generate(3, 12, 90);
        let l = &c.levels[0];
        let d = l.down.unwrap();
        let path = l.find_path(l.entry, d).expect("a way to the stair");
        assert_eq!(*path.last().unwrap(), d);
        // a body does not fit in the rock
        assert!(l.blocked(500, 500));
        let (ex, ey) = (l.entry.0 as i64 * TILE + 500, l.entry.1 as i64 * TILE + 500);
        assert!(!l.blocked(ex, ey));
    }
}
