//! Wild animals that wander and can be tamed with food.

use crate::agents::AgentKind;
use crate::map::{Map, Terrain, TILE_MM};
use crate::rng::hash2;
use serde::{Deserialize, Serialize};

pub const EPOCH_TICKS: u64 = 600;
const WANDER_STEP_MM: i64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WildSpecies {
    Dog,
    Ox,
    Horse,
    /// Prey: cannot be tamed, flees from people, gives meat.
    Deer,
    /// Rare predator: stalks people, shies away from fire.
    Wolf,
}

impl WildSpecies {
    /// The worker kind it becomes once tamed (None for prey).
    pub fn tamed(self) -> Option<AgentKind> {
        match self {
            WildSpecies::Dog => Some(AgentKind::Dog),
            WildSpecies::Ox => Some(AgentKind::Ox),
            WildSpecies::Horse => Some(AgentKind::Horse),
            WildSpecies::Deer | WildSpecies::Wolf => None,
        }
    }

    /// Can be killed with a spear for meat.
    pub fn huntable(self) -> bool {
        matches!(self, WildSpecies::Deer | WildSpecies::Wolf)
    }
}

/// Prey notices people this close and runs away from them.
const FLEE_RADIUS_MM: i64 = 7_000;
const FLEE_STEP_MM: i64 = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wild {
    pub id: u32,
    pub species: WildSpecies,
    pub x: i64,
    pub y: i64,
    /// 0..=100; a tamed animal joins at 100. Animals being tamed stand still.
    pub trust: i32,
    tx: i64,
    ty: i64,
    epoch: u64,
}

/// Deterministic population from the seed. Spawn area stays calm (nothing within `calm_tiles` of spawn).
/// What wild animal (if any) the world starts with on a tile.
pub fn wild_at(seed: u64, map: &Map, tx: i32, ty: i32, spawn_tile: (i32, i32)) -> Option<WildSpecies> {
    let r = hash2(seed ^ 0x4444, tx, ty) % 1000;
    let far = (tx - spawn_tile.0).abs().max((ty - spawn_tile.1).abs()) >= 30;
    Some(match map.terrain(tx, ty) {
        Some(Terrain::Forest) if r < 3 => WildSpecies::Dog,
        Some(Terrain::Forest) if (7..10).contains(&r) && far => WildSpecies::Wolf,
        Some(Terrain::Forest) if r < 7 => WildSpecies::Deer,
        Some(Terrain::Grass) if r < 2 => WildSpecies::Ox,
        Some(Terrain::Grass) if r == 2 => WildSpecies::Horse,
        Some(Terrain::Grass) if r < 5 => WildSpecies::Deer,
        _ => return None,
    })
}

