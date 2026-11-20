//! Format 33 stores the feet position in world coordinates for every walker.

use super::{
    CommonSaveExt, NaturalCaveEdits, SaveCore, SaveData as OldSaveData, SaveError, SaveExt,
};
use crate::agents::{AgentKind, RouteStep};
use crate::cave_terrain::WorldPoint;
use crate::command::{AgentId, CommandKind};
use crate::nav::walking::{WalkFloor, WalkRoute};
use crate::physics::{Air, Body};
use crate::world::{Agent, CavePassage, Inventory, Task, World};
use serde::{Deserialize, Serialize};

pub(super) const VERSION: u32 = 33;

#[derive(Serialize, Deserialize)]
struct SaveData {
    core: SaveCore<SavedAgent>,
    air: Vec<Air>,
    bodies: Vec<Body>,
    ext: CommonSaveExt,
    cave_edits: NaturalCaveEdits,
}

#[derive(Serialize, Deserialize)]
struct SavedAgent {
    id: AgentId,
    kind: AgentKind,
    position: WorldPoint,
    task: Task,
    inventory: Inventory,
    health: i32,
    hunger: i32,
    thirst: i32,
    warmth: i32,
    route: Vec<RouteStep>,
    route_i: u32,
    route_wait: u32,
    route_try: u8,
    injury: i32,
    broken: i32,
    sprint: bool,
    stamina: i32,
    sick: i32,
    walk_route: Option<WalkRoute>,
    passage: Option<Passage>,
    floor_target: Option<i64>,
    after_entry: Option<WorldPoint>,
    after_surface: Option<CommandKind>,
}

#[derive(Serialize, Deserialize)]
struct Passage {
    light: u16,
    leaving: bool,
}

impl From<SaveExt> for CommonSaveExt {
    fn from(value: SaveExt) -> Self {
        let SaveExt {
            settings,
            recipe_ids,
            custom_rules,
            buildings,
            generations,
            wear,
            nutrition,
            chunks,
            facing,
            burn,
            seen_hi,
            journal,
            ..
        } = value;
        Self {
            settings,
            recipe_ids,
            custom_rules,
            buildings,
            generations,
            wear,
            nutrition,
            chunks,
            facing,
            burn,
            seen_hi,
            journal,
        }
    }
}

pub(super) fn encode(data: OldSaveData, world: &World) -> postcard::Result<Vec<u8>> {
    let OldSaveData {
        core,
        air,
        bodies,
        ext,
    } = data;
    let core = core.map_agents(|a| SavedAgent::from_agent(a, world));
    let cave_edits = NaturalCaveEdits {
        dug: world.dug_cave.iter().copied().collect(),
        rubble: world.cave_rubble.iter().copied().collect(),
    };
    postcard::to_allocvec(&SaveData {
        core,
        air,
        bodies,
        ext: ext.into(),
        cave_edits,
    })
}

pub(super) fn decode(payload: &[u8]) -> Result<(OldSaveData, NaturalCaveEdits), SaveError> {
    let SaveData {
        core,
        air,
        bodies,
        ext,
        cave_edits,
    } = postcard::from_bytes::<SaveData>(payload).map_err(|_| SaveError::Decode)?;
    for a in &core.agents {
        let z = a.position.z_mm;
        if a.passage.is_some() && (!(-95_000..=70_000).contains(&z) || z.rem_euclid(1_000) != 0) {
            return Err(SaveError::WorldMismatch);
        }
    }
    let core = core.map_agents(SavedAgent::into_agent);
    Ok((
        OldSaveData {
            core,
            air,
            bodies,
            ext: ext.upgrade(),
        },
        cave_edits,
    ))
}

impl SavedAgent {
    fn from_agent(a: Agent, world: &World) -> Self {
        let position = a.position();
        let floor = |(x, y)| {
            let z_mm = i64::from(crate::map::elevation_mm(world.seed, x, y));
            WalkFloor { x, y, z_mm }
        };
        let walk_route = a.walk_route.or_else(|| {
            (a.path_goal != (-1, -1)).then(|| WalkRoute {
                goal: floor(a.path_goal).center(),
                remaining: a.path.into_iter().map(floor).collect(),
            })
        });
        Self {
            id: a.id,
            kind: a.kind,
            position,
            task: a.task,
            inventory: a.inventory,
            health: a.health,
            hunger: a.hunger,
            thirst: a.thirst,
            warmth: a.warmth,
            route: a.route,
            route_i: a.route_i,
            route_wait: a.route_wait,
            route_try: a.route_try,
            injury: a.injury,
            broken: a.broken,
            sprint: a.sprint,
            stamina: a.stamina,
            sick: a.sick,
            walk_route,
            passage: a.passage.map(|c| Passage {
                light: c.light,
                leaving: c.leaving,
            }),
            floor_target: a.floor_target,
            after_entry: a.after_entry,
            after_surface: a.after_surface,
        }
    }

    fn into_agent(self) -> Agent {
        let a = self;
        let (path, path_goal) = a.walk_route.as_ref().map_or_else(
            || (Vec::new(), (-1, -1)),
            |r| {
                (
                    r.remaining.iter().map(|f| (f.x, f.y)).collect(),
                    r.goal.tile(),
                )
            },
        );
        Agent {
            id: a.id,
            kind: a.kind,
            x: a.position.x_mm,
            y: a.position.y_mm,
            z: a.position.z_mm,
            task: a.task,
            inventory: a.inventory,
            health: a.health,
            hunger: a.hunger,
            thirst: a.thirst,
            warmth: a.warmth,
            route: a.route,
            route_i: a.route_i,
            route_wait: a.route_wait,
            route_try: a.route_try,
            injury: a.injury,
            broken: a.broken,
            sprint: a.sprint,
            fly: false,
            stamina: a.stamina,
            sick: a.sick,
            path,
            path_goal,
            walk_route: a.walk_route,
            floor_target: a.floor_target,
            after_entry: a.after_entry,
            after_surface: a.after_surface,
            generation: 0,
            wear: Vec::new(),
            nutrition: [600; 3],
            seated: None,
            passage: a.passage.map(|c| CavePassage {
                light: c.light,
                leaving: c.leaving,
            }),
        }
    }
}
