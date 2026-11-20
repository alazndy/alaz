//! Natural cave transitions, light, and gameplay queries.

use super::*;
use crate::caves::{CaveView, Cell};

const CAVE_TORCH_TICKS: u16 = 9_000;
pub const CAVE_MAX_DEPTH: u8 = 3;
pub(super) const CAVE_MINE_TICKS: u16 = 150;
pub const ROCK_SWING_TICKS: u32 = 110;
pub(super) const RUBBLE_SWING_TICKS: u32 = 60;
const PUMP_REACH_MM: i64 = 15_000;

pub(crate) fn command_needs_surface(k: &CommandKind) -> bool {
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
    inv.get(ItemKind::Torch) > 0
        || (inv.get(ItemKind::OilLamp) > 0
            && (inv.get(ItemKind::PlantOil) > 0 || inv.get(ItemKind::Kerosene) > 0))
}

impl World {
    pub(super) fn pumped_at(&self, mx: i64, my: i64) -> bool {
        self.buildings
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                matches!(p.kind, BuildingKind::Pump | BuildingKind::SteamPump)
                    && dist2(p.x, p.y, mx, my) <= PUMP_REACH_MM * PUMP_REACH_MM
            })
            .any(|(pi, p)| {
                if p.kind == BuildingKind::SteamPump {
                    self.requirement_met(pi)
                } else {
                    self.powered_at(p.x, p.y)
                }
            })
    }

    pub(super) fn enter_natural_cave(&mut self, ai: usize, elevation: i16, (x, y): (i64, i64)) {
        let a = &mut self.agents[ai];
        if matches!(a.task, Task::Craft { .. }) {
            self.events.push(Event::CraftAbandoned(a.id));
        }
        let light = if has_light(&a.inventory) {
            CAVE_TORCH_TICKS
        } else {
            0
        };
        a.set_cave_view(CaveView {
            x,
            y,
            z: elevation,
            light,
            leaving: false,
        });
        if !matches!(a.task, Task::Walk { .. }) {
            a.task = Task::Idle;
        }
        a.path.clear();
        a.path_goal = (-1, -1);
        a.walk_route = None;
        a.floor_target = None;
        a.seated = None;
        let id = a.id;
        let destination = a.after_entry.take();
        self.events.push(Event::CaveEntered(id));
        if let Some(position) = destination {
            self.apply_command(&Command {
                agent: id,
                kind: CommandKind::MoveToFloor { position },
            });
        }
    }

    pub(crate) fn cave_dig_command(&mut self, idx: usize, tx: i32, ty: i32) {
        let a = &self.agents[idx];
        let Some(c) = a.cave_view() else { return };
        if a.kind != AgentKind::Human {
            return;
        }
        if !matches!(
            self.natural_cave_cell(c, tx, ty),
            Some(Cell::Rock | Cell::Rubble)
        ) {
            return;
        }
        let id = a.id;
        if matches!(a.task, Task::Craft { .. }) {
            self.events.push(Event::CraftAbandoned(id));
        }
        let a = &mut self.agents[idx];
        a.task = Task::Dig {
            tx,
            ty,
            progress: 0,
        };
        a.after_surface = None;
        a.after_entry = None;
        if let Some(c) = &mut a.passage {
            c.leaving = false;
        }
    }

    pub(crate) fn run_caves(&mut self) {
        for ai in 0..self.agents.len() {
            let a = &mut self.agents[ai];
            let Some(mut c) = a.cave_view() else { continue };
            let light = if has_light(&a.inventory) {
                CAVE_TORCH_TICKS
            } else {
                0
            };
            if light != c.light {
                self.events.push(if light > 0 {
                    Event::CaveRelit(a.id)
                } else {
                    Event::CaveDark(a.id)
                });
            }
            c.light = light;
            a.set_cave_view(c);
        }
    }

    pub(super) fn leave_at(&mut self, ai: usize, c: &CaveView, resume: Task) {
        let id = self.agents[ai].id;
        let a = &mut self.agents[ai];
        a.passage = None;
        a.task = resume;
        a.path.clear();
        a.path_goal = (-1, -1);
        a.x = c.x;
        a.y = c.y;
        a.z = i64::from(crate::map::elevation_at_mm(self.seed, a.x, a.y));
        a.walk_route = None;
        a.floor_target = None;
        let pending = a.after_surface.take();
        self.events.push(Event::CaveLeft {
            agent: id,
            dark: c.light == 0,
        });
        if let Some(kind) = pending {
            self.apply_command(&Command { agent: id, kind });
        }
    }

    pub fn cave_dig_target(&self, id: AgentId) -> Option<(i32, i32)> {
        let c = self.agents.get(id.0 as usize)?.cave_view()?;
        let (x, y) = c.tile();
        crate::cave_terrain::SIDES
            .into_iter()
            .map(|(dx, dy)| (x + dx, y + dy))
            .filter_map(|(x, y)| match self.natural_cave_cell(c, x, y) {
                Some(Cell::Rubble) => Some((0, (x, y))),
                Some(Cell::Rock) => Some((
                    1 + u8::from(self.natural_cave_ore(c, x, y).is_none()),
                    (x, y),
                )),
                _ => None,
            })
            .min_by_key(|&(rank, _)| rank)
            .map(|(_, tile)| tile)
    }

    pub fn cave_down_spot(&self, id: AgentId) -> Option<(i64, i64)> {
        self.natural_deeper_spot(self.agents.get(id.0 as usize)?.cave_view()?)
    }

    pub fn cave_way_out(&self, id: AgentId) -> Option<((i64, i64), i32)> {
        let a = self.agents.get(id.0 as usize)?;
        self.natural_way_out(a.cave_view()?, a)
    }
}
