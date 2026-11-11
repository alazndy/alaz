//! Every actor (player, remote player, bot, animal, future AI rival) acts only through commands.

use crate::agents::RouteStep;
use crate::buildings::BuildingKind;
use crate::map::ItemKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct AgentId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct NodeId(pub u32);

/// Index into `rules().recipes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RecipeId(pub u16);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CommandKind {
    /// A new player joins (issued by the host's turn server only; the `agent` field is ignored).
    Join,
    /// Move toward a fixed-point target (mm).
    MoveTo { x: i64, y: i64 },
    /// Walk to a resource node and harvest it until empty or inventory full.
    Gather { node: NodeId },
    /// Eat one unit of a food item (berries, raw or cooked meat).
    Eat { item: ItemKind },
    /// Chase and spear a prey animal (needs a spear).
    Hunt { wild: u32 },
    /// Walk to a water tile and drink until full.
    Drink { tx: i32, ty: i32 },
    /// Hand-craft: instant recipes finish at once, minigame recipes start the timing game.
    Craft { recipe: RecipeId },
    /// Try what happens when these things (item indexes, 255 = empty hand slot) are worked together.
    /// A combination that makes something teaches it; one that does not does nothing.
    Combine { items: [u8; 4] },
    /// One strike of the running craft minigame; the marker position at the tick it lands decides the hit.
    Strike,
    /// One knapping strike: where on the rim of the stone (degrees, 0..359) and how hard (0..=100).
    StrikeAt { angle: u16, power: u8 },
    /// Place a part for a fitting step: a point in a square, -100..=100 on both axes.
    PlaceAt { x: i16, y: i16 },
    /// Pull a lashing tight with this pressure (0..=100).
    Tighten { power: u8 },
    /// Place a building at a free position (mm); materials are spent immediately.
    Build { kind: BuildingKind, x: i64, y: i64 },
    /// Load inputs into a station; it works on its own and keeps the result.
    StartJob { building: u32, recipe: RecipeId },
    /// Put everything a building accepts (station ingredients, or anything into a depot) into it.
    Deposit { building: u32 },
    /// Take goods out of a station or depot (optionally one item only).
    Collect { building: u32, item: Option<ItemKind> },
    /// Let a station start a recipe by itself whenever its input buffer allows (None = off).
    SetAuto { building: u32, recipe: Option<RecipeId> },
    /// Walk up to a wild animal and feed it berries until it trusts you.
    Tame { wild: u32 },
    /// Give one upkeep item from the issuer's inventory to a nearby worker.
    Feed { target: crate::command::AgentId },
    /// Replace a worker's standing route (empty = stop).
    SetRoute { worker: crate::command::AgentId, route: Vec<RouteStep> },
    /// Step into a cave mouth with a torch (one is used up). Needs to be standing next to it.
    EnterCave { node: NodeId },
    /// In a cave: work the seam in this chamber.
    CaveMine,
    /// In a cave: go one chamber deeper, where the seams are richer and the ceiling worse.
    CaveDeeper,
    /// In a cave: walk back out to the light.
    CaveLeave,
    /// Start or stop running. Running is faster, costs breath and thirst, and only works with breath left.
    SetSprint { on: bool },
    /// Add one wood to a burning building.
    Refuel { building: u32 },
    /// Walk straight toward a point (mm), the way the keys push: over an edge, if there is one in the way.
    Walk { x: i64, y: i64 },
    /// Jump: up, and along if walking.
    Jump,
    /// Break up a felled trunk where it lies (a body id), then pick up the pieces.
    Split { body: u32 },
    /// Pick up a piece lying on the ground (a body id), then the others near it.
    Pick { body: u32 },
    /// Hitch a cart or a plow from the issuer's pack to a nearby ox or horse.
    Hitch { worker: crate::command::AgentId, item: ItemKind },
    /// Sharpen or mend a worn tool, using up a piece of its material.
    Repair { item: ItemKind },
    /// At a running computer: write a new hand recipe into this world's rules (the design table).
    Design { inputs: Vec<(ItemKind, u16)>, output: (ItemKind, u16) },
    /// Strike the rock with a prospector's pick: what deposits lie around, and which way the nearest is.
    Prospect,
    /// Put some of an item down on the ground in front of you (at most 10 at a time); it lies there as pieces
    /// anyone can pick up.
    Drop { item: ItemKind, n: u16 },
    /// Fish the water at (tx, ty), from the bank or wading: a rod or a spear, and a school close by.
    Fish { tx: i32, ty: i32 },
    /// Move a building (with everything in it) to a new spot nearby.
    Relocate { building: u32, x: i64, y: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Command {
    pub agent: AgentId,
    pub kind: CommandKind,
}
