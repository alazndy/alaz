//! Surfaces and elements: what the ground is like underfoot (mud, puddles, ice) and what burns on it.
//!
//! Wet ground is a pure function of the sky's recent past (`time::ground_wet`) and of the tile, so it needs no
//! saved state: rain soaks the grass to mud and fills the flat hollows, the cold turns puddles to ice, the sun and
//! wind dry it all again. Only fire is real state: a small sorted map of tiles that burn or lie in ash.
//!
//! Fire is kept on a short leash: it needs dry fuel (grass, undergrowth), it spreads with a chance that falls off
//! steeply as the ground gets damp, it never crosses water, sand, rock, ash or a building, rain puts it out, and a
//! tile burns for seconds. The branching is below the point where a fire could run away on its own (the wind only
//! pushes it along), and a hard cap on burning tiles guards the rest.

use crate::command::AgentId;
use crate::map::{Map, Terrain, TILE_MM};
use crate::time::{self, Weather};
use crate::world::{Event, Task, World};

/// What the ground of a tile is like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    Dry = 0,
    /// Wet earth: slows the going (soles with grip help).
    Mud = 1,
    /// Standing water in a flat hollow: a fire will not start or spread on it.
    Puddle = 2,
    /// A frozen puddle: runners slip.
    Ice = 3,
    /// What a fire left behind; the grass comes back in time.
    Ash = 4,
    /// Burning now.
    Fire = 5,
}

/// A tile that burns or lies in ash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub fire: bool,
    pub born: u64,
    pub until: u64,
}

/// Fire is advanced every this many ticks (half a second).
const FIRE_STEP: u64 = 30;
/// A tile burns for seven to fourteen seconds.
const FIRE_TICKS: u64 = 420;
/// Ash lies for about a day, then the grass has grown back.
const ASH_TICKS: u64 = time::TICKS_PER_DAY;
/// No more than this many tiles burn at once, anywhere: whatever the dice say, the world cannot go up in flames.
pub const MAX_FIRES: usize = 240;
/// Chance (of 10000) that a burning tile sets one dry neighbour alight in one step, with fuel and dryness at their best.
const SPREAD_BP: u64 = 300;
/// Damage per step to someone standing in flames (health runs to 10000).
const FIRE_HURT: i32 = 300;
/// Mud takes this share (permille) off walking speed.
pub const MUD_SLOW: i32 = 280;
/// Standing water takes this much.
pub const PUDDLE_SLOW: i32 = 100;
/// Chance (permille) per tenth of a second that a runner on ice goes down.
const SLIP_PERMILLE: u64 = 30;
/// Wet ground below this (permille) is only damp: no mud, no puddles anywhere.
const QUIET_BELOW: i32 = 390;

/// The ground as the weather has left it, at one moment. Cheap to make (nine hashes) and `Copy`, so walkers carry one.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub seed: u64,
    /// How wet the ground is, permille.
    pub wet: i32,
    pub sky: Weather,
    /// Air temperature, tenths of a degree.
    pub ambient_x10: i32,
}

impl Ground {
    pub fn at(seed: u64, tick: u64, days: u64) -> Ground {
        Ground { seed, wet: time::ground_wet(seed, tick, days), sky: time::weather(seed, tick, days), ambient_x10: time::ambient_x10(tick, days) }
    }

    /// Is the ground wet nowhere enough to matter?
    pub fn quiet(&self) -> bool {
        self.wet < QUIET_BELOW
    }

    pub fn freezing(&self) -> bool {
        self.ambient_x10 < 0
    }

    /// How wet this tile is, permille: the hollows hold more than the knolls.
    fn score(&self, tx: i32, ty: i32) -> i32 {
        let n = crate::rng::value_noise(self.seed ^ 0x50FF, tx, ty, 5) as i32;
        self.wet * (500 + n * 2) / 1000
    }

    /// A flat place, where water stands (not a slope).
    fn flat(&self, tx: i32, ty: i32) -> bool {
        let e = |x, y| crate::map::elevation_mm(self.seed, x, y);
        let e0 = e(tx, ty);
        (e(tx + 1, ty) - e0).abs() + (e(tx, ty + 1) - e0).abs() <= 150
    }

