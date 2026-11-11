//! Data-driven rules parsed once from `data/rules.toml`.

use crate::agents::AgentKind;
use crate::buildings::BuildingKind;
use crate::map::{ItemKind, NodeKind};
use crate::world::Discovery;
use serde::Deserialize;
use std::sync::OnceLock;

const RULES_TOML: &str = include_str!("../data/rules.toml");

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ItemCount {
    pub item: ItemKind,
    pub n: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BuildingDef {
    pub kind: BuildingKind,
    pub name: String,
    pub radius_mm: i64,
    pub discovery: Discovery,
    pub cost: Vec<ItemCount>,
    /// Things that must be carried to build it but are not used up (flint to light a fire).
    #[serde(default)]
    pub needs: Vec<ItemKind>,
    /// Heat sources: the hottest it gets while it burns or works (°C), and how fast it climbs there (°C a second).
    #[serde(default)]
    pub heat_max: i16,
    #[serde(default)]
    pub heat_rate: i16,
}

/// A hand-craft played as a timing game: a marker sweeps back and forth and each strike must land
/// in the target zone. Owned tools (not consumed) widen the zone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinigameKind {
    /// A marker sweeps a bar; strike when it is in the zone.
    #[default]
    Timing,
    /// Knapping: aim at a spot on the rim of the stone and strike with the right force.
    Knap,
    /// Fitting one part into another: put it on the right spot (a point in a square).
    Fit,
    /// Lashing or pressing: hold and release at the right pressure.
    Tension,
    /// Stacking pieces one on another: drop each at a sideways position; the pile must balance
    /// (the centre of mass of everything above must rest on the piece below).
    Stack,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Minigame {
    /// What this step is, shown to the player ("Sapı düzelt").
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub kind: MinigameKind,
    pub strikes: u8,
    /// Half-width of the target zone: ticks of marker travel (timing) or degrees around the rim (knap).
    pub window: u32,
    /// Ticks for one full sweep there and back.
    pub period: u32,
    #[serde(default)]
    pub tools: Vec<ToolBonus>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ToolBonus {
    pub item: ItemKind,
    pub bonus: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub station: Option<BuildingKind>,
    pub inputs: Vec<ItemCount>,
    /// Things that must be carried to make it but are not used up.
    #[serde(default)]
    pub needs: Vec<ItemKind>,
    #[serde(default)]
    pub outputs: Vec<ItemCount>,
    /// Finished job releases a new worker of this kind next to the station.
    #[serde(default)]
    pub spawns: Option<AgentKind>,
    /// 0 = instant (hand crafting).
    pub ticks: u32,
    pub discovery: Discovery,
    /// Hand recipes with steps are played as minigames, one step after the other, instead of being instant.
    #[serde(default)]
    pub steps: Vec<Minigame>,
    /// Extra units of the first output for a flawless (or lucky) result.
    #[serde(default)]
    pub fine_bonus: u16,
    /// Station recipes: the work only goes on while the station is at least this hot (°C).
    #[serde(default)]
    pub heat: i16,
    /// A mould among the inputs that comes back after a good cast, unless it cracks (by chance, by difficulty).
    #[serde(default)]
    pub mold: Option<ItemKind>,
    /// What a failed attempt gives back instead (spilled metal cools to ore, the mould and crucible survive).
    #[serde(default)]
    pub fail_returns: Vec<ItemCount>,
    /// Station alloying: all of two ores loaded into the station melt together when the job ends.
    #[serde(default)]
    pub alloy: Option<AlloyDef>,
    /// Hand work that must be done standing at a building (forging at the anvil).
    #[serde(default)]
    pub near: Option<BuildingKind>,
    /// A mine shaft's work: what comes up is the deposit under it (more from a rich one).
    #[serde(default)]
    pub mine: bool,
}

/// Two ores melted together: if the base ore's share (percent) is in range the alloy comes out, else a bad one.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AlloyDef {
    pub base: ItemKind,
    pub add: ItemKind,
    pub ores_per_ingot: u32,
    pub output: ItemKind,
    pub bad: ItemKind,
    pub lo: u16,
    pub hi: u16,
    pub lo_realistic: u16,
    pub hi_realistic: u16,
}

/// What a tool is for, when a resource asks for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Fells trees.
    Axe,
    /// Breaks rock seams: a stone hammer at first, picks later.
    Pick,
    Knife,
    Shovel,
}

/// A carried item that counts as a tool of some kind and tier (stone 0, copper 1, bronze 2, iron 3, ...).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ToolDef {
    pub item: ItemKind,
    pub kind: ToolKind,
    pub tier: u8,
    /// Uses before it goes blunt (doubled when forgiving); 0 = never wears.
    #[serde(default)]
    pub durability: u16,
    /// What a repair uses up (a stone for stone tools, ore or metal for metal ones).
    #[serde(default)]
    pub repair: Option<ItemKind>,
}

