//! Playing in a cave: going in, walking its floor, digging its walls, the light, the stairs and what the rock does.
//! The cave itself (its levels and how they are cut) is in `crate::caves`.

use super::*;
use crate::caves::{ore_item, slide, Cave, CaveSelf, Cell, Level, CAVE_LEVELS};
use crate::rules::{Gear, ToolKind};
use std::collections::BTreeMap;

/// Ticks of light in a torch (2.5 minutes). A lamp with plant oil burns twice as long, with kerosene three times.
pub const CAVE_TORCH_TICKS: u16 = 9_000;
/// Depth of the deepest level (the first is 0).
pub const CAVE_MAX_DEPTH: u8 = 3;
/// Ticks for one swing of a pick at an ore vein; bare rock takes a little less, rubble about a third.
pub const CAVE_MINE_TICKS: u16 = 150;
pub const ROCK_SWING_TICKS: u32 = 110;
pub(super) const RUBBLE_SWING_TICKS: u32 = 60;
/// How close the walker must be to a cave mouth's middle to step in (mm).
pub(crate) const ENTER_REACH_MM: i64 = 1_500;
/// A cave mouth can be entered from this far by command; further off the walker goes up to it first.
const ENTER_NEAR_MM: i64 = 3_000;
/// After a stair the walker is left alone this long, so it does not go straight back.
const STAIR_COOL: u16 = 90;
/// In the dark, every this many ticks one may stumble.
const STUMBLE_EVERY: u16 = 600;
/// How far a pump may stand from the mouth and still keep the sump dry (mm).
const PUMP_REACH_MM: i64 = 15_000;

/// A fall of rock to settle once the walkers' places are known: where (a tile) and which level of which cave.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RockFall {
    node: u32,
    level: u8,
    tx: i32,
    ty: i32,
    salt: u64,
}

/// Commands that mean something only on the surface: underground they are ignored.
pub(crate) fn cave_refuses(k: &CommandKind) -> bool {
    matches!(
        k,
        CommandKind::Gather { .. }
            | CommandKind::Hunt { .. }
            | CommandKind::Drink { .. }
            | CommandKind::Build { .. }
            | CommandKind::StartJob { .. }
            | CommandKind::Deposit { .. }
            | CommandKind::Load { .. }
            | CommandKind::Collect { .. }
            | CommandKind::SetAuto { .. }
            | CommandKind::Tame { .. }
            | CommandKind::Feed { .. }
            | CommandKind::SetRoute { .. }
            | CommandKind::EnterCave { .. }
            | CommandKind::Refuel { .. }
            | CommandKind::Jump
            | CommandKind::Split { .. }
            | CommandKind::Pick { .. }
            | CommandKind::Hitch { .. }
            | CommandKind::Design { .. }
            | CommandKind::Prospect
            | CommandKind::Drop { .. }
            | CommandKind::Fish { .. }
            | CommandKind::Relocate { .. }
            | CommandKind::Turn { .. }
            | CommandKind::Camp
    )
}

/// Does this pack hold a light for a cave: a torch, or a lamp with oil in it?
pub fn has_light(inv: &Inventory) -> bool {
    inv.get(ItemKind::Torch) > 0 || (inv.get(ItemKind::OilLamp) > 0 && (inv.get(ItemKind::PlantOil) > 0 || inv.get(ItemKind::Kerosene) > 0))
}

/// Light a light from the pack and say how many ticks it burns. A lamp (kept) takes a spoonful of oil: kerosene burns
/// clean and steady, three times a torch; plant oil twice. A torch is used up.
pub(crate) fn take_light(inv: &mut Inventory) -> Option<u16> {
    if inv.get(ItemKind::OilLamp) > 0 && inv.get(ItemKind::Kerosene) > 0 {
        inv.counts[ItemKind::Kerosene as usize] -= 1;
        Some(CAVE_TORCH_TICKS * 3)
    } else if inv.get(ItemKind::OilLamp) > 0 && inv.get(ItemKind::PlantOil) > 0 {
        inv.counts[ItemKind::PlantOil as usize] -= 1;
        Some(CAVE_TORCH_TICKS * 2)
    } else if inv.get(ItemKind::Torch) > 0 {
        inv.counts[ItemKind::Torch as usize] -= 1;
        Some(CAVE_TORCH_TICKS)
    } else {
        None
    }
}