    /// The surface the weather alone makes of a tile (never fire or ash).
    pub fn weather_surface(&self, map: &Map, tx: i32, ty: i32) -> Surface {
        if self.quiet() {
            return Surface::Dry;
        }
        if !matches!(map.terrain(tx, ty), Some(Terrain::Grass | Terrain::Forest)) {
            return Surface::Dry;
        }
        let s = self.score(tx, ty);
        if self.freezing() {
            return if s >= 700 && self.flat(tx, ty) { Surface::Ice } else { Surface::Dry };
        }
        if s >= 800 && self.flat(tx, ty) {
            Surface::Puddle
        } else if s >= 480 {
            Surface::Mud
        } else {
            Surface::Dry
        }
    }

    /// How well a tile burns, permille (0: not at all). Dry grass and brush, in warm and clear weather, after a dry spell.
    pub fn fuel(&self, map: &Map, tx: i32, ty: i32) -> i32 {
        if self.sky != Weather::Clear || self.ambient_x10 < 50 || self.wet >= 300 {
            return 0;
        }
        let base = match map.terrain(tx, ty) {
            Some(Terrain::Grass) => 1000,
            Some(Terrain::Forest) => 750,
            _ => return 0,
        };
        // patches of the meadow dry out sooner than others
        let n = crate::rng::value_noise(self.seed ^ 0xD7E1, tx, ty, 7) as i32;
        let patch = 650 + n * 14 / 10;
        base * (1000 - self.wet * 3) / 1000 * patch.min(1000) / 1000
    }

    /// The speed a walker keeps on this tile, permille of its pace.
    pub fn walk_permille(&self, map: &Map, tx: i32, ty: i32, grip: i32) -> i32 {
        if self.quiet() {
            return 1000;
        }
        match self.weather_surface(map, tx, ty) {
            Surface::Mud => 1000 - crate::rules::Gear::cut(MUD_SLOW, grip),
            Surface::Puddle => 1000 - PUDDLE_SLOW,
            _ => 1000,
        }
    }
}

impl World {
    /// The ground as the weather has left it now.
    pub fn ground(&self) -> Ground {
        Ground::at(self.seed, self.tick, self.settings.season_days as u64)
    }

    /// What the ground of a tile is like now (fire and ash first, then what the weather made of it).
    pub fn surface_at(&self, tx: i32, ty: i32) -> Surface {
        if let Some(b) = self.burn.get(&(tx, ty)) {
            return if b.fire { Surface::Fire } else { Surface::Ash };
        }
        self.ground().weather_surface(&self.map, tx, ty)
    }

    /// `surface_at` as the renderer reads it: the surface in the low three bits and, for fire and ash, how far
    /// along it is (0..=3) above them.
    pub fn surface_code(&self, ground: &Ground, tx: i32, ty: i32) -> u8 {
        if let Some(b) = self.burn.get(&(tx, ty)) {
            let span = (b.until - b.born).max(1);
            let age = ((self.tick.saturating_sub(b.born)) * 4 / span).min(3) as u8;
            return (if b.fire { Surface::Fire } else { Surface::Ash }) as u8 | age << 3;
        }
        ground.weather_surface(&self.map, tx, ty) as u8
    }

    /// Every tile that burns or lies in ash, with its state, in map order.
    pub fn burn_iter(&self) -> impl Iterator<Item = (&(i32, i32), &Burn)> {
        self.burn.iter()
    }

    /// Tiles on fire now, in map order.
    pub fn burning(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.burn.iter().filter(|(_, b)| b.fire).map(|(&k, _)| k)
    }

    /// Is anything burning or lying in ash?
    pub fn has_burn(&self) -> bool {
        !self.burn.is_empty()
    }

    /// Changes whenever a tile catches, goes out or its ash clears (not saved): renderers refresh on it.
    pub fn burn_version(&self) -> u64 {
        self.burn_seq
    }

