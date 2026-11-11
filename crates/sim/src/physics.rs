//! Gravity. People leave the ground when they jump or walk over an edge, and come down hard if the drop was high.
//! A tree that has had the last of its wood cut topples over, pivoting on its stump and speeding up as it goes, and
//! whatever it comes down on is struck. The fallen trunk holds the tree's wood: it is broken up where it lies, a blow
//! at a time, and each blow throws off a piece that has to be picked up. Rocks knocked loose roll and bounce down the
//! slopes and off the ledges.
//! Integer maths only (positions in 1/256 mm), so every peer computes the same falls.

use crate::agents::AgentKind;
use crate::command::AgentId;
use crate::map::{elevation_at_mm, Terrain, CLIFF_MM, TILE_MM};
use crate::rng::hash2;
use crate::world::{Event, Task, World, BREAK_TICKS, MAX_STAT};
use serde::{Deserialize, Serialize};

/// Sub-millimetre steps: positions and speeds in the air are kept in 1/256 mm.
pub const Q: i64 = 256;
/// 9.81 m/s² at 60 ticks a second, in 1/256 mm per tick².
pub const G: i64 = 698;
/// Upward speed of a jump: about 3.8 m/s, enough to rise three quarters of a metre.
pub const JUMP_VZ: i64 = 64 * Q;
/// Breath a jump costs.
pub const JUMP_STAMINA: i32 = 450;
/// A drop up to this is landed on your feet (mm); from here on it hurts, and from `LETHAL_FALL_MM` nobody walks away.
pub const SAFE_FALL_MM: i64 = 2_000;
pub const LETHAL_FALL_MM: i64 = 11_000;
/// From this high a fall breaks a leg for sure; above `SAFE_FALL_MM` it may.
pub const BREAK_FALL_MM: i64 = 3_800;
/// A felled trunk lies this long before it has rotted into the ground; a rock lies still this long, and is gone after
/// `ROCK_LIFE_TICKS` in any case.
pub const TREE_REST_TICKS: u32 = 60 * 60;
/// A trunk nobody breaks up rots away after this long; broken-off pieces left lying go after `PIECE_LIFE_TICKS`.
pub const LOG_ROT_TICKS: u32 = 15 * 60 * 60;
pub const PIECE_LIFE_TICKS: u32 = 15 * 60 * 60;
pub const ROCK_REST_TICKS: u32 = 45 * 60;
pub const ROCK_LIFE_TICKS: u32 = 120 * 60;
/// No more loose rocks than this at once.
pub const MAX_ROCKS: usize = 24;
/// How often the rocky heights shed a stone near each person (ticks), and the odds it happens (percent).
const ROCKFALL_EVERY: u64 = 20 * 60;
const ROCKFALL_PERCENT: u64 = 25;
/// Upright to flat, in µrad.
pub const HALF_PI_URAD: i64 = 1_570_796;

/// Someone off the ground. `top` is the highest the body got (for how far it fell).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Air {
    pub agent: AgentId,
    pub x: i64,
    pub y: i64,
    pub z: i64,
    pub vx: i64,
    pub vy: i64,
    pub vz: i64,
    pub top: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    /// A felled tree, toppling or lying.
    Tree,
    /// A loose rock.
    Rock,
    /// A piece broken off a trunk, lying where it flew, to be picked up.
    Piece,
}

/// Something heavy moving on its own, or lying where it came to rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    /// Stays the same while the body exists, so work can be aimed at it.
    pub id: u32,
    pub kind: BodyKind,
    /// Tree: the tile it grew on, so the renderer can draw the very tree that stood there.
    pub tx: i32,
    pub ty: i32,
    /// Tree: the foot of the trunk. Rock: the bottom of the rock. 1/256 mm; `z` is the height above the shore.
    pub x: i64,
    pub y: i64,
    pub z: i64,
    pub vx: i64,
    pub vy: i64,
    pub vz: i64,
    /// Tree: which way it falls, as a unit vector in permille.
    pub dx: i32,
    pub dy: i32,
    /// Tree: lean from upright (µrad) and how fast it grows (µrad per tick). Rock: distance rolled (mm), and 1 + the
    /// index of the last one it struck (so a rock tumbling on does not strike the same person over and over).
    pub angle: i64,
    pub spin: i64,
    /// Tree: who cut it. They watch which way it goes and are clear when it lands; anyone else in the way is not.
    pub by: AgentId,
    /// Tree: height (mm). Rock and piece: radius (mm).
    pub size: i32,
    pub age: u32,
    /// Ticks it has lain still.
    pub rest: u32,
    /// Tree: pieces of wood still in the trunk, and how many it held when it came down.
    pub wood: u16,
    pub full: u16,
    /// Piece: what it is (an `ItemKind` index).
    pub item: u8,
}

