use super::*;
use crate::cave_terrain::{CaveMouth, CaveNav, CaveTile, SIDES};
use crate::caves::{CaveSelf, Cell, ore_item, vein_ore};
use crate::rules::{Gear, Rules, ToolKind};
use crate::world::cave_play::{CAVE_MINE_TICKS, ROCK_SWING_TICKS, RUBBLE_SWING_TICKS};
use std::collections::BTreeMap;

fn centre(x: i32, y: i32) -> (i64, i64) {
    CaveTile { x, y, z: 0 }.center_mm()
}

fn cave_edit_key(tile: CaveTile) -> u64 {
    crate::rng::hash2((tile.z as u64).wrapping_mul(0xCA7E_2026), tile.x, tile.y)
}

fn rubble_key(tile: CaveTile) -> u64 {
    cave_edit_key(tile).rotate_left(19)
}

fn rockfall_roll(seed: u64, tile: CaveTile, tick: u64) -> u64 {
    crate::rng::hash2(
        seed ^ cave_edit_key(tile) ^ 0xCA7E_F411,
        tick as i32,
        (tick >> 32) as i32,
    )
}

fn finish_task(a: &mut Agent) {
    a.task = Task::Idle;
    a.path.clear();
    a.path_goal = (-1, -1);
}

fn cave_sight_line(start: CaveTile, end: CaveTile, floors: &BTreeMap<(i32, i32), i16>) -> bool {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let steps = dx.abs().max(dy.abs());
    let mut previous = start.xy();
    for i in 1..=steps {
        let offset = |d: i32| (d * i + d.signum() * (steps / 2)) / steps;
        let eye = (i32::from(start.z) + 1) * (steps - i) + i32::from(end.z) * i;
        let clear = |xy| {
            floors
                .get(&xy)
                .is_some_and(|&z| i32::from(z) * steps <= eye)
        };
        let next = (start.x + offset(dx), start.y + offset(dy));
        if i < steps && !clear(next) {
            return false;
        }
        if next.0 != previous.0
            && next.1 != previous.1
            && !clear((next.0, previous.1))
            && !clear((previous.0, next.1))
        {
            return false;
        }
        previous = next;
    }
    true
}

impl World {
    pub(crate) fn load_cave_edit(&mut self, tile: CaveTile) {
        if self.dug_cave.insert(tile) {
            self.dug_cave_digest ^= cave_edit_key(tile);
        }
    }

    pub(crate) fn load_cave_rubble(&mut self, tile: CaveTile) {
        if self.cave_rubble.insert(tile) {
            self.dug_cave_digest ^= rubble_key(tile);
        }
    }