    /// Set a tile alight if it can burn now. False if it cannot (wet, bare, built on, already burning) or the world
    /// is at its limit of flames.
    pub fn ignite(&mut self, tx: i32, ty: i32) -> bool {
        let g = self.ground();
        self.ignite_with(&g, tx, ty)
    }

    fn ignite_with(&mut self, g: &Ground, tx: i32, ty: i32) -> bool {
        if self.burn.contains_key(&(tx, ty)) || self.obstacles.blocked(tx, ty) || g.fuel(&self.map, tx, ty) == 0 {
            return false;
        }
        if self.burn.values().filter(|b| b.fire).count() >= MAX_FIRES {
            return false;
        }
        let life = FIRE_TICKS / 2 + crate::rng::hash2(self.seed ^ 0xF1AE, tx, ty ^ (self.tick / FIRE_STEP) as i32) % FIRE_TICKS;
        self.burn.insert((tx, ty), Burn { fire: true, born: self.tick, until: self.tick + life });
        self.burn_seq += 1;
        true
    }

    /// Mud, ice and fire at work, once per tick.
    pub(crate) fn run_surface(&mut self) {
        let t = self.tick;
        if t % 10 == 0 {
            self.run_slips();
        }
        if t % 60 == 0 {
            self.run_sparks();
        }
        if t % FIRE_STEP != 0 || self.burn.is_empty() {
            return;
        }
        let g = self.ground();
        let days = self.settings.season_days as u64;
        // ash clears, and the grass is back
        let before = self.burn.len();
        self.burn.retain(|_, b| b.fire || b.until > t);
        if self.burn.len() != before {
            self.burn_seq += 1;
        }
        let fires: Vec<(i32, i32)> = self.burning().collect();
        if fires.is_empty() {
            return;
        }
        let (heading, strength) = time::wind(self.seed, t, days);
        let down = ((heading + 785) / 1571 % 4) as usize;
        const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let wet_out = g.sky != Weather::Clear || g.wet >= 300;
        let salt = self.seed ^ 0xF1A3 ^ (t / FIRE_STEP).wrapping_mul(0x9E37_79B9);
        let mut lit: Vec<(i32, i32)> = Vec::new();
        let mut out: Vec<(i32, i32)> = Vec::new();
        for &(tx, ty) in &fires {
            let b = self.burn[&(tx, ty)];
            // rain puts flames out quickly; the rest burn down
            if t >= b.until || (wet_out && crate::rng::hash2(salt ^ 0x77, tx, ty) % 100 < 35) {
                out.push((tx, ty));
                continue;
            }
            for (i, (dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (tx + dx, ty + dy);
                if self.burn.contains_key(&(nx, ny)) || self.obstacles.blocked(nx, ny) {
                    continue;
                }
                let fuel = g.fuel(&self.map, nx, ny) as u64;
                if fuel == 0 {
                    continue;
                }
                // the wind carries flames along with it and holds them back against it
                let push = if i == down { 1000 + strength as i64 * 13 / 10 } else if i == (down + 2) % 4 { (1000 - strength as i64 * 6 / 10).max(300) } else { 1000 };
                let p = SPREAD_BP * fuel * fuel / 1_000_000 * push as u64 / 1000;
                if crate::rng::hash2(salt, nx, ny ^ (tx.wrapping_mul(31) ^ ty.wrapping_mul(17))) % 10_000 < p {
                    lit.push((nx, ny));
                }
            }
        }
        let mut changed = false;
        for k in out {
            self.burn.insert(k, Burn { fire: false, born: t, until: t + ASH_TICKS });
            changed = true;
        }
        for (nx, ny) in lit {
            changed |= self.ignite_with(&g, nx, ny);
        }
        if changed {
            self.burn_seq += 1;
        }
        // flames hurt whoever stands in them
        if self.burning().next().is_none() {
            return;
        }
        let mut hurt: Vec<AgentId> = Vec::new();
        for a in &mut self.agents {
            let k = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
            if self.burn.get(&k).is_some_and(|b| b.fire) {
                a.health -= crate::rules::Gear::cut(FIRE_HURT, a.inventory.gear().fire);
                if t % (FIRE_STEP * 4) == 0 {
                    hurt.push(a.id);
                }
            }
        }
        for id in hurt {
            self.events.push(Event::Burned(id));
        }
    }

    /// Runners on ice go down now and then.
    fn run_slips(&mut self) {
        let g = self.ground();
        if !g.freezing() || g.quiet() {
            return;
        }
        let t = self.tick;
        let mut slipped: Vec<AgentId> = Vec::new();
        for a in &mut self.agents {
            if !(a.sprint && a.stamina > 0 && matches!(a.task, Task::Move { .. } | Task::Walk { .. })) {
                continue;
            }
            let (tx, ty) = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
            if self.burn.contains_key(&(tx, ty)) || g.weather_surface(&self.map, tx, ty) != Surface::Ice {
                continue;
            }
            let gear = a.inventory.gear();
            let chance = crate::rules::Gear::cut(SLIP_PERMILLE as i32, gear.grip) as u64;
            if crate::rng::hash2(self.seed ^ 0x51F, a.id.0 as i32, (t / 10) as i32) % 1000 < chance {
                a.health -= crate::rules::Gear::cut(250, gear.fall);
                a.stamina = (a.stamina - 1500).max(0);
                a.sprint = false;
                a.task = Task::Idle;
                slipped.push(a.id);
            }
        }
        for id in slipped {
            self.events.push(Event::Slipped(id));
        }
    }

    /// Where fires start by themselves: embers off a campfire on a windy dry day, and the odd dry lightning in summer.
    pub(crate) fn run_sparks(&mut self) {
        let t = self.tick;
        let days = self.settings.season_days as u64;
        let g = self.ground();
        if g.sky != Weather::Clear || g.wet >= 300 {
            return;
        }
        let (heading, strength) = time::wind(self.seed, t, days);
        if strength > 500 {
            let fires: Vec<(i64, i64)> = self.buildings.iter().filter(|b| b.kind.burns() && b.fuel > 0).map(|b| (b.x, b.y)).collect();
            // a gale carries embers: a chance per second (of 100000) that grows with the wind (a quarter of a percent at 75%)
            let chance = strength as u64 - 500;
            for (x, y) in fires {
                let h = crate::rng::hash2(self.seed ^ 0xE4B3, (x / 100) as i32, (t / 60) as i32);
                if h % 100_000 >= chance {
                    continue;
                }
                // two to four tiles downwind (the nearest of the four ways)
                let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][((heading + 785) / 1571 % 4) as usize];
                let reach = 2 + ((h >> 20) % 3) as i32;
                let (tx, ty) = ((x / TILE_MM) as i32 + dx * reach, (y / TILE_MM) as i32 + dy * reach);
                if self.ignite_with(&g, tx, ty) {
                    self.events.push(Event::FireStarted { x: tx as i64 * TILE_MM + TILE_MM / 2, y: ty as i64 * TILE_MM + TILE_MM / 2 });
                }
            }
        }
        // dry lightning: on summer days, near someone
        if self.season() == 1 && t % (time::TICKS_PER_DAY / 12) == 0 {
            let humans: Vec<(i64, i64)> = self.agents.iter().filter(|a| a.kind == crate::agents::AgentKind::Human).map(|a| (a.x, a.y)).collect();
            let h = crate::rng::hash2(self.seed ^ 0x1167, (t / (time::TICKS_PER_DAY / 12)) as i32, 0);
            if !humans.is_empty() && h % 100 < 6 {
                let (x, y) = humans[(h >> 8) as usize % humans.len()];
                let (dx, dy) = (((h >> 24) % 81) as i32 - 40, ((h >> 36) % 81) as i32 - 40);
                let (tx, ty) = ((x / TILE_MM) as i32 + dx, (y / TILE_MM) as i32 + dy);
                if self.ignite_with(&g, tx, ty) {
                    self.events.push(Event::FireStarted { x: tx as i64 * TILE_MM + TILE_MM / 2, y: ty as i64 * TILE_MM + TILE_MM / 2 });
                }
            }
        }
    }
}
