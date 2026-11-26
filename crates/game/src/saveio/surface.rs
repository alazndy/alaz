//! The byte form of a player's surface map.
//!
//! Little endian: the chunks (count, then x, y and 64 rows), the resources and the buildings (count, then x, y and
//! a code), and the camp mark (a flag, then x, y when set).

use crate::surface_map::{CHUNK, Records, Tile, mapped};
use alaz_sim::{BuildingKind, World, map::NodeKind};

pub(super) const FILE_NAME: &str = "surface-map.alz";

/// A resource's code is its place here. Only ever add at the end.
const NODES: [NodeKind; 11] = [
    NodeKind::StoneOutcrop,
    NodeKind::Tree,
    NodeKind::BerryBush,
    NodeKind::ClayDeposit,
    NodeKind::IronOutcrop,
    NodeKind::HerbPatch,
    NodeKind::CopperOutcrop,
    NodeKind::TinOutcrop,
    NodeKind::Deadwood,
    NodeKind::FiberPlant,
    NodeKind::CaveMouth,
];

pub(super) fn encode(records: &Records) -> Vec<u8> {
    if records.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    put_count(&mut out, records.explored.len());
    for (&chunk, rows) in &records.explored {
        put_tile(&mut out, chunk);
        for row in rows {
            out.extend_from_slice(&row.to_le_bytes());
        }
    }
    put_count(&mut out, records.resources.len());
    for (&tile, &kind) in &records.resources {
        put_tile(&mut out, tile);
        out.push(NODES.iter().position(|&n| n == kind).unwrap_or(usize::from(u8::MAX)) as u8);
    }
    put_count(&mut out, records.buildings.len());
    for (&tile, &kind) in &records.buildings {
        put_tile(&mut out, tile);
        out.push(BuildingKind::ALL.iter().position(|&b| b == kind).unwrap_or(usize::from(u8::MAX)) as u8);
    }
    out.push(u8::from(records.camp.is_some()));
    if let Some(camp) = records.camp {
        put_tile(&mut out, camp);
    }
    out
}

/// `None` for anything that is not exactly a map of this world. No bytes at all are an empty map.
pub(super) fn decode(bytes: &[u8], world: &World) -> Option<Records> {
    let mut records = Records::default();
    if bytes.is_empty() {
        return Some(records);
    }
    let mut r = Reader(bytes);
    let on_map = |t: Tile| world.map.in_bounds(t.0, t.1).then_some(t);
    for _ in 0..r.count()? {
        let chunk = r.tile()?;
        on_map((chunk.0.checked_mul(CHUNK)?, chunk.1.checked_mul(CHUNK)?))?;
        let mut rows = [0u64; CHUNK as usize];
        for row in &mut rows {
            *row = u64::from_le_bytes(r.take()?);
        }
        if records.explored.insert(chunk, rows).is_some() {
            return None;
        }
    }
    for _ in 0..r.count()? {
        let tile = on_map(r.tile()?)?;
        let kind = *NODES.get(usize::from(r.byte()?))?;
        if !mapped(kind) || records.resources.insert(tile, kind).is_some() {
            return None;
        }
    }
    for _ in 0..r.count()? {
        let tile = on_map(r.tile()?)?;
        let kind = *BuildingKind::ALL.get(usize::from(r.byte()?))?;
        if records.buildings.insert(tile, kind).is_some() {
            return None;
        }
    }
    records.camp = match r.byte()? {
        0 => None,
        1 => Some(on_map(r.tile()?)?),
        _ => return None,
    };
    r.0.is_empty().then_some(records)
}

fn put_count(out: &mut Vec<u8>, count: usize) {
    // a map is bounded by memory far below this
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
}

fn put_tile(out: &mut Vec<u8>, tile: Tile) {
    out.extend_from_slice(&tile.0.to_le_bytes());
    out.extend_from_slice(&tile.1.to_le_bytes());
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }

    fn count(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    fn tile(&mut self) -> Option<Tile> {
        Some((i32::from_le_bytes(self.take()?), i32::from_le_bytes(self.take()?)))
    }
}