/// A body as format-19 saves stored it (before trunks were broken up where they lie).
#[derive(Deserialize)]
pub(crate) struct BodyV19 {
    kind: BodyKind,
    tx: i32,
    ty: i32,
    x: i64,
    y: i64,
    z: i64,
    vx: i64,
    vy: i64,
    vz: i64,
    dx: i32,
    dy: i32,
    angle: i64,
    spin: i64,
    by: AgentId,
    size: i32,
    age: u32,
    rest: u32,
}

impl BodyV19 {
    pub(crate) fn upgrade(self, id: u32, seed: u64) -> Body {
        let full = log_pieces(seed, self.tx, self.ty);
        let wood = if self.kind == BodyKind::Tree { full } else { 0 };
        Body { id, kind: self.kind, tx: self.tx, ty: self.ty, x: self.x, y: self.y, z: self.z, vx: self.vx, vy: self.vy, vz: self.vz, dx: self.dx, dy: self.dy, angle: self.angle, spin: self.spin, by: self.by, size: self.size, age: self.age, rest: self.rest, wood, full: if wood > 0 { full } else { 0 }, item: 0 }
    }
}

/// How many pieces of wood the trunk of the tree on tile (tx, ty) breaks into: a sapling 3, a big tree 8, a giant 14.
pub fn log_pieces(seed: u64, tx: i32, ty: i32) -> u16 {
    (crate::map::tree_height_mm(seed, tx, ty) / 500).clamp(3, 14) as u16
}

impl Body {
    /// Where the body is in mm (tree: its foot), and its height above the shore.
    pub fn pos_mm(&self) -> (i64, i64, i64) {
        (self.x.div_euclid(Q), self.y.div_euclid(Q), self.z.div_euclid(Q))
    }

    /// A trunk that has come to rest on the ground.
    pub fn lying(&self) -> bool {
        self.kind == BodyKind::Tree && self.angle >= HALF_PI_URAD
    }

    /// Where the next blow on a lying trunk falls (mm): it is broken up from the crown end back to the stump.
    pub fn cut_point(&self) -> (i64, i64) {
        let (bx, by, _) = self.pos_mm();
        let along = (self.size as i64 * self.wood as i64 / self.full.max(1) as i64 - 250).max(300);
        (bx + self.dx as i64 * along / 1000, by + self.dy as i64 * along / 1000)
    }

    /// The length of trunk still lying there (mm).
    pub fn trunk_left_mm(&self) -> i64 {
        if self.full == 0 { self.size as i64 } else { self.size as i64 * self.wood as i64 / self.full as i64 }
    }
}

/// A fresh id for a body made this tick: unique while it lasts.
fn new_body_id(bodies: &[Body], tick: u64) -> u32 {
    let base = (tick as u32).wrapping_mul(16);
    (0..).map(|k| base.wrapping_add(k)).find(|id| bodies.iter().all(|b| b.id != *id)).unwrap_or(base)
}

/// sin of an angle in µrad (0..=π), scaled by a million. Bhaskara's approximation: within 0.2%.
pub fn isin(urad: i64) -> i64 {
    let md = urad.clamp(0, 3_141_593) * 180_000 / 3_141_593; // millidegrees
    let p = md * (180_000 - md);
    4 * p * 1_000_000 / (40_500_000_000 - p)
}

fn ground_q(seed: u64, x: i64, y: i64) -> i64 {
    elevation_at_mm(seed, x.div_euclid(Q), y.div_euclid(Q)) as i64 * Q
}

/// The fall of the ground around a point: permille grade along x and y (positive = rising that way).
fn grade(seed: u64, x_mm: i64, y_mm: i64) -> (i64, i64) {
    let e = |dx: i64, dy: i64| elevation_at_mm(seed, x_mm + dx, y_mm + dy) as i64;
    ((e(250, 0) - e(-250, 0)) * 2, (e(0, 250) - e(0, -250)) * 2)
}