fn centre(t: (i32, i32)) -> (i64, i64) {
    (t.0 as i64 * TILE_MM + TILE_MM / 2, t.1 as i64 * TILE_MM + TILE_MM / 2)
}

/// Pace underground, mm per tick.
fn cave_speed(a: &Agent, c: &CaveSelf, lv: &Level) -> i64 {
    let mut s = a.kind.speed_mm();
    let cell = lv.cell(c.tile().0, c.tile().1);
    if a.sprint && a.stamina > 0 && cell != Cell::Deep {
        s = s * 17 / 10;
    }
    match cell {
        Cell::Shallow => s = s * 55 / 100,
        Cell::Deep => s = s * 45 / 100,
        _ => {}
    }
    if a.broken > 0 {
        s = s * 55 / 100;
    }
    s = s * (1000 - a.inventory.gear().burden as i64) / 1000;
    // groping in the dark
    if c.light == 0 {
        s = s * 60 / 100;
    }
    s.max(1)
}

/// One step toward (tx, ty) mm, sliding along walls.
fn cave_step(a: &Agent, c: &mut CaveSelf, lv: &Level, tx: i64, ty: i64) -> Step {
    // a body caught in the rock (a fall of stone beside it) is set back on the middle of its tile
    if lv.blocked(c.x, c.y) {
        let (cx, cy) = centre(c.tile());
        c.x = cx;
        c.y = cy;
    }
    let (dx, dy) = (tx - c.x, ty - c.y);
    let len = ((dx * dx + dy * dy) as u64).isqrt() as i64;
    if len == 0 {
        return Step::Arrived;
    }
    let speed = cave_speed(a, c, lv);
    let (nx, ny) = if len <= speed { (tx, ty) } else { (c.x + dx * speed / len, c.y + dy * speed / len) };
    match slide(lv, c.x, c.y, nx, ny) {
        Some((fx, fy)) if (fx, fy) != (c.x, c.y) => {
            c.x = fx;
            c.y = fy;
            if (fx, fy) == (tx, ty) { Step::Arrived } else { Step::Moved }
        }
        _ => Step::Blocked,
    }
}

/// Walk to a point round the walls (a planned route when the straight way is not clear).
fn cave_advance(a: &mut Agent, c: &mut CaveSelf, lv: &Level, tx: i64, ty: i64) -> Step {
    let want = (tx.div_euclid(TILE_MM) as i32, ty.div_euclid(TILE_MM) as i32);
    let Some(goal) = lv.nearest_walkable(want) else { return Step::Blocked };
    let target = if goal == want && !lv.blocked(tx, ty) { (tx, ty) } else { centre(goal) };
    if a.path_goal != goal {
        a.path.clear();
        a.path_goal = goal;
    }
    if a.path.is_empty() {
        if lv.line_clear((c.x, c.y), target) {
            return cave_step(a, c, lv, target.0, target.1);
        }
        let Some(route) = lv.find_path(c.tile(), goal) else { return Step::Blocked };
        let mut wp = lv.smooth((c.x, c.y), &route);
        wp.reverse();
        a.path = wp;
    }
    let Some(&next) = a.path.last() else { return Step::Blocked };
    let (wx, wy) = if a.path.len() == 1 && next == goal { target } else { centre(next) };
    match cave_step(a, c, lv, wx, wy) {
        Step::Arrived => {
            a.path.pop();
            if (c.x, c.y) == target { Step::Arrived } else { Step::Moved }
        }
        // something came down in the way: plan again next tick
        Step::Blocked => {
            a.path.clear();
            Step::Moved
        }
        s => s,
    }
}

