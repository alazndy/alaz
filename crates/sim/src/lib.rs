//! A.L.A.Z simulation core: deterministic, render-independent.
//! Positions are fixed-point integers (1 unit = 1 mm) to keep lockstep determinism.

pub mod agents;
pub mod buildings;
pub mod debug;
pub mod command;
pub mod map;
mod items_serde;
mod nav;
pub mod physics;
pub mod rng;
pub mod rules;
pub mod save;
pub mod time;
pub mod wildlife;
pub mod world;

#[cfg(test)]
mod tests;

pub use agents::{AgentKind, RouteStep};
pub use buildings::{Building, BuildingKind, Job};
pub use command::{AgentId, Command, CommandKind, NodeId, RecipeId};
pub use map::{climate, elevation_mm, ItemKind, Map, NodeKind, Terrain};
pub use save::{peek as peek_save, SaveError, SaveInfo};
pub use rules::rules;
pub use wildlife::{Wild, WildSpecies};
pub use rules::MinigameKind;
pub use world::{PERCEPTION_STEPS, CraftView, Discovery, Event, Milestone, Reject, World};