/// Mining a face that drops at least this much over a tile (mm) can send a stone off down it.
pub const LOOSE_ROCK_DROP_MM: i32 = 700;

/// Which way is downhill from the corner of tile (tx, ty), and how far the ground drops over a tile that way (mm).
pub fn steepest_drop(seed: u64, tx: i32, ty: i32) -> (i32, i32, i32) {
    let e0 = crate::map::elevation_mm(seed, tx, ty);
    let mut best = (0, 0, 0);
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)] {
        let d = e0 - crate::map::elevation_mm(seed, tx + dx, ty + dy);
        let d = if dx != 0 && dy != 0 { d * 10 / 14 } else { d };
        if d > best.2 {
            best = (dx, dy, d);
        }
    }
    best
}

impl World {
    pub fn airborne(&self, id: AgentId) -> Option<&Air> {
        self.air.iter().find(|f| f.agent == id)
    }

    /// Spring up. Moving people take their speed with them; the breath runs short if they keep at it.
    pub(crate) fn jump(&mut self, idx: usize) {
        let a = &self.agents[idx];
        if a.kind != AgentKind::Human || a.broken > 0 || a.stamina < JUMP_STAMINA || matches!(a.task, Task::Cave { .. } | Task::Craft { .. }) || self.airborne(a.id).is_some() {
            return;
        }
        let mut speed = a.kind.speed_mm();
        if a.sprint && a.stamina > 0 {
            speed = speed * 17 / 10;
        }
        let (vx, vy) = match a.task {
            Task::Move { x, y } | Task::Walk { x, y } => {
                let (dx, dy) = (x - a.x, y - a.y);
                let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
                if len > 50 { (dx * speed * Q / len, dy * speed * Q / len) } else { (0, 0) }
            }
            _ => (0, 0),
        };
        let g = ground_q(self.seed, a.x * Q, a.y * Q);
        let air = Air { agent: a.id, x: a.x * Q, y: a.y * Q, z: g, vx, vy, vz: JUMP_VZ, top: g };
        self.agents[idx].stamina -= JUMP_STAMINA;
        self.air.push(air);
    }

    /// Someone stepped over an edge with this velocity (mm per tick).
    pub(crate) fn launch(&mut self, id: AgentId, vx: i64, vy: i64) {
        let Some(a) = self.agents.iter().find(|a| a.id == id) else { return };
        if self.airborne(id).is_some() {
            return;
        }
        let g = ground_q(self.seed, a.x * Q, a.y * Q);
        self.air.push(Air { agent: id, x: a.x * Q, y: a.y * Q, z: g, vx: vx * Q, vy: vy * Q, vz: 0, top: g });
    }

    /// The last of a tree has been cut: it starts to lean away from the cutter at (cx, cy).
    /// Also used by the front end's demo mode.
    pub fn fell_tree(&mut self, tx: i32, ty: i32, cutter: AgentId, cx: i64, cy: i64) -> u32 {
        let (bx, by) = crate::map::node_anchor_mm(self.seed, tx, ty);
        let (mut dx, dy) = (bx - cx, by - cy);
        if dx == 0 && dy == 0 {
            dx = 1;
        }
        let len = ((dx * dx + dy * dy) as u64).isqrt().max(1) as i64;
        let size = crate::map::tree_height_mm(self.seed, tx, ty);
        let z = ground_q(self.seed, bx * Q, by * Q);
        let id = new_body_id(&self.bodies, self.tick);
        let wood = log_pieces(self.seed, tx, ty);
        self.bodies.push(Body {
            id,
            wood,
            full: wood,
            item: 0,
            kind: BodyKind::Tree,
            tx,
            ty,
            x: bx * Q,
            y: by * Q,
            z,
            vx: 0,
            vy: 0,
            vz: 0,
            dx: (dx * 1000 / len) as i32,
            dy: (dy * 1000 / len) as i32,
            angle: 25_000,
            spin: 0,
            by: cutter,
            size,
            age: 0,
            rest: 0,
        });
        id
    }

