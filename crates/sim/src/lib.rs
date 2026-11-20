//! A.L.A.Z simulation core: deterministic, render-independent.
//! Positions are fixed-point integers (1 unit = 1 mm) to keep lockstep determinism.

pub mod agents;
pub mod buildings;
pub mod camp;
pub mod cave_terrain;
pub mod caves;
pub mod debug;
pub mod item_guide;
pub mod journal;
pub mod command;
pub mod map;
mod items_serde;
mod nav;
pub mod physics;
pub mod rng;
pub mod rules;
pub mod save;
pub mod surface;
pub mod time;
pub mod wildlife;
pub mod world;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod surface_tests;

pub use agents::{AgentKind, RouteStep};
pub use caves::{Cave, CaveSelf};
pub use buildings::{Building, BuildingKind, Job};
pub use command::{AgentId, Command, CommandKind, NodeId, RecipeId};
pub use map::{climate, elevation_mm, ItemKind, Map, NodeKind, Terrain};
pub use save::{peek as peek_save, SaveError, SaveInfo};
pub use rules::rules;
pub use surface::{Burn, Ground, Surface};
pub use wildlife::{Wild, WildSpecies};
pub use rules::MinigameKind;
pub use journal::{Goal, Journal};
pub use world::{load_room, PERCEPTION_STEPS, CraftView, Discovery, Event, Milestone, Reject, World};