/// What one tick of work underground does for one walker: walking, and digging. (The light, the stairs and the
/// water are `World::run_caves`.)
#[allow(clippy::too_many_arguments)]
pub(crate) fn cave_task(a: &mut Agent, caves: &mut BTreeMap<u32, Cave>, events: &mut Vec<Event>, rules: &crate::rules::Rules, settings: &WorldSettings, seed: u64, tick: u64, stage: u32, rocks: &mut Vec<RockFall>) {
    let Some(mut c) = a.cave else { return };
    let Some(cave) = caves.get_mut(&c.node) else {
        // (a cave that is not there: back to the surface)
        a.cave = None;
        return;
    };
    let Some(lv) = cave.level(c.level) else {
        a.cave = None;
        return;
    };
    match a.task {
        Task::Move { x, y } => {
            if !matches!(cave_advance(a, &mut c, lv, x, y), Step::Moved) {
                a.task = Task::Idle;
            }
        }
        Task::Walk { x, y } => {
            if !matches!(cave_step(a, &mut c, lv, x, y), Step::Moved) {
                a.task = Task::Idle;
            }
        }
        Task::Dig { tx, ty, progress } => {
            let human = a.kind == AgentKind::Human;
            let cell = lv.cell(tx, ty);
            let rubble = cell == Cell::Rubble;
            let vein = lv.ore_at(tx, ty);
            let tier = pick_tier(rules, a.kind, &a.inventory);
            if !lv.diggable(tx, ty) {
                a.task = Task::Idle;
            } else if tier.is_none() && !rubble {
                if human {
                    events.push(Event::Rejected(a.id, Reject::NeedsTool { tool: ToolKind::Pick, tier: 0 }));
                }
                a.task = Task::Idle;
            } else if a.inventory.is_full(a.capacity()) {
                if human {
                    events.push(Event::Rejected(a.id, Reject::PackFull));
                }
                a.task = Task::Idle;
            } else if (c.tile().0 - tx).abs().max((c.tile().1 - ty).abs()) > 1 {
                // walk up to the wall: to the open tile beside it nearest to here
                let stand = [(0, 1), (1, 0), (0, -1), (-1, 0)]
                    .iter()
                    .map(|&(dx, dy)| (tx + dx, ty + dy))
                    .filter(|&(x, y)| matches!(lv.cell(x, y), Cell::Floor | Cell::Shallow))
                    .min_by_key(|&(x, y)| { let (px, py) = centre((x, y)); dist2(px, py, c.x, c.y) });
                match stand {
                    Some(s) => {
                        let (px, py) = centre(s);
                        if matches!(cave_advance(a, &mut c, lv, px, py), Step::Blocked) {
                            if human {
                                events.push(Event::Rejected(a.id, Reject::Unreachable));
                            }
                            a.task = Task::Idle;
                        }
                    }
                    None => a.task = Task::Idle,
                }
            } else {
                let base = if rubble { RUBBLE_SWING_TICKS } else if vein.is_some() { CAVE_MINE_TICKS as u32 } else { ROCK_SWING_TICKS };
                let tool = rules.best_tool(ToolKind::Pick, |k| a.inventory.get(k) > 0).copied();
                let life = tool.map_or(0, |t| tool_life(&t, settings));
                let blunt = human && tool.is_some_and(|t| life > 0 && a.wear_of(t.item) >= life);
                // handier with each evolutionary step: 10% slower at stage 0, 10% faster at stage 5
                let need = (base * (110 - 4 * stage) / 100).max(1) * if blunt { 3 } else { 1 };
                if progress + 1 < need {
                    a.task = Task::Dig { tx, ty, progress: progress + 1 };
                } else {
                    // a swing lands
                    let depth = c.depth() as u64;
                    let level = c.level;
                    let roll = crate::rng::hash2(seed ^ 0xCA7E, c.node as i32 * 8 + depth as i32, (tick % 1_000_000) as i32) % 100;
                    let fall = if rubble { 0 } else { 5 + 7 * depth };
                    if roll < fall {
                        // the ceiling over the miner comes down: stones on the head, and the floor round about is strewn with rubble
                        let blow = a.inventory.gear().blow;
                        a.health -= Gear::cut(1_500, blow);
                        a.injury += Gear::cut(600, blow);
                        events.push(Event::CaveRockfall(a.id));
                        if depth >= 2 && crate::rng::hash2(seed ^ 0xB0E5, a.id.0 as i32, (tick % 1_000_000) as i32) % 100 < 50 && a.broken == 0 {
                            a.broken = BREAK_TICKS;
                            events.push(Event::Fractured(a.id));
                        }
                        rocks.push(RockFall { node: c.node, level, tx: c.tile().0, ty: c.tile().1, salt: roll ^ tick });
                        a.task = Task::Idle;
                    } else {
                        let mut gave: Vec<(ItemKind, u16)> = Vec::new();
                        let mut left = 0;
                        match vein {
                            Some((code, swings)) => {
                                // the deeper, the richer the vein
                                if let Some(ore) = ore_item(code) {
                                    gave.push((ore, 1 + (depth >= 3) as u16));
                                }
                                gave.push((ItemKind::Stone, 1));
                                left = swings - 1;
                            }
                            None => gave.push((ItemKind::Stone, if rubble { 1 } else { 2 })),
                        }
                        for (item, n) in gave {
                            a.inventory.counts[item as usize] += n;
                            if human {
                                events.push(Event::Gathered { agent: a.id, item });
                            }
                        }
                        // an apron-wearer's tools last longer: some strokes leave no mark
                        let spared = human && {
                            let save = a.inventory.gear().tool_save as u64;
                            save > 0 && crate::rng::hash2(seed ^ 0xA9F0, a.id.0 as i32, (tick % 1_000_000) as i32) % 100 < save
                        };
                        if let Some(t) = tool.filter(|_| human && life > 0 && !spared)
                            && a.add_wear(t.item) == life {
                                events.push(Event::ToolBlunt { agent: a.id, item: t.item });
                            }
                        // a vein with ore left is worked on; anything else opens up into floor
                        if left > 0 {
                            cave.set_tile(level, tx, ty, Cell::Rock, left);
                            a.task = Task::Dig { tx, ty, progress: 0 };
                        } else {
                            cave.set_tile(level, tx, ty, Cell::Floor, 0);
                            a.task = Task::Idle;
                        }
                    }
                }
            }
        }
        Task::Idle | Task::Craft { .. } => {}
        // surface work does not follow anyone underground
        _ => a.task = Task::Idle,
    }
    a.cave = Some(c);
}