    /// A piece of `item` flies off from (x, y, z) mm with this velocity (mm per tick).
    pub(crate) fn throw_piece(&mut self, item: crate::map::ItemKind, (x, y, z): (i64, i64, i64), (vx, vy, vz): (i64, i64, i64)) -> u32 {
        let id = new_body_id(&self.bodies, self.tick);
        let (x, y) = (x.clamp(300, self.map.w as i64 * TILE_MM - 300), y.clamp(300, self.map.h as i64 * TILE_MM - 300));
        self.bodies.push(Body { id, kind: BodyKind::Piece, tx: 0, ty: 0, x: x * Q, y: y * Q, z: z * Q, vx: vx * Q, vy: vy * Q, vz: vz * Q, dx: 0, dy: 0, angle: 0, spin: 0, by: AgentId(u32::MAX), size: 110, age: 0, rest: 0, wood: 0, full: 0, item: item as u8 });
        id
    }

    /// A rock comes loose at (x, y) mm and sets off this way (mm per tick).
    pub fn loose_rock(&mut self, x: i64, y: i64, vx: i64, vy: i64, salt: u64) {
        if self.bodies.iter().filter(|b| b.kind == BodyKind::Rock).count() >= MAX_ROCKS {
            return;
        }
        let size = 160 + (hash2(self.seed ^ 0x50C4, (salt & 0x7fff_ffff) as i32, self.tick as i32) % 220) as i32;
        let z = ground_q(self.seed, x * Q, y * Q) + 200 * Q;
        let id = new_body_id(&self.bodies, self.tick);
        self.bodies.push(Body { id, wood: 0, full: 0, item: 0, kind: BodyKind::Rock, tx: 0, ty: 0, x: x * Q, y: y * Q, z, vx: vx * Q, vy: vy * Q, vz: 0, dx: 0, dy: 0, angle: 0, spin: 0, by: AgentId(u32::MAX), size, age: 0, rest: 0 });
    }

    pub(crate) fn run_physics(&mut self) {
        self.run_air();
        self.run_bodies();
        self.run_rockfall();
    }

    fn run_air(&mut self) {
        let seed = self.seed;
        let mut i = 0;
        while i < self.air.len() {
            let Some(ai) = self.agents.iter().position(|a| a.id == self.air[i].agent) else {
                self.air.remove(i);
                continue;
            };
            let f = &mut self.air[i];
            f.vz -= G;
            let (nx, ny) = (f.x + f.vx, f.y + f.vy);
            // a rock face (or the map's end) in the way: the body slides down it instead of going on
            let wall = self.map.terrain_mm(nx.div_euclid(Q), ny.div_euclid(Q)).is_none() || ground_q(seed, nx, ny) > f.z + 150 * Q;
            if wall {
                f.vx = 0;
                f.vy = 0;
            } else {
                f.x = nx;
                f.y = ny;
            }
            f.z += f.vz;
            f.top = f.top.max(f.z);
            let a = &mut self.agents[ai];
            a.x = f.x.div_euclid(Q);
            a.y = f.y.div_euclid(Q);
            let g = ground_q(seed, f.x, f.y);
            if f.z > g {
                i += 1;
                continue;
            }
            // down: how far it was from the top of the flight to here
            let drop = (f.top - g) / Q;
            let water = self.map.terrain_mm(a.x, a.y) == Some(Terrain::Water);
            let id = a.id;
            self.air.remove(i);
            if water || drop <= SAFE_FALL_MM {
                continue;
            }
            let a = &mut self.agents[ai];
            a.health -= ((drop - SAFE_FALL_MM) * MAX_STAT as i64 / (LETHAL_FALL_MM - SAFE_FALL_MM)) as i32;
            let unlucky = hash2(seed ^ 0xFA11, id.0 as i32, self.tick as i32) % (BREAK_FALL_MM - SAFE_FALL_MM) as u64;
            if a.broken == 0 && (drop >= BREAK_FALL_MM || (unlucky as i64) < drop - SAFE_FALL_MM) {
                a.broken = BREAK_TICKS;
                a.sprint = false;
                self.events.push(Event::Fractured(id));
            }
            self.events.push(Event::Fell { agent: id, height_mm: drop as i32 });
        }
    }