    fn natural_nav(&self) -> CaveNav<'_> {
        CaveNav::new(&self.map, &self.dug_cave).with_rubble(&self.cave_rubble)
    }

    pub fn natural_cave_rubble_at(&self, tile: CaveTile) -> bool {
        self.cave_rubble.contains(&tile)
    }

    pub fn natural_mouth(&self, node: u32) -> Option<CaveMouth> {
        let n = self.nodes.get(node as usize)?;
        (n.kind == NodeKind::CaveMouth).then(|| self.map.natural_cave_mouth(n.tx, n.ty))?
    }

    pub fn natural_cave_tiles(&self, c: CaveSelf, radius: i32) -> Vec<CaveTile> {
        let Some(tile) = c.natural_tile() else {
            return Vec::new();
        };
        self.natural_nav().nearby_floors(tile, radius.clamp(0, 32))
    }

    /// What the player can discover with the current light. Remembered tiles belong to the local map.
    pub fn natural_cave_sight(&self, c: CaveSelf) -> BTreeMap<CaveTile, Cell> {
        let Some(start) = c.natural_tile() else {
            return BTreeMap::new();
        };
        let radius = if c.light > 0 { 7 } else { 1 };
        let mut nav = self.natural_nav();
        let tiles = nav.nearby_floors(start, radius);
        let floors = tiles.iter().map(|tile| (tile.xy(), tile.z)).collect();
        let in_range = |tile: CaveTile| {
            (tile.x - start.x).pow(2) + (tile.y - start.y).pow(2) <= radius * radius
        };
        let mut visible = BTreeMap::new();
        for tile in tiles
            .into_iter()
            .filter(|&tile| in_range(tile) && cave_sight_line(start, tile, &floors))
        {
            let cell = if self.natural_cave_exit_at(tile).is_some() {
                Cell::Exit
            } else if self.natural_cave_flooded(tile) {
                Cell::Deep
            } else {
                Cell::Floor
            };
            visible.insert(tile, cell);
            for (dx, dy) in SIDES {
                if let Some(rubble) = nav.adjacent_rubble(tile, tile.x + dx, tile.y + dy)
                    && in_range(rubble) && cave_sight_line(start, rubble, &floors) {
                        visible.insert(rubble, Cell::Rubble);
                    }
            }
        }
        visible
    }

    pub fn natural_cave_revision(&self) -> u64 {
        self.dug_cave_digest
    }

    pub fn natural_cave_cell(&self, c: CaveSelf, x: i32, y: i32) -> Option<Cell> {
        let tile = c.natural_tile()?;
        let mut nav = self.natural_nav();
        if nav.adjacent_rubble(tile, x, y).is_some() {
            Some(Cell::Rubble)
        } else if nav.walls(tile, x, y) {
            Some(Cell::Rock)
        } else if tile.xy() == (x, y) || nav.next_floor(tile, x, y).is_some() {
            Some(Cell::Floor)
        } else {
            None
        }
    }

    pub fn natural_cave_ore(&self, c: CaveSelf, x: i32, y: i32) -> Option<u8> {
        let from = c.natural_tile()?;
        self.natural_cave_ore_at(CaveTile { x, y, z: from.z })
    }

    pub fn natural_cave_ore_at(&self, tile: CaveTile) -> Option<u8> {
        let roll = crate::rng::hash2(
            self.seed ^ (tile.z as u64).wrapping_mul(0x61DE3),
            tile.x,
            tile.y,
        );
        (roll % 100 < 19).then(|| vein_ore(CaveSelf::natural_depth_at(tile.z), roll / 100))
    }

    pub fn natural_cave_flooded(&self, tile: CaveTile) -> bool {
        let (x, y) = tile.center_mm();
        CaveSelf::natural_depth_at(tile.z) == crate::world::cave_play::CAVE_MAX_DEPTH
            && !self.pumped_at(x, y)
    }

    pub(super) fn natural_exit(&self, c: CaveSelf) -> Option<(i64, i64)> {
        self.natural_cave_exit_at(c.natural_tile()?)
    }

    pub fn natural_cave_exit_at(&self, tile: CaveTile) -> Option<(i64, i64)> {
        SIDES.into_iter().find_map(|(dx, dy)| {
            let (x, y) = (tile.x + dx, tile.y + dy);
            let mouth = self.map.natural_cave_mouth(x, y)?;
            (tile == mouth.threshold).then(|| centre(x, y))
        })
    }

    pub(super) fn natural_leave_command(&mut self, ai: usize, mut c: CaveSelf) {
        let Some(start) = c.natural_tile() else {
            return;
        };
        let rules = self.rules_arc();
        let a = &self.agents[ai];
        let pick = pick_tier(&rules, a.kind, &a.inventory);
        let route = self.natural_nav().route_to(
            start,
            |tile| self.natural_cave_exit_at(tile).is_some(),
            |from, to| self.natural_step_access(from, to, pick, &rules).is_ok(),
        );
        let a = &mut self.agents[ai];
        if let Some(route) = route {
            let end = route.first().copied().unwrap_or(start);
            a.path = route.into_iter().map(CaveTile::xy).collect();
            a.path_goal = end.xy();
            let (x, y) = end.center_mm();
            a.task = Task::Move { x, y };
            c.leaving = true;
        } else {
            finish_task(a);
            self.events.push(Event::Rejected(a.id, Reject::Unreachable));
            c.leaving = false;
        }
        a.cave = Some(c);
    }

    fn natural_step_access(
        &self,
        from: CaveTile,
        to: CaveTile,
        pick: Option<u8>,
        rules: &Rules,
    ) -> Result<(), Reject> {
        if self.natural_cave_flooded(to) && !self.natural_cave_flooded(from) {
            return Err(Reject::Flooded);
        }
        let depth = CaveSelf::natural_depth_at(to.z);
        if depth > CaveSelf::natural_depth_at(from.z) {
            let tier = rules.cave_tier(depth);
            if pick.is_none_or(|have| have < tier) {
                return Err(Reject::NeedsTool {
                    tool: ToolKind::Pick,
                    tier,
                });
            }
        }
        Ok(())
    }

    fn natural_move_route(
        &self,
        nav: &mut CaveNav<'_>,
        start: CaveTile,
        goal: (i32, i32),
        pick: Option<u8>,
        rules: &Rules,
    ) -> Result<Vec<CaveTile>, Reject> {
        let access = |from, to| self.natural_step_access(from, to, pick, rules);
        if let Some(route) = nav.route_to(
            start,
            |tile| tile.xy() == goal,
            |from, to| access(from, to).is_ok(),
        ) {
            return Ok(route);
        }
        // Preserve the tool or water hint when the target has a route on foot but the player can't use it.
        if let Some(route) = nav.route(start, goal, None) {
            let mut previous = start;
            for tile in route.into_iter().rev() {
                access(previous, tile)?;
                previous = tile;
            }
        }
        Err(Reject::Unreachable)
    }

    pub(super) fn natural_way_out(&self, c: CaveSelf, a: &Agent) -> Option<((i64, i64), i32)> {
        let (x, y) = *a.path.last()?;
        let (wx, wy) = centre(x, y);
        c.leaving
            .then_some(((wx - c.x, wy - c.y), a.path.len() as i32))
    }

    pub(super) fn natural_deeper_spot(&self, c: CaveSelf) -> Option<(i64, i64)> {
        let current = c.natural_tile()?;
        self.natural_cave_tiles(c, 16)
            .into_iter()
            .find(|tile| tile.z < current.z && tile.xy() != current.xy())
            .map(CaveTile::center_mm)
    }

    pub(super) fn run_natural_cave_tasks(&mut self) {
        if !crate::map::has_natural_caves(self.seed) {
            return;
        }
        let rules = self.rules_arc();
        for ai in 0..self.agents.len() {
            let Some(c) = self.agents[ai].cave else {
                continue;
            };
            let Some(tile) = c.natural_tile() else {
                continue;
            };
            match self.agents[ai].task {
                Task::Walk { x, y } => self.natural_move(ai, c, tile, (x, y), false, &rules),
                Task::Move { x, y } => self.natural_move(ai, c, tile, (x, y), true, &rules),
                Task::Dig { tx, ty, progress } => {
                    self.natural_dig(ai, c, tile, tx, ty, progress, &rules)
                }
                _ => {}
            }
        }
    }

    fn natural_move(
        &mut self,
        ai: usize,
        mut c: CaveSelf,
        tile: CaveTile,
        target: (i64, i64),
        routed: bool,
        rules: &Rules,
    ) {
        if c.leaving && self.agents[ai].path.is_empty() && self.natural_cave_exit_at(tile).is_none()
        {
            self.natural_leave_command(ai, c);
            return;
        }
        let (gx, gy) = (target.0.div_euclid(TILE_MM), target.1.div_euclid(TILE_MM));
        if gx < 0 || gy < 0 || gx >= i64::from(self.map.w) || gy >= i64::from(self.map.h) {
            finish_task(&mut self.agents[ai]);
            return;
        }
        let goal = (gx as i32, gy as i32);
        if !routed {
            let a = &mut self.agents[ai];
            a.path.clear();
            a.path_goal = (-1, -1);
        }
        let a = &self.agents[ai];
        let pick = pick_tier(rules, a.kind, &a.inventory);
        let mut nav = CaveNav::new(&self.map, &self.dug_cave).with_rubble(&self.cave_rubble);
        let waypoint = if routed {
            let cached = a
                .path
                .last()
                .copied()
                .filter(|_| a.path_goal == goal)
                .filter(|&(x, y)| {
                    tile.xy() == (x, y)
                        || nav.next_floor(tile, x, y).is_some_and(|next| {
                            self.natural_step_access(tile, next, pick, rules).is_ok()
                        })
                });
            if let Some(next) = cached {
                next
            } else if c.leaving && goal != tile.xy() {
                self.natural_leave_command(ai, c);
                return;
            } else {
                let path = self.natural_move_route(&mut nav, tile, goal, pick, rules);
                let a = &mut self.agents[ai];
                match path {
                    Ok(path) => {
                        a.path = path.into_iter().map(CaveTile::xy).collect();
                        a.path_goal = goal;
                        a.path.last().copied().unwrap_or(goal)
                    }
                    Err(reason) => {
                        finish_task(a);
                        c.leaving = false;
                        a.cave = Some(c);
                        self.events.push(Event::Rejected(a.id, reason));
                        return;
                    }
                }
            }
        } else if goal == tile.xy() {
            goal
        } else {
            let (dx, dy) = (target.0 - c.x, target.1 - c.y);
            let sides = if dx.abs() >= dy.abs() {
                [(dx.signum(), 0), (0, dy.signum())]
            } else {
                [(0, dy.signum()), (dx.signum(), 0)]
            };
            sides
                .into_iter()
                .map(|(x, y)| (tile.x + x as i32, tile.y + y as i32))
                .find(|&(x, y)| nav.next_floor(tile, x, y).is_some())
                .unwrap_or(tile.xy())
        };
        let next = if waypoint == tile.xy() {
            Some(tile)
        } else {
            nav.next_floor(tile, waypoint.0, waypoint.1)
        };
        let Some(next) = next else {
            if c.leaving {
                self.natural_leave_command(ai, c);
            } else {
                finish_task(&mut self.agents[ai]);
            }
            return;
        };
        let depth = CaveSelf::natural_depth_at(next.z);
        let already_wet = self.natural_cave_flooded(tile);
        let access = self.natural_step_access(tile, next, pick, rules);
        let a = &mut self.agents[ai];
        if let Err(rejection) = access {
            if c.leaving {
                self.natural_leave_command(ai, c);
            } else {
                self.events.push(Event::Rejected(a.id, rejection));
                finish_task(a);
            }
            return;
        }
        let (tx, ty) = if next.xy() == goal && goal == tile.xy() && !c.leaving {
            target
        } else {
            next.center_mm()
        };
        let (dx, dy) = (tx - c.x, ty - c.y);
        let length = ((dx * dx + dy * dy) as u64).isqrt() as i64;
        let mut speed = a.kind.speed_mm() * (1000 - i64::from(a.inventory.gear().burden)) / 1000;
        if c.light == 0 {
            speed = speed * 60 / 100;
        }
        if a.broken > 0 {
            speed = speed * 55 / 100;
        }
        if already_wet {
            speed = speed * 45 / 100;
        }
        let speed = speed.max(1);
        if length <= speed {
            c.x = tx;
            c.y = ty;
            if routed && a.path.last() == Some(&waypoint) {
                a.path.pop();
            }
            if waypoint == goal && a.path.is_empty() {
                finish_task(a);
            }
        } else {
            c.x += dx * speed / length;
            c.y += dy * speed / length;
        }
        if c.tile() == next.xy() {
            if c.depth() != depth {
                self.events.push(Event::CaveLevel {
                    agent: a.id,
                    level: depth + 1,
                });
                if depth == crate::world::cave_play::CAVE_MAX_DEPTH
                    && self.milestones & Milestone::DrainMine.bit() == 0
                {
                    self.milestones |= Milestone::DrainMine.bit();
                    self.events.push(Event::Milestone(Milestone::DrainMine));
                }
            }
            c.set_elevation(next.z);
        }
        a.x = c.x;
        a.y = c.y;
        a.cave = Some(c);
    }

    fn natural_dig(
        &mut self,
        ai: usize,
        c: CaveSelf,
        from: CaveTile,
        x: i32,
        y: i32,
        progress: u32,
        rules: &Rules,
    ) {
        let mut nav = self.natural_nav();
        let rubble = nav.adjacent_rubble(from, x, y);
        let opened = if rubble.is_none() && nav.walls(from, x, y) {
            [from.z, from.z + 1]
                .map(|z| CaveTile { x, y, z })
                .map(|voxel| nav.is_rock(voxel).then_some(voxel))
        } else if rubble.is_some() {
            [None; 2]
        } else {
            finish_task(&mut self.agents[ai]);
            return;
        };
        if opened.iter().flatten().any(|voxel| {
            let floor = CaveTile {
                z: voxel.z + 1,
                ..*voxel
            };
            self.is_mouth_floor(floor)
                || self
                    .agents
                    .iter()
                    .any(|a| a.cave.and_then(|c| c.natural_tile()) == Some(floor))
        }) {
            let a = &mut self.agents[ai];
            finish_task(a);
            self.events.push(Event::Rejected(a.id, Reject::Blocked));
            return;
        }
        let ore = rubble
            .is_none()
            .then(|| self.natural_cave_ore(c, x, y).and_then(ore_item))
            .flatten();
        let a = &mut self.agents[ai];
        let tool = rules.best_tool(ToolKind::Pick, |item| a.inventory.get(item) > 0);
        if tool.is_none() && rubble.is_none() {
            self.events.push(Event::Rejected(
                a.id,
                Reject::NeedsTool {
                    tool: ToolKind::Pick,
                    tier: 0,
                },
            ));
            finish_task(a);
            return;
        }
        let spare = a.capacity().saturating_sub(a.inventory.total());
        let rewards = if rubble.is_some() { spare.min(1) } else { 2 };
        if rubble.is_none() && spare < rewards {
            self.events.push(Event::Rejected(a.id, Reject::PackFull));
            finish_task(a);
            return;
        }
        let life = tool.map_or(0, |tool| tool_life(tool, &self.settings));
        let blunt = tool.is_some_and(|tool| life > 0 && a.wear_of(tool.item) >= life);
        let base = if rubble.is_some() {
            if tool.is_some() && !blunt {
                RUBBLE_SWING_TICKS
            } else {
                RUBBLE_SWING_TICKS * 3 / 2
            }
        } else if ore.is_some() {
            u32::from(CAVE_MINE_TICKS) * if blunt { 3 } else { 1 }
        } else {
            ROCK_SWING_TICKS * if blunt { 3 } else { 1 }
        };
        if progress < base - 1 {
            a.task = Task::Dig {
                tx: x,
                ty: y,
                progress: progress + 1,
            };
            return;
        }
        let roll = rockfall_roll(self.seed, from, self.tick);
        if rubble.is_none() && roll % 100 < 5 + 7 * u64::from(c.depth()) {
            self.natural_rockfall(ai, from, c.depth(), roll / 100);
            return;
        }
        if let Some(tile) = rubble {
            self.cave_rubble.remove(&tile);
            self.dug_cave_digest ^= rubble_key(tile);
        }
        for voxel in opened.into_iter().flatten() {
            if self.dug_cave.insert(voxel) {
                self.dug_cave_digest ^= cave_edit_key(voxel);
            }
        }
        for item in [ItemKind::Stone, ore.unwrap_or(ItemKind::Stone)]
            .into_iter()
            .take(usize::from(rewards))
        {
            a.inventory.counts[item as usize] += 1;
            self.events.push(Event::Gathered { agent: a.id, item });
        }
        if let Some(tool) = tool
            && life > 0 && a.add_wear(tool.item) == life {
                self.events.push(Event::ToolBlunt {
                    agent: a.id,
                    item: tool.item,
                });
            }
        finish_task(a);
    }

    fn natural_rockfall(&mut self, ai: usize, from: CaveTile, depth: u8, salt: u64) {
        let a = &mut self.agents[ai];
        let blow = a.inventory.gear().blow;
        a.health -= Gear::cut(1_500, blow);
        a.injury += Gear::cut(600, blow);
        self.events.push(Event::CaveRockfall(a.id));
        if depth >= 2 && salt.is_multiple_of(2) && a.broken == 0 {
            a.broken = BREAK_TICKS;
            self.events.push(Event::Fractured(a.id));
        }
        finish_task(a);

        let occupied = self
            .agents
            .iter()
            .filter_map(|a| a.cave.and_then(|c| c.natural_tile()))
            .collect::<std::collections::BTreeSet<_>>();
        let mut candidates = self.natural_nav().nearby_floors(from, 2);
        candidates.retain(|tile| {
            !occupied.contains(tile)
                && !self.natural_cave_flooded(*tile)
                && !self.is_mouth_floor(*tile)
        });
        candidates.sort_by_key(|tile| (cave_edit_key(*tile) ^ salt, *tile));
        candidates.truncate(1 + (salt % 2) as usize);
        for &tile in &candidates {
            self.load_cave_rubble(tile);
        }
        for i in 0..self.agents.len() {
            let a = &self.agents[i];
            if let Some(cave) = a.cave.filter(|c| c.elevation_m().is_some() && c.leaving)
                && candidates.iter().any(|tile| a.path.contains(&tile.xy()))
            {
                self.natural_leave_command(i, cave);
            }
        }
    }

    fn is_mouth_floor(&self, tile: CaveTile) -> bool {
        SIDES.into_iter().any(|(dx, dy)| {
            [1, 2].into_iter().any(|step| {
                self.map
                    .natural_cave_mouth(tile.x - dx * step, tile.y - dy * step)
                    .is_some_and(|mouth| mouth.threshold == tile || mouth.landing == tile)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cave_sight_stops_at_walls_and_rising_floors() {
        let start = CaveTile { x: 0, y: 0, z: 0 };
        let end = CaveTile { x: 2, ..start };
        let mut floors = BTreeMap::from([((0, 0), 0), ((2, 0), 0)]);
        assert!(!cave_sight_line(start, end, &floors));
        floors.insert((1, 0), 3);
        assert!(!cave_sight_line(start, end, &floors));
        floors.insert((1, 0), 0);
        assert!(cave_sight_line(start, end, &floors));
        assert!(!cave_sight_line(
            start,
            CaveTile { x: -1, y: -1, z: 0 },
            &floors
        ));
    }

    #[test]
    fn cave_click_routes_around_a_descent_that_requires_a_pick() {
        let seed = crate::map::tag_seed_natural(42, 256);
        let mut w = World::with_size(seed, 256, 256);
        let mut nav = w.natural_nav();
        let (x, y) = (10..246)
            .flat_map(|y| (10..246).map(move |x| (x, y)))
            .find(|&(x, y)| {
                (0..3).all(|dx| {
                    (0..2).all(|dy| {
                        (-17..=-13).all(|z| {
                            nav.is_rock(CaveTile {
                                x: x + dx,
                                y: y + dy,
                                z,
                            })
                        })
                    })
                })
            })
            .expect("solid rock for the two routes");
        let start = CaveTile { x, y, z: -15 };
        let shortcut = CaveTile {
            x: x + 1,
            z: -16,
            ..start
        };
        let end = CaveTile { x: x + 2, ..start };
        let detour = [0, 1, 2].map(|dx| CaveTile {
            x: x + dx,
            y: y + 1,
            ..start
        });
        for floor in [start, end, shortcut].into_iter().chain(detour) {
            for dz in 0..if floor == shortcut { 3 } else { 2 } {
                w.load_cave_edit(CaveTile {
                    z: floor.z + dz,
                    ..floor
                });
            }
        }
        let id = w.spawn_agent(start.center_mm().0, start.center_mm().1);
        let (x, y) = start.center_mm();
        w.agents[0].cave = Some(CaveSelf {
            node: w.node_at(222, 150).unwrap().0,
            level: (start.z + 100) as u8,
            x,
            y,
            light: 600,
            full: 600,
            dark: 0,
            cool: 0,
            leaving: false,
        });
        assert_eq!(
            w.natural_nav().route(start, end.xy(), Some(end.z)),
            Some(vec![end, shortcut])
        );
        let (x, y) = end.center_mm();
        w.step(&[Command {
            agent: id,
            kind: CommandKind::MoveTo { x, y },
        }]);
        assert_eq!(w.agents[0].path.last(), Some(&detour[0].xy()));
        for _ in 0..400 {
            if w.agents[0].task == Task::Idle {
                break;
            }
            w.step(&[]);
        }
        assert_eq!(w.agents[0].cave.unwrap().natural_tile(), Some(end));
    }

    #[test]
    fn cave_mining_keeps_the_support_under_the_entrance() {
        let seed = crate::map::tag_seed_natural(42, 256);
        let mut w = World::with_size(seed, 256, 256);
        let mut nav = w.natural_nav();
        let (node, floor, from) = w
            .nodes
            .iter()
            .enumerate()
            .find_map(|(id, _)| {
                let mouth = w.natural_mouth(id as u32)?;
                [mouth.threshold, mouth.landing]
                    .into_iter()
                    .find_map(|floor| {
                        let support_z = floor.z - 1;
                        if !nav.is_rock(CaveTile {
                            z: support_z - 1,
                            ..floor
                        }) {
                            return None;
                        }
                        SIDES.into_iter().find_map(|(dx, dy)| {
                            let from = CaveTile {
                                x: floor.x + dx,
                                y: floor.y + dy,
                                z: support_z,
                            };
                            (w.map.in_bounds(from.x, from.y)
                                && [-1, 0, 1, 2].into_iter().all(|dz| {
                                    nav.is_rock(CaveTile {
                                        z: from.z + dz,
                                        ..from
                                    })
                                }))
                            .then_some((id as u32, floor, from))
                        })
                    })
            })
            .expect("an entrance above a solid wall");
        for z in [from.z, from.z + 1] {
            w.load_cave_edit(CaveTile { z, ..from });
        }
        assert!(w.natural_nav().walls(from, floor.x, floor.y));
        let (x, y) = from.center_mm();
        let id = w.spawn_agent(x, y);
        w.agents[0].inventory.counts[ItemKind::IronPickaxe as usize] = 1;
        let c = CaveSelf {
            node,
            level: (from.z + 100) as u8,
            x,
            y,
            light: 600,
            full: 600,
            dark: 0,
            cool: 0,
            leaving: false,
        };
        w.agents[0].cave = Some(c);
        w.step(&[Command {
            agent: id,
            kind: CommandKind::CaveDig {
                tx: floor.x,
                ty: floor.y,
            },
        }]);
        assert!(
            w.events.iter().any(
                |event| matches!(event, Event::Rejected(agent, Reject::Blocked) if *agent == id)
            )
        );
        assert!(w.natural_nav().floor(floor));
    }

    #[test]
    fn cave_leave_chooses_the_nearer_mouth_and_replans_when_blocked() {
        let seed = crate::map::tag_seed_natural(42, 256);
        let mut w = World::with_size(seed, 256, 256);
        let entry = w.node_at(222, 150).unwrap();
        let entry_mouth = w.natural_mouth(entry.0).unwrap();
        let floors = w.natural_nav().nearby_floors(entry_mouth.landing, 32);
        let mut fixture = None;
        for start in floors.into_iter().filter(|&tile| !w.is_mouth_floor(tile)) {
            let exits = |tile| w.natural_cave_exit_at(tile).is_some();
            let Some(route) = w.natural_nav().route_to(start, exits, |_, _| true) else {
                continue;
            };
            let (Some(&first), Some(&blocked)) = (route.first(), route.last()) else {
                continue;
            };
            if first == entry_mouth.threshold || w.is_mouth_floor(blocked) {
                continue;
            }
            let pile = std::collections::BTreeSet::from([blocked]);
            let mut nav = CaveNav::new(&w.map, &w.dug_cave).with_rubble(&pile);
            let Some(alternate) = nav
                .route_to(start, exits, |_, _| true)
                .and_then(|route| route.first().copied())
            else {
                continue;
            };
            if fixture.is_none() || alternate != first {
                fixture = Some((start, first, blocked, alternate));
            }
            if alternate != first {
                break;
            }
        }
        let (start, first, blocked, alternate) =
            fixture.expect("a known exit route with a walkable detour");
        let expected_exit = w.natural_cave_exit_at(alternate).unwrap();

        let (x, y) = w.nodes[entry.0 as usize].center_mm();
        let id = w.spawn_agent(x, y);
        w.agents[0].inventory.counts[ItemKind::Torch as usize] = 2;
        w.agents[0].inventory.counts[ItemKind::IronPickaxe as usize] = 1;
        w.step(&[Command {
            agent: id,
            kind: CommandKind::EnterCave { node: entry },
        }]);
        let a = &mut w.agents[0];
        let cave = a.cave.as_mut().unwrap();
        (cave.x, cave.y) = start.center_mm();
        cave.set_elevation(start.z);
        cave.cool = 0;
        (a.x, a.y) = (cave.x, cave.y);

        w.step(&[Command {
            agent: id,
            kind: CommandKind::CaveLeave,
        }]);
        assert_eq!(w.agents[0].path_goal, first.xy());
        assert!(w.cave_way_out(id).is_some());
        w.load_cave_rubble(blocked);
        w.step(&[]);
        assert_eq!(w.agents[0].path_goal, alternate.xy());
        assert!(!w.agents[0].path.contains(&blocked.xy()));
        for _ in 0..1_400 {
            if w.agents[0].cave.is_none() {
                break;
            }
            w.step(&[]);
        }
        assert!(w.agents[0].cave.is_none(), "an alternate way led outside");
        assert_eq!((w.agents[0].x, w.agents[0].y), expected_exit);
        assert!(w.natural_cave_rubble_at(blocked));
    }

    #[test]
    fn a_working_pump_opens_a_flooded_natural_passage() {
        let seed = crate::map::tag_seed_natural(42, 256);
        let mut w = World::with_size(seed, 256, 256);
        let node = w.node_at(222, 150).unwrap();
        let mut nav = w.natural_nav();
        let (from, into) = (10..246)
            .flat_map(|y| (10..246).map(move |x| CaveTile { x, y, z: -59 }))
            .filter(|tile| {
                w.map
                    .terrain(tile.x, tile.y)
                    .is_some_and(|ground| ground != Terrain::Water)
            })
            .find_map(|tile| {
                if !nav.floor(tile) {
                    return None;
                }
                let next = SIDES.into_iter().find_map(|(dx, dy)| {
                    nav.next_floor(tile, tile.x + dx, tile.y + dy)
                        .filter(|next| next.z == -60)
                })?;
                Some((tile, next))
            })
            .expect("a natural slope into the flooded depth");
        let (x, y) = w.nodes[node.0 as usize].center_mm();
        let id = w.spawn_agent(x, y);
        w.agents[0].inventory.counts[ItemKind::Torch as usize] = 2;
        w.agents[0].inventory.counts[ItemKind::IronPickaxe as usize] = 1;
        w.step(&[Command {
            agent: id,
            kind: CommandKind::EnterCave { node },
        }]);
        // Start at the brink so this test measures drainage independently of the journey down.
        let a = &mut w.agents[0];
        let mut cave = a.cave.unwrap();
        (cave.x, cave.y) = from.center_mm();
        cave.set_elevation(from.z);
        a.cave = Some(cave);
        (a.x, a.y) = (cave.x, cave.y);
        let (x, y) = into.center_mm();
        let walk = Command {
            agent: id,
            kind: CommandKind::MoveTo { x, y },
        };
        assert!(w.natural_cave_flooded(into));
        w.step(std::slice::from_ref(&walk));
        assert!(
            w.events.iter().any(
                |event| matches!(event, Event::Rejected(agent, Reject::Flooded) if *agent == id)
            )
        );
        assert_eq!(w.agents[0].cave.unwrap().natural_tile(), Some(from));

        w.buildings
            .push(Building::new(BuildingKind::SteamPump, x + 3_000, y));
        let mut engine = Building::new(BuildingKind::SteamEngine, x + 6_000, y);
        let recipe = w
            .rules()
            .recipes
            .iter()
            .position(|r| r.id == "raise_steam")
            .unwrap();
        engine.job = Some(crate::buildings::Job {
            recipe: recipe as u16,
            remaining: 3_600,
        });
        engine.temp = 300;
        w.buildings.push(engine);
        for _ in 0..40 {
            w.step(&[]);
        }
        assert!(!w.natural_cave_flooded(into));
        w.step(&[walk]);
        for _ in 0..120 {
            if w.agents[0].cave.unwrap().natural_tile() == Some(into) {
                break;
            }
            w.step(&[]);
        }
        assert_eq!(w.agents[0].cave.unwrap().natural_tile(), Some(into));
        assert!(w.milestone_done(Milestone::DrainMine));
    }

    #[test]
    fn a_natural_rockfall_blocks_a_route_and_survives_loading() {
        let seed = crate::map::tag_seed_natural(42, 256);
        let mut world = World::with_size(seed, 256, 256);
        let entry = world.node_at(222, 150).unwrap();
        let other = world.node_at(218, 166).unwrap();
        let (x, y) = world.nodes[entry.0 as usize].center_mm();
        let miner = world.spawn_agent(x, y);
        world.agents[0].inventory.counts[ItemKind::Torch as usize] = 2;
        world.agents[0].inventory.counts[ItemKind::IronPickaxe as usize] = 1;
        world.step(&[Command {
            agent: miner,
            kind: CommandKind::EnterCave { node: entry },
        }]);
        let c = world.agents[0].cave.unwrap();
        let stand = c.natural_tile().unwrap();
        let (tx, ty) = world.cave_dig_target(miner).expect("a wall to mine");
        let mut nav = world.natural_nav();
        let accessible: Vec<_> = SIDES
            .into_iter()
            .filter_map(|(dx, dy)| nav.next_floor(stand, stand.x + dx, stand.y + dy))
            .filter(|tile| !world.is_mouth_floor(*tile))
            .collect();
        assert!(!accessible.is_empty());
        let tick = (world.tick..world.tick + 5_000)
            .find(|&tick| {
                let roll = rockfall_roll(seed, stand, tick);
                roll % 100 < 5 && roll / 100 % 2 == 1
            })
            .unwrap();
        world.tick = tick;
        world.agents[0].task = Task::Dig {
            tx,
            ty,
            progress: u32::from(CAVE_MINE_TICKS),
        };
        world.step(&[]);
        assert!(
            world
                .events
                .iter()
                .any(|e| matches!(e, Event::CaveRockfall(id) if *id == miner))
        );
        assert!(world.agents[0].injury > 0);
        assert!((1..=2).contains(&world.cave_rubble.len()));
        assert!(!world.cave_rubble.contains(&stand));
        assert!(
            world
                .cave_rubble
                .iter()
                .all(|&tile| !world.is_mouth_floor(tile))
        );

        let saved_hash = world.hash();
        let mut world = World::from_bytes(&world.to_bytes()).unwrap();
        assert_eq!(world.hash(), saved_hash);
        let (x, y) = world.nodes[other.0 as usize].center_mm();
        let rescuer = world.spawn_agent(x, y);
        let ai = rescuer.0 as usize;
        world.agents[ai].inventory.counts[ItemKind::Torch as usize] = 2;
        world.step(&[Command {
            agent: rescuer,
            kind: CommandKind::EnterCave { node: other },
        }]);
        let c = world.agents[ai].cave.unwrap();
        let (approach, pile) = world
            .natural_cave_tiles(c, 28)
            .into_iter()
            .find_map(|from| {
                let mut nav = world.natural_nav();
                let pile = SIDES
                    .into_iter()
                    .find_map(|(dx, dy)| nav.adjacent_rubble(from, from.x + dx, from.y + dy))?;
                assert_eq!(nav.next_floor(from, pile.x, pile.y), None);
                Some((from, pile))
            })
            .expect("the other entrance leads to the same rubble");
        let (x, y) = approach.center_mm();
        world.step(&[Command {
            agent: rescuer,
            kind: CommandKind::MoveTo { x, y },
        }]);
        for _ in 0..1_200 {
            if world.agents[ai].cave.and_then(|c| c.natural_tile()) == Some(approach)
                && world.agents[ai].task == Task::Idle
            {
                break;
            }
            world.step(&[]);
        }
        let c = world.agents[ai].cave.unwrap();
        assert_eq!(c.natural_tile(), Some(approach));
        assert_eq!(
            world.natural_cave_cell(c, pile.x, pile.y),
            Some(Cell::Rubble)
        );
        let stones = world.agents[ai].inventory.get(ItemKind::Stone);
        world.step(&[Command {
            agent: rescuer,
            kind: CommandKind::CaveDig {
                tx: pile.x,
                ty: pile.y,
            },
        }]);
        for _ in 0..RUBBLE_SWING_TICKS * 3 / 2 {
            world.step(&[]);
        }
        assert!(!world.natural_cave_rubble_at(pile));
        assert_eq!(
            world.natural_nav().next_floor(approach, pile.x, pile.y),
            Some(pile)
        );
        assert_eq!(world.agents[ai].inventory.get(ItemKind::Stone), stones + 1);
        assert_eq!(
            World::from_bytes(&world.to_bytes()).unwrap().hash(),
            world.hash()
        );
    }
}