impl World {
    /// Is a working pump within reach of this mouth? A piston pump wants power; a steam pump wants steam.
    pub fn pumped(&self, node: u32) -> bool {
        let Some(n) = self.nodes.get(node as usize) else { return false };
        let (mx, my) = n.center_mm();
        self.pumped_at(mx, my)
    }

    pub(super) fn pumped_at(&self, mx: i64, my: i64) -> bool {
        self.buildings
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.kind, BuildingKind::Pump | BuildingKind::SteamPump) && dist2(p.x, p.y, mx, my) <= PUMP_REACH_MM * PUMP_REACH_MM)
            .any(|(pi, p)| if p.kind == BuildingKind::SteamPump { self.requirement_met(pi) } else { self.powered_at(p.x, p.y) })
    }

    /// The cave under a mouth, cut now if nobody has been in it yet. None if the node is not a cave mouth.
    pub fn cave_for(&mut self, node: u32) -> Option<&mut Cave> {
        if crate::map::has_natural_caves(self.seed) {
            return None;
        }
        let n = self.nodes.get(node as usize).filter(|n| n.kind == NodeKind::CaveMouth)?;
        let (seed, tx, ty) = (self.seed, n.tx, n.ty);
        Some(self.caves.entry(node).or_insert_with(|| Cave::generate(seed, tx, ty)))
    }

    fn refresh_sump(&mut self, node: u32) {
        let wet = !self.pumped(node);
        if let Some(c) = self.caves.get_mut(&node) {
            c.set_wet(wet);
        }
    }

    /// Where a walker's light puts it: the cave level it stands on, if it is underground.
    pub fn cave_level_of(&self, id: AgentId) -> Option<&Level> {
        let c = self.agents.get(id.0 as usize)?.cave?;
        self.caves.get(&c.node)?.level(c.level)
    }

    pub(crate) fn cave_enter_command(&mut self, idx: usize, node: NodeId) {
        let id = self.agents[idx].id;
        if self.agents[idx].kind != AgentKind::Human {
            return;
        }
        let Some(mouth) = self.nodes.get(node.0 as usize).filter(|n| n.kind == NodeKind::CaveMouth).map(|n| n.center_mm()) else { return };
        let a = &self.agents[idx];
        if !has_light(&a.inventory) {
            return self.reject(id, Reject::NoLight);
        }
        if dist2(a.x, a.y, mouth.0, mouth.1) <= ENTER_NEAR_MM * ENTER_NEAR_MM {
            self.enter_cave(idx, node);
        } else {
            self.agents[idx].task = Task::Enter { node };
        }
    }

    /// Step into the mouth: a light is lit, the cave is cut if it is the first visit, and the walker stands inside.
    pub(crate) fn enter_cave(&mut self, idx: usize, node: NodeId) {
        let id = self.agents[idx].id;
        if self.agents[idx].cave.is_some() || self.agents[idx].kind != AgentKind::Human {
            return;
        }
        let natural = self.natural_mouth(node.0);
        if natural.is_none() && self.cave_for(node.0).is_none() {
            return;
        }
        let a = &mut self.agents[idx];
        // no light is needed to go in: without one the walker sees little and gropes
        let light = take_light(&mut a.inventory).unwrap_or(0);
        if matches!(a.task, Task::Craft { .. }) {
            self.events.push(Event::CraftAbandoned(id));
        }
        let a = &mut self.agents[idx];
        let (lx, ly) = if let Some(mouth) = natural {
            mouth.landing.center_mm()
        } else {
            let lv = &self.caves[&node.0].levels[0];
            centre(lv.landing(lv.entry))
        };
        let mut cave = CaveSelf { node: node.0, level: 1, x: lx, y: ly, light, full: light, dark: 0, cool: STAIR_COOL, leaving: false };
        if let Some(mouth) = natural {
            cave.set_elevation(mouth.landing.z);
            a.x = lx;
            a.y = ly;
        }
        a.cave = Some(cave);
        a.task = Task::Idle;
        a.path.clear();
        a.path_goal = (-1, -1);
        a.seated = None;
        if natural.is_none() { self.refresh_sump(node.0); }
        self.events.push(Event::CaveEntered(id));
    }

    pub(crate) fn cave_dig_command(&mut self, idx: usize, tx: i32, ty: i32) {
        let a = &self.agents[idx];
        let Some(c) = a.cave else { return };
        if a.kind != AgentKind::Human {
            return;
        }
        let can_dig = if c.elevation_m().is_some() {
            matches!(self.natural_cave_cell(c, tx, ty), Some(Cell::Rock | Cell::Rubble))
        } else {
            self.caves.get(&c.node).and_then(|cv| cv.level(c.level)).is_some_and(|l| l.diggable(tx, ty))
        };
        if !can_dig {
            return;
        }
        let id = a.id;
        if matches!(a.task, Task::Craft { .. }) {
            self.events.push(Event::CraftAbandoned(id));
        }
        let a = &mut self.agents[idx];
        a.task = Task::Dig { tx, ty, progress: 0 };
        if let Some(c) = &mut a.cave {
            c.leaving = false;
        }
    }

    pub(crate) fn cave_leave_command(&mut self, idx: usize) {
        let Some(mut c) = self.agents[idx].cave else { return };
        if c.elevation_m().is_some() {
            self.natural_leave_command(idx, c);
            return;
        }
        let Some(lv) = self.caves.get(&c.node).and_then(|cv| cv.level(c.level)) else { return };
        let (x, y) = centre(lv.entry);
        c.leaving = true;
        self.agents[idx].cave = Some(c);
        self.agents[idx].task = Task::Move { x, y };
    }

    /// Stone comes down on the floor round (tx, ty): one or two tiles of rubble, never on the ways in and out, on
    /// the sump, or under anyone's feet.
    pub(crate) fn drop_rocks(&mut self, r: RockFall) {
        let occupied: Vec<(i32, i32)> = self.agents.iter().filter_map(|a| a.cave).filter(|c| c.node == r.node && c.level == r.level).map(|c| c.tile()).collect();
        let Some(cave) = self.caves.get_mut(&r.node) else { return };
        let Some(lv) = cave.level(r.level) else { return };
        let near = |p: (i32, i32), q: (i32, i32)| (p.0 - q.0).abs().max((p.1 - q.1).abs()) <= 2;
        let mut cands: Vec<(i32, i32)> = Vec::new();
        for dy in -2..=2 {
            for dx in -2..=2 {
                let t = (r.tx + dx, r.ty + dy);
                let sump = lv.sump.contains(&(Level::idx(t.0, t.1) as u16));
                if lv.cell(t.0, t.1) == Cell::Floor && !sump && !near(t, lv.entry) && lv.down.is_none_or(|d| !near(t, d)) && !occupied.contains(&t) {
                    cands.push(t);
                }
            }
        }
        if cands.is_empty() {
            return;
        }
        let n = 1 + (r.salt % 2) as usize;
        let start = (r.salt >> 3) as usize % cands.len();
        let picks: Vec<(i32, i32)> = (0..n.min(cands.len())).map(|k| cands[(start + k * 3) % cands.len()]).collect();
        for t in picks {
            cave.set_tile(r.level, t.0, t.1, Cell::Rubble, 0);
        }
    }

    /// Time underground, a tick: the light burns, the dark is groped through, the water takes its toll and the stairs
    /// lead where they lead.
    pub(crate) fn run_caves(&mut self) {
        if self.caves.is_empty() && !self.agents.iter().any(|a| a.cave.is_some()) {
            return;
        }
        if self.tick.is_multiple_of(30) {
            let mouths: std::collections::BTreeSet<u32> = self.agents.iter().filter_map(|a| a.cave).filter(|c| c.elevation_m().is_none()).map(|c| c.node).collect();
            for m in mouths {
                self.refresh_sump(m);
            }
        }
        let rules_now = self.rules_arc();
        for ai in 0..self.agents.len() {
            let Some(mut c) = self.agents[ai].cave else { continue };
            let id = self.agents[ai].id;
            // the light
            if c.light > 0 {
                c.light -= 1;
                if c.light == 0 {
                    // the next one is lit from the pack, if there is one
                    match take_light(&mut self.agents[ai].inventory) {
                        Some(l) => {
                            c.light = l;
                            c.full = l;
                            c.dark = 0;
                            self.events.push(Event::CaveRelit(id));
                        }
                        None => self.events.push(Event::CaveDark(id)),
                    }
                }
            } else if let Some(l) = take_light(&mut self.agents[ai].inventory) {
                // a light found or made underground is lit at once
                c.light = l;
                c.full = l;
                c.dark = 0;
                self.events.push(Event::CaveRelit(id));
            } else {
                c.dark = c.dark.saturating_add(1);
                // groping about in the dark, now and then a knock
                if c.dark % STUMBLE_EVERY == 0 && crate::rng::hash2(self.seed ^ 0xDA6C, id.0 as i32, (self.tick % 1_000_000) as i32) % 100 < 50 {
                    let a = &mut self.agents[ai];
                    a.health -= 120;
                    a.injury += 120;
                    self.events.push(Event::CaveStumbled(id));
                }
            }
            c.cool = c.cool.saturating_sub(1);
            if c.elevation_m().is_some() {
                if let Some(exit) = (c.cool == 0).then(|| self.natural_exit(c)).flatten() {
                    self.leave_at(ai, &c, Some(exit));
                } else {
                    self.agents[ai].cave = Some(c);
                }
                continue;
            }
            // the stairs
            let cell = self.caves.get(&c.node).and_then(|cv| cv.level(c.level)).map_or(Cell::Floor, |l| l.cell(c.tile().0, c.tile().1));
            let mut gone = false;
            if c.cool == 0 {
                match cell {
                    Cell::Exit => {
                        self.leave_cave(ai, &c);
                        gone = true;
                    }
                    Cell::Up if c.level > 1 => {
                        let to = self.caves.get(&c.node).and_then(|cv| cv.level(c.level - 1)).and_then(|l| l.down);
                        if let Some(d) = to {
                            let lv = &self.caves[&c.node].levels[(c.level - 2) as usize];
                            let (x, y) = centre(lv.landing(d));
                            c.level -= 1;
                            c.x = x;
                            c.y = y;
                            c.cool = STAIR_COOL;
                            self.new_level(ai, &c);
                        }
                    }
                    Cell::Down => {
                        let depth_to = c.level;
                        let need = rules_now.cave_tier(depth_to);
                        let have = pick_tier(&rules_now, self.agents[ai].kind, &self.agents[ai].inventory);
                        let flooded = depth_to == CAVE_MAX_DEPTH && !self.pumped(c.node);
                        if flooded {
                            self.events.push(Event::Rejected(id, Reject::Flooded));
                        } else if have.is_none_or(|t| t < need) {
                            self.events.push(Event::Rejected(id, Reject::NeedsTool { tool: ToolKind::Pick, tier: need }));
                        }
                        if flooded || have.is_none_or(|t| t < need) {
                            // turned back at the top of the stair
                            let lv = &self.caves[&c.node].levels[(c.level - 1) as usize];
                            let d = lv.down.unwrap_or(c.tile());
                            let (x, y) = centre(lv.landing(d));
                            c.x = x;
                            c.y = y;
                            c.cool = STAIR_COOL * 2;
                            self.agents[ai].task = Task::Idle;
                            self.agents[ai].path.clear();
                        } else if c.level < CAVE_LEVELS {
                            let lv = &self.caves[&c.node].levels[c.level as usize];
                            let (x, y) = centre(lv.landing(lv.entry));
                            c.level += 1;
                            c.x = x;
                            c.y = y;
                            c.cool = STAIR_COOL;
                            if c.level - 1 == CAVE_MAX_DEPTH && self.milestones & Milestone::DrainMine.bit() == 0 {
                                self.milestones |= Milestone::DrainMine.bit();
                                self.events.push(Event::Milestone(Milestone::DrainMine));
                            }
                            self.new_level(ai, &c);
                        }
                    }
                    _ => {}
                }
            }
            if gone {
                continue;
            }
            self.agents[ai].cave = Some(c);
            // on its own way out: on to the next stair
            if c.leaving && self.agents[ai].task == Task::Idle {
                let (x, y) = self.caves.get(&c.node).and_then(|cv| cv.level(c.level)).map_or((c.x, c.y), |l| centre(l.entry));
                self.agents[ai].task = Task::Move { x, y };
            }
        }
    }

    /// Arrived on another level: the old plan means nothing here.
    fn new_level(&mut self, ai: usize, c: &CaveSelf) {
        let id = self.agents[ai].id;
        let a = &mut self.agents[ai];
        a.cave = Some(*c);
        a.path.clear();
        a.path_goal = (-1, -1);
        a.task = Task::Idle;
        self.events.push(Event::CaveLevel { agent: id, level: c.level });
    }

    /// Out into the light: the surface, in front of the mouth.
    fn leave_cave(&mut self, ai: usize, c: &CaveSelf) {
        let exit = self.nodes.get(c.node as usize).map(|n| n.center_mm()).map(|(x, y)| (x, y + 1_500));
        self.leave_at(ai, c, exit);
    }

    fn leave_at(&mut self, ai: usize, c: &CaveSelf, exit: Option<(i64, i64)>) {
        let id = self.agents[ai].id;
        let a = &mut self.agents[ai];
        a.cave = None;
        a.task = Task::Idle;
        a.path.clear();
        a.path_goal = (-1, -1);
        if let Some((mx, my)) = exit {
            a.x = mx;
            a.y = my;
        }
        self.events.push(Event::CaveLeft { agent: id, dark: c.light == 0 });
    }

    /// The wall a pick would best work from here: the nearest ore vein on the level, else the nearest bare wall.
    pub fn cave_dig_target(&self, id: AgentId) -> Option<(i32, i32)> {
        let c = self.agents.get(id.0 as usize)?.cave?;
        if c.elevation_m().is_some() {
            let (x, y) = c.tile();
            return crate::cave_terrain::SIDES.into_iter().map(|(dx, dy)| (x + dx, y + dy))
                .filter_map(|(x, y)| match self.natural_cave_cell(c, x, y) {
                    Some(Cell::Rubble) => Some((0, (x, y))),
                    Some(Cell::Rock) => Some((1 + u8::from(self.natural_cave_ore(c, x, y).is_none()), (x, y))),
                    _ => None,
                })
                .min_by_key(|&(rank, _)| rank)
                .map(|(_, tile)| tile);
        }
        let lv = self.caves.get(&c.node)?.level(c.level)?;
        let (px, py) = (c.x, c.y);
        let mut best: Option<(i64, (i32, i32))> = None;
        for y in 1..crate::caves::CAVE_H - 1 {
            for x in 1..crate::caves::CAVE_W - 1 {
                let open = [(1, 0), (0, 1), (-1, 0), (0, -1)].iter().any(|&(dx, dy)| lv.walkable(x + dx, y + dy));
                if !open || !lv.diggable(x, y) {
                    continue;
                }
                let (cx, cy) = centre((x, y));
                // (a vein counts as much nearer than bare rock)
                let d = dist2(cx, cy, px, py) / if lv.ore_at(x, y).is_some() { 16 } else { 1 };
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, (x, y)));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Where the stair down stands on the level, in mm.
    pub fn cave_down_spot(&self, id: AgentId) -> Option<(i64, i64)> {
        let c = self.agents.get(id.0 as usize)?.cave?;
        if c.elevation_m().is_some() {
            return self.natural_deeper_spot(c);
        }
        let lv = self.caves.get(&c.node)?.level(c.level)?;
        lv.down.map(centre)
    }

    /// Which way out lies, for the HUD: the first bend of the way from here to the stair up (or the exit), as an
    /// offset in mm, and the walking distance in tiles.
    pub fn cave_way_out(&self, id: AgentId) -> Option<((i64, i64), i32)> {
        let a = self.agents.get(id.0 as usize)?;
        let c = a.cave?;
        if c.elevation_m().is_some() {
            return self.natural_way_out(c, a);
        }
        let lv = self.caves.get(&c.node)?.level(c.level)?;
        let route = lv.find_path(c.tile(), lv.entry)?;
        let wp = lv.smooth((c.x, c.y), &route);
        let first = centre(*wp.first()?);
        Some(((first.0 - c.x, first.1 - c.y), route.len() as i32))
    }
}