/// A resource that cannot be worked without a tool of at least this tier. The first `loose` units of a node
/// lie loose on top and come away by hand.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct GateDef {
    pub node: NodeKind,
    pub tool: ToolKind,
    pub tier: u8,
    #[serde(default)]
    pub loose: u16,
}

#[derive(Debug, Deserialize)]
struct RulesFile {
    /// Pick tier needed to step down into each cave chamber (index = chamber depth).
    #[serde(default)]
    cave_tiers: Vec<u8>,
    building: Vec<BuildingDef>,
    recipe: Vec<RecipeDef>,
    #[serde(default)]
    tool: Vec<ToolDef>,
    #[serde(default)]
    gate: Vec<GateDef>,
    #[serde(default)]
    perish: Vec<PerishDef>,
}

/// Food that spoils: the share of a stack that goes bad in a day, permille (before cold, jars and difficulty).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct PerishDef {
    pub item: ItemKind,
    pub per_day: u32,
}

#[derive(Debug)]
pub struct Rules {
    pub buildings: Vec<BuildingDef>,
    pub recipes: Vec<RecipeDef>,
    pub tools: Vec<ToolDef>,
    pub gates: Vec<GateDef>,
    pub cave_tiers: Vec<u8>,
    pub perish: Vec<PerishDef>,
    /// FNV-1a of the source text; stored in saves.
    pub checksum: u64,
}

impl Rules {
    pub fn building(&self, kind: BuildingKind) -> &BuildingDef {
        self.buildings.iter().find(|b| b.kind == kind).expect("building def")
    }

    /// The best tier of a kind of tool among the things carried, if any.
    pub fn tool_tier(&self, kind: ToolKind, has: impl Fn(ItemKind) -> bool) -> Option<u8> {
        self.tools.iter().filter(|t| t.kind == kind && has(t.item)).map(|t| t.tier).max()
    }

    /// The best carried tool of a kind (the one that does the work, and wears).
    pub fn best_tool(&self, kind: ToolKind, has: impl Fn(ItemKind) -> bool) -> Option<&ToolDef> {
        self.tools.iter().filter(|t| t.kind == kind && has(t.item)).max_by_key(|t| t.tier)
    }

    pub fn tool(&self, item: ItemKind) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.item == item)
    }

    pub fn gate(&self, node: NodeKind) -> Option<&GateDef> {
        self.gates.iter().find(|g| g.node == node)
    }

    /// Pick tier needed to go down into chamber `depth`.
    pub fn cave_tier(&self, depth: u8) -> u8 {
        self.cave_tiers.get(depth as usize).copied().unwrap_or(0)
    }
}

impl Rules {
    /// Parse rules from TOML text in the shape of `data/rules.toml`.
    pub fn parse(text: &str) -> Result<Rules, String> {
        let file: RulesFile = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut h: u64 = 0xcbf29ce484222325;
        for b in text.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        Ok(Rules { buildings: file.building, recipes: file.recipe, tools: file.tool, gates: file.gate, cave_tiers: file.cave_tiers, perish: file.perish, checksum: h })
    }

    /// The source text of the base rules (a world's own rules start from a copy of it).
    pub fn base_text() -> &'static str {
        RULES_TOML
    }
}

/// The base rules every new world starts with (`data/rules.toml`). A world may carry its own (`World::rules`).
pub fn rules() -> &'static Rules {
    base_arc()
}

/// The base rules as a shared handle (worlds hold a clone when they play by them).
pub fn rules_arc() -> std::sync::Arc<Rules> {
    std::sync::Arc::clone(base_arc())
}

fn base_arc() -> &'static std::sync::Arc<Rules> {
    static RULES: OnceLock<std::sync::Arc<Rules>> = OnceLock::new();
    RULES.get_or_init(|| std::sync::Arc::new(Rules::parse(RULES_TOML).expect("data/rules.toml is invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_and_are_consistent() {
        let r = rules();
        for k in BuildingKind::ALL {
            assert_eq!(r.buildings.iter().filter(|b| b.kind == k).count(), 1, "{k:?}");
        }
        let mut ids: Vec<_> = r.recipes.iter().map(|x| x.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), r.recipes.len());
        for rec in &r.recipes {
            assert!(!rec.inputs.is_empty() && (!rec.outputs.is_empty() || rec.spawns.is_some() || rec.alloy.is_some() || rec.mine), "{}", rec.id);
            if rec.station.is_some() {
                assert!(rec.ticks > 0, "{} needs a duration", rec.id);
            }
        }
    }
}

impl RecipeDef {
    /// Total number of strikes over all steps.
    pub fn total_strikes(&self) -> u32 {
        self.steps.iter().map(|s| s.strikes as u32).sum()
    }
}