    fn run_bodies(&mut self) {
        let seed = self.seed;
        let wind_now = crate::time::wind(seed, self.tick, self.settings.season_days as u64);
        let mut hits: Vec<(usize, i32, bool, bool)> = Vec::new();
        let mut i = 0;
        while i < self.bodies.len() {
            let b = &mut self.bodies[i];
            b.age = b.age.saturating_add(1);
            let gone = match b.kind {
                BodyKind::Tree => {
                    if b.angle < HALF_PI_URAD {
                        // a rod pivoting on its foot: angular acceleration 3g/2L · sin(lean)
                        let alpha = 4_087_500 * isin(b.angle) / b.size.max(500) as i64 / 1_000_000;
                        b.spin += alpha.max(1);
                        b.angle += b.spin;
                        if b.angle >= HALF_PI_URAD {
                            b.angle = HALF_PI_URAD;
                            b.spin = 0;
                            let (bx, by, _) = b.pos_mm();
                            let (len, dx, dy) = (b.size as i64, b.dx as i64, b.dy as i64);
                            let mut struck = false;
                            for (ai, a) in self.agents.iter().enumerate() {
                                if matches!(a.task, Task::Cave { .. }) || a.id == b.by {
                                    continue;
                                }
                                // along the trunk and how far to its side
                                let (rx, ry) = (a.x - bx, a.y - by);
                                let t = (rx * dx + ry * dy) / 1000;
                                let side = ((rx * dy - ry * dx) / 1000).abs();
                                if t < 200 || t > len + 300 {
                                    continue;
                                }
                                let crown = t > len * 45 / 100;
                                let reach = if crown { len / 5 + 300 } else { 380 };
                                if side <= reach {
                                    let dmg = if crown { 900 + len as i32 / 4 } else { 1_200 + len as i32 * 3 / 4 };
                                    hits.push((ai, dmg, !crown && len > 3_000, true));
                                    struck = true;
                                }
                            }
                            self.events.push(Event::TreeDown { x: bx + dx * len / 2000, y: by + dy * len / 2000, size_mm: len as i32, struck });
                        }
                        false
                    } else {
                        // broken up to the stump, or left to rot
                        b.rest += 1;
                        b.wood == 0 || b.rest > LOG_ROT_TICKS
                    }
                }
                BodyKind::Rock | BodyKind::Piece => {
                    let rock = b.kind == BodyKind::Rock;
                    b.vz -= G;
                    let (nx, ny) = (b.x + b.vx, b.y + b.vy);
                    let mut gone = false;
                    match self.map.terrain_mm(nx.div_euclid(Q), ny.div_euclid(Q)) {
                        None => {
                            b.vx = 0;
                            b.vy = 0;
                        }
                        // a rock sinks; a piece of wood (or anything put down) stops at the water's edge
                        Some(Terrain::Water) if rock => gone = true,
                        // anything else floats: it rides the river's current, or lies on still water
                        Some(Terrain::Water) => {
                            let (cx, cy) = crate::map::current_mm(&self.map, nx.div_euclid(Q).div_euclid(TILE_MM) as i32, ny.div_euclid(Q).div_euclid(TILE_MM) as i32);
                            // and the wind pushes it over the water, slowly (a quarter mm a tick in a full gale)
                            let (wh, ws) = wind_now;
                            let (wx, wy) = (crate::map::cos_mrad(wh as i64) * ws as i64 / 4, crate::map::sin_mrad(wh as i64) * ws as i64 / 4);
                            b.vx = cx * Q + wx * Q / 1_000_000;
                            b.vy = cy * Q + wy * Q / 1_000_000;
                            b.x = nx;
                            b.y = ny;
                            b.rest = 0;
                        }
                        Some(_) => {
                            if ground_q(seed, nx, ny) > b.z + b.size as i64 * Q / 2 {
                                // into a rising face: it knocks back off it
                                b.vx = -b.vx / 3;
                                b.vy = -b.vy / 3;
                            } else {
                                b.angle += (((b.vx * b.vx + b.vy * b.vy) as u64).isqrt() as i64) / Q;
                                b.x = nx;
                                b.y = ny;
                            }
                        }
                    }
                    b.z += b.vz;
                    let g = ground_q(seed, b.x, b.y);
                    if b.z <= g {
                        b.z = g;
                        if b.vz < -30 * Q {
                            // a hard landing: it bounces, losing most of its fall
                            b.vz = -b.vz * 30 / 100;
                            b.vx = b.vx * 75 / 100;
                            b.vy = b.vy * 75 / 100;
                            if rock {
                                self.events.push(Event::RockBounce { x: b.x / Q, y: b.y / Q });
                            }
                        } else {
                            b.vz = 0;
                            let (gx, gy) = grade(seed, b.x.div_euclid(Q), b.y.div_euclid(Q));
                            let steep = ((gx * gx + gy * gy) as u64).isqrt() as i64;
                            // a rock rolls down any real slope; a split length of wood only slides down a steep one
                            if rock || steep > 900 {
                                b.vx -= G * gx / (1000 + steep);
                                b.vy -= G * gy / (1000 + steep);
                            }
                            // rolling resistance, and a little drag; wood scrapes to a stop
                            let sp = ((b.vx * b.vx + b.vy * b.vy) as u64).isqrt() as i64;
                            let fr = if rock { G * 9 / 100 + sp / 60 } else { G * 45 / 100 + sp / 8 };
                            if sp <= fr {
                                b.vx = 0;
                                b.vy = 0;
                            } else {
                                b.vx -= b.vx * fr / sp;
                                b.vy -= b.vy * fr / sp;
                            }
                            if b.vx == 0 && b.vy == 0 && (steep < 700 || !rock && steep <= 900) {
                                b.rest += 1;
                            } else {
                                b.rest = 0;
                            }
                        }
                    }
                    // what it runs into
                    let sp = ((b.vx * b.vx + b.vy * b.vy + b.vz * b.vz) as u64).isqrt() as i64 / Q;
                    if rock && sp >= 20 && !gone {
                        let (rx, ry, rz) = b.pos_mm();
                        let reach = b.size as i64 + 330;
                        for (ai, a) in self.agents.iter().enumerate() {
                            if matches!(a.task, Task::Cave { .. }) || hits.iter().any(|h| h.0 == ai) || b.spin == ai as i64 + 1 {
                                continue;
                            }
                            let feet = elevation_at_mm(seed, a.x, a.y) as i64;
                            if (a.x - rx).pow(2) + (a.y - ry).pow(2) <= reach * reach && rz >= feet - 150 && rz <= feet + 1_700 {
                                hits.push((ai, (sp * sp * 18 / 100).min(MAX_STAT as i64) as i32, sp > 110, false));
                                b.vx /= 4;
                                b.vy /= 4;
                                b.vz = b.vz.max(0);
                                b.spin = ai as i64 + 1;
                                break;
                            }
                        }
                    }
                    if rock { gone || b.rest > ROCK_REST_TICKS || b.age > ROCK_LIFE_TICKS } else { gone || b.age > PIECE_LIFE_TICKS }
                }
            };
            if gone {
                self.bodies.remove(i);
            } else {
                i += 1;
            }
        }
        for (ai, dmg, breaks, tree) in hits {
            let a = &mut self.agents[ai];
            let id = a.id;
            a.health -= dmg;
            if breaks && a.broken == 0 {
                a.broken = BREAK_TICKS;
                a.sprint = false;
                self.events.push(Event::Fractured(id));
            }
            self.events.push(if tree { Event::Crushed(id) } else { Event::RockHit(id) });
        }
    }