pub fn generate(seed: u64, map: &Map, spawn_tile: (i32, i32), calm_tiles: i32) -> Vec<Wild> {
    if map.lazy() {
        // a world made in chunks gets its animals chunk by chunk
        return Vec::new();
    }
    let mut out = crate::map::par_rows(map.w, map.h, |y0, y1| {
        let mut out = Vec::new();
        for ty in y0..y1 {
        for tx in 0..map.w {
            if (tx - spawn_tile.0).abs().max((ty - spawn_tile.1).abs()) < calm_tiles {
                continue;
            }
            let Some(species) = wild_at(seed, map, tx, ty, spawn_tile) else { continue };
            let (x, y) = (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
            out.push(Wild { id: 0, species, x, y, trust: 0, tx: x, ty: y, epoch: 0 });
        }
        }
        out
    });
    for (i, w) in out.iter_mut().enumerate() {
        w.id = i as u32;
    }
    out
}

impl Wild {
    /// Where it is heading, in mm from where it stands (zero when it rests).
    pub fn heading_mm(&self) -> (i64, i64) {
        (self.tx - self.x, self.ty - self.y)
    }

    /// Stop wandering toward an old target (used after being driven by the world).
    pub(crate) fn settle(&mut self) {
        self.tx = self.x;
        self.ty = self.y;
    }

    pub fn new(id: u32, species: WildSpecies, x: i64, y: i64) -> Self {
        Self { id, species, x, y, trust: 0, tx: x, ty: y, epoch: 0 }
    }

    /// Random walk: a new nearby target each epoch, slow steps, never into water.
    pub fn wander(&mut self, seed: u64, tick: u64, map: &Map, people: &[(i64, i64)]) {
        if self.species == WildSpecies::Deer {
            let nearest = people
                .iter()
                .map(|&(px, py)| ((px - self.x).pow(2) + (py - self.y).pow(2), px, py))
                .min();
            if let Some((d2, px, py)) = nearest
                && d2 < FLEE_RADIUS_MM * FLEE_RADIUS_MM {
                    // run straight away from the closest person, stop at water or the map edge
                    let (dx, dy) = (self.x - px, self.y - py);
                    let len = ((dx * dx + dy * dy) as u64).isqrt().max(1) as i64;
                    let (nx, ny) = (self.x + dx * FLEE_STEP_MM / len, self.y + dy * FLEE_STEP_MM / len);
                    if !matches!(map.terrain_mm(nx, ny), None | Some(Terrain::Water)) && !sheer(map, self.x, self.y, nx, ny) {
                        self.x = nx;
                        self.y = ny;
                    }
                    self.tx = self.x;
                    self.ty = self.y;
                    return;
                }
        }
        if self.trust > 0 {
            if tick.is_multiple_of(300) {
                self.trust -= 1;
            }
            return;
        }
        let epoch = tick / EPOCH_TICKS;
        if epoch != self.epoch {
            self.epoch = epoch;
            let h = hash2(seed ^ (self.id as u64).wrapping_mul(0x9E37), epoch as i32, 17);
            let dx = ((h & 0xFF) as i64 - 128) * 40;
            let dy = (((h >> 8) & 0xFF) as i64 - 128) * 40;
            self.tx = self.x + dx;
            self.ty = self.y + dy;
            // thirsty for a whole spell of epochs, long enough to walk the distance: it heads for the nearest bank and
            // rests there (the nearest bank from the bank is the bank itself)
            let spell = hash2(seed ^ 0xD21C ^ (self.id as u64).wrapping_mul(0x9E37), (epoch / DRINK_SPELL_EPOCHS) as i32, 3);
            if self.species != WildSpecies::Wolf && spell % DRINK_ONE_IN == 0 {
                if let Some((bx, by)) = nearest_bank(map, self.x, self.y, DRINK_RANGE_TILES) {
                    self.tx = bx;
                    self.ty = by;
                }
            }
        }
        let (dx, dy) = (self.tx - self.x, self.ty - self.y);
        let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        if len == 0 {
            return;
        }
        let step = WANDER_STEP_MM.min(len);
        let (nx, ny) = (self.x + dx * step / len, self.y + dy * step / len);
        match map.terrain_mm(nx, ny) {
            None | Some(Terrain::Water) => {
                self.tx = self.x;
                self.ty = self.y;
            }
            // animals keep away from cliff edges and do not scale rock walls
            _ if sheer(map, self.x, self.y, nx, ny) => {
                self.tx = self.x;
                self.ty = self.y;
            }
            _ => {
                self.x = nx;
                self.y = ny;
            }
        }
    }
}

/// An animal is thirsty for a spell of this many epochs at a time: a walk of 20 tiles takes about four epochs.
const DRINK_SPELL_EPOCHS: u64 = 6;
/// One spell in this many is a thirsty one.
const DRINK_ONE_IN: u64 = 3;
/// How far, in tiles, it looks for water.
const DRINK_RANGE_TILES: i32 = 20;

/// The centre of the dry tile nearest to (x, y) that touches water, within `range` tiles: where an animal stands to
/// drink. Rings are scanned from the inside out in a fixed order, so every peer picks the same bank.
pub fn nearest_bank(map: &Map, x: i64, y: i64, range: i32) -> Option<(i64, i64)> {
    let (cx, cy) = ((x / TILE_MM) as i32, (y / TILE_MM) as i32);
    let dry = |tx: i32, ty: i32| map.terrain(tx, ty).is_some_and(|t| t != Terrain::Water);
    let wet = |tx: i32, ty: i32| map.terrain(tx, ty) == Some(Terrain::Water);
    for r in 0..=range {
        for ty in cy - r..=cy + r {
            for tx in cx - r..=cx + r {
                if (tx - cx).abs().max((ty - cy).abs()) != r || !dry(tx, ty) {
                    continue;
                }
                if wet(tx + 1, ty) || wet(tx - 1, ty) || wet(tx, ty + 1) || wet(tx, ty - 1) {
                    return Some((tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2));
                }
            }
        }
    }
    None
}

/// Is the ground between two close points a drop or a wall no animal would take?
fn sheer(map: &Map, x0: i64, y0: i64, x1: i64, y1: i64) -> bool {
    let len = (((x1 - x0).pow(2) + (y1 - y0).pow(2)) as u64).isqrt() as i64;
    let rise = (crate::map::elevation_at_mm(map.seed, x1, y1) - crate::map::elevation_at_mm(map.seed, x0, y0)) as i64;
    rise.abs() > len * 3 / 2 + 20
}

#[cfg(test)]
mod drink_tests {
    use super::*;

    #[test]
    fn animals_walk_to_the_bank_to_drink() {
        let seed = crate::map::tag_seed_new_world(7, 256);
        let map = crate::world::World::with_size(seed, 256, 256).map;
        let near_bank = |x: i64, y: i64| {
            let (cx, cy) = ((x / TILE_MM) as i32, (y / TILE_MM) as i32);
            (-1..=1).any(|d| map.terrain(cx + d, cy) == Some(Terrain::Water) || map.terrain(cx, cy + d) == Some(Terrain::Water))
        };
        // a dry tile 8 to 15 tiles from the nearest bank
        let (sx, sy) = (0..256i32)
            .flat_map(|y| (0..256i32).map(move |x| (x, y)))
            .find(|&(x, y)| {
                map.terrain(x, y).is_some_and(|t| t != Terrain::Water)
                    && nearest_bank(&map, x as i64 * TILE_MM, y as i64 * TILE_MM, 20).is_some_and(|(bx, by)| {
                        let d = ((bx / TILE_MM) as i32 - x).abs().max(((by / TILE_MM) as i32 - y).abs());
                        (8..=15).contains(&d)
                    })
            })
            .expect("land a short walk from water");
        let start = (sx as i64 * TILE_MM + TILE_MM / 2, sy as i64 * TILE_MM + TILE_MM / 2);
        let mut drank = 0;
        for id in 0..40u32 {
            let mut deer = Wild::new(id, WildSpecies::Ox, start.0, start.1);
            let mut at_bank = false;
            for tick in 0..EPOCH_TICKS * 48 {
                deer.wander(seed, tick, &map, &[]);
                at_bank |= near_bank(deer.x, deer.y);
            }
            drank += usize::from(at_bank);
        }
        assert!(drank >= 15, "oxen that reached the water: {drank} of 40");
    }
}
