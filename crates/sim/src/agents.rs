//! Agent kinds (people, tamed animals, bots) and the standing-order route language.

use crate::map::{ItemKind, NodeKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Human,
    Dog,
    Ox,
    Horse,
    Bot,
}

impl AgentKind {
    /// Movement per tick in mm.
    pub fn speed_mm(self) -> i64 {
        match self {
            AgentKind::Human => 25,
            AgentKind::Dog => 40,
            AgentKind::Ox => 18,
            AgentKind::Horse => 45,
            AgentKind::Bot => 30,
        }
    }

    /// Total items one agent can carry.
    pub fn capacity(self) -> u16 {
        match self {
            AgentKind::Human => 40,
            AgentKind::Dog => 8,
            AgentKind::Ox => 60,
            AgentKind::Horse => 25,
            AgentKind::Bot => 40,
        }
    }

    pub fn is_worker(self) -> bool {
        self != AgentKind::Human
    }

    /// Only people and bots can harvest; animals only haul.
    pub fn can_harvest(self) -> bool {
        matches!(self, AgentKind::Human | AgentKind::Bot)
    }

    /// What keeps this worker going: berries for animals, charcoal for bots.
    pub fn upkeep_item(self) -> Option<ItemKind> {
        match self {
            AgentKind::Human => None,
            AgentKind::Bot => Some(ItemKind::Charcoal),
            _ => Some(ItemKind::Berries),
        }
    }

    /// Upkeep restored by one unit of the upkeep item (out of 10000).
    pub fn upkeep_gain(self) -> i32 {
        match self {
            AgentKind::Bot => 4_000,
            _ => 1_800,
        }
    }

    /// One upkeep point is lost every this many ticks.
    pub fn upkeep_decay_ticks(self) -> u64 {
        match self {
            AgentKind::Bot => 10,
            _ => 13,
        }
    }
}

/// One step of a worker's repeating route. Steps that cannot be done are skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteStep {
    /// Harvest the nearest node of `kind` around a point until the carrier is full or none is left.
    Harvest { kind: NodeKind, x: i64, y: i64, radius_mm: i64 },
    /// Walk to a building and put in everything it accepts.
    Deposit { building: u32 },
    /// Walk to a building and take its output (optionally one item only).
    Collect { building: u32, item: Option<ItemKind> },
    /// Stand still.
    Wait { ticks: u32 },
}
