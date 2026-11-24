//! Cave cells, natural ore distributions and the previous save format's walker record.

use crate::map::ItemKind;
use serde::{Deserialize, Serialize};

const TILE: i64 = crate::map::TILE_MM;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Cell {
    /// Solid rock (diggable except at the very edge).
    Rock = 0,
    Floor = 1,
    /// Wading depth: slow going.
    Shallow = 2,
    /// Swimming depth.
    Deep = 3,
    /// Steps down to the next level.
    Down = 4,
    /// Steps up to the level above.
    Up = 5,
    /// The way out to the surface (level 1 only).
    Exit = 6,
    /// Fallen stone, loose enough to clear by hand.
    Rubble = 7,
}

impl Cell {
    pub fn from_u8(v: u8) -> Cell {
        match v {
            1 => Cell::Floor,
            2 => Cell::Shallow,
            3 => Cell::Deep,
            4 => Cell::Down,
            5 => Cell::Up,
            6 => Cell::Exit,
            7 => Cell::Rubble,
            _ => Cell::Rock,
        }
    }

    pub fn walkable(self) -> bool {
        !matches!(self, Cell::Rock | Cell::Rubble)
    }
}

/// Codes for the ore found in a natural vein.
pub const ORE_IRON: u8 = 1;
pub const ORE_COPPER: u8 = 2;
pub const ORE_TIN: u8 = 3;
pub const ORE_COAL: u8 = 4;

pub fn ore_item(code: u8) -> Option<ItemKind> {
    match code {
        ORE_IRON => Some(ItemKind::IronOre),
        ORE_COPPER => Some(ItemKind::CopperOre),
        ORE_TIN => Some(ItemKind::TinOre),
        ORE_COAL => Some(ItemKind::Coal),
        _ => None,
    }
}

/// The ore a vein on depth `depth` (0 = level 1) turns out to be: the same mix the old chambers gave, without the
/// share that was plain stone.
pub(crate) fn vein_distribution(depth: u8) -> &'static [(u8, u64)] {
    match depth {
        0 => &[(ORE_IRON, 35)],
        1 => &[(ORE_IRON, 30), (ORE_COPPER, 30)],
        2 => &[
            (ORE_COPPER, 25),
            (ORE_TIN, 25),
            (ORE_IRON, 15),
            (ORE_COAL, 20),
        ],
        _ => &[
            (ORE_TIN, 30),
            (ORE_COPPER, 20),
            (ORE_IRON, 15),
            (ORE_COAL, 35),
        ],
    }
}

pub(crate) fn vein_ore(depth: u8, roll: u64) -> u8 {
    let table = vein_distribution(depth);
    let total: u64 = table.iter().map(|t| t.1).sum();
    let mut r = roll % total;
    for &(code, w) in table {
        if r < w {
            return code;
        }
        r -= w;
    }
    table[0].0
}

/// Current cave floor and light, projected from the walker's world position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaveView {
    pub x: i64,
    pub y: i64,
    pub z: i16,
    pub light: u16,
    pub(crate) leaving: bool,
}

impl CaveView {
    pub fn elevation_m(self) -> i16 {
        self.z
    }

    pub(crate) fn natural_tile(self) -> crate::cave_terrain::CaveTile {
        let (x, y) = self.tile();
        crate::cave_terrain::CaveTile { x, y, z: self.z }
    }

    pub fn tile(self) -> (i32, i32) {
        (
            self.x.div_euclid(TILE) as i32,
            self.y.div_euclid(TILE) as i32,
        )
    }

    pub fn depth(self) -> u8 {
        Self::natural_depth_at(self.z)
    }

    pub(crate) fn natural_depth_at(z: i16) -> u8 {
        match z {
            ..=-60 => 3,
            -59..=-35 => 2,
            -34..=-16 => 1,
            _ => 0,
        }
    }
}