    /// Up in the rocky heights a stone breaks off a ledge now and then and comes down near whoever is about.
    fn run_rockfall(&mut self) {
        if self.tick % ROCKFALL_EVERY != ROCKFALL_EVERY - 1 {
            return;
        }
        let people: Vec<(AgentId, i64, i64)> = self.agents.iter().filter(|a| a.kind == AgentKind::Human && !matches!(a.task, Task::Cave { .. })).map(|a| (a.id, a.x, a.y)).collect();
        for (id, x, y) in people {
            let h = hash2(self.seed ^ 0x40CF, id.0 as i32, (self.tick / ROCKFALL_EVERY) as i32);
            if h % 100 >= ROCKFALL_PERCENT {
                continue;
            }
            let tx = x.div_euclid(TILE_MM) as i32 + ((h >> 8) % 19) as i32 - 9;
            let ty = y.div_euclid(TILE_MM) as i32 + ((h >> 16) % 19) as i32 - 9;
            if self.map.terrain(tx, ty) != Some(Terrain::Rock) {
                continue;
            }
            let (dx, dy, drop) = steepest_drop(self.seed, tx, ty);
            if drop < CLIFF_MM {
                continue;
            }
            let (cx, cy) = (tx as i64 * TILE_MM, ty as i64 * TILE_MM);
            self.loose_rock(cx, cy, dx as i64 * 14, dy as i64 * 14, h);
            self.events.push(Event::Rockfall { x: cx, y: cy });
        }
    }
}
