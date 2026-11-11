//! Save format: `ALAZ` magic, format version, payload checksum, then a postcard payload.
//! The map and resource nodes are regenerated from the seed; only changes are stored.
//! The sim hash is stored too, so a load that does not reproduce the saved state is detected.

use crate::map::ItemKind;
use crate::buildings::Building;
use crate::rules::rules;
use crate::wildlife::Wild;
use crate::world::{Agent, World, WorldSettings};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 4] = b"ALAZ";
pub const FORMAT_VERSION: u32 = 24;
/// The format before flint: 32 item slots and the rules without flint. Still read, and upgraded on load.
const LEGACY_VERSION: u32 = 17;
/// The format before gravity: nobody in the air and nothing falling. Still read, and upgraded on load.
const PRE_PHYSICS_VERSION: u32 = 18;
/// The format before trunks were broken up where they lie: bodies had no id and held no wood.
const PRE_LOG_VERSION: u32 = 19;
/// The format before the extension block: no world settings (seasons were 3 days), no rule signature, no
/// building temperature or soil, no animal generations. Still read, and upgraded on load.
const PRE_EXT_VERSION: u32 = 20;
/// The format before tool wear: the extension block without it. Still read, and upgraded on load (tools unworn).
const PRE_WEAR_VERSION: u32 = 21;
/// The format before food groups. Still read, and upgraded on load (everyone half fed).
const PRE_NUTRITION_VERSION: u32 = 22;
/// The format before worlds made in chunks. Still read (such saves are all of fixed, fully made worlds).
const PRE_CHUNK_VERSION: u32 = 23;

/// Decode a payload, reading item arrays the old way for a format-17 save; older saves have no falling bodies.
fn decode(version: u32, payload: &[u8]) -> Result<SaveData, SaveError> {
    let legacy = version == LEGACY_VERSION;
    crate::items_serde::LEGACY.with(|l| l.set(legacy));
    let r = if version == FORMAT_VERSION {
        postcard::from_bytes(payload)
    } else if version == PRE_CHUNK_VERSION {
        postcard::from_bytes::<SaveDataV23>(payload).map(|d| SaveData { core: d.core, air: d.air, bodies: d.bodies, ext: d.ext.upgrade() })
    } else if version == PRE_NUTRITION_VERSION {
        postcard::from_bytes::<SaveDataV22>(payload).map(|d| SaveData { core: d.core, air: d.air, bodies: d.bodies, ext: d.ext.upgrade() })
    } else if version == PRE_WEAR_VERSION {
        postcard::from_bytes::<SaveDataV21>(payload).map(|d| SaveData { core: d.core, air: d.air, bodies: d.bodies, ext: d.ext.upgrade() })
    } else if version == PRE_EXT_VERSION {
        postcard::from_bytes::<SaveDataV20>(payload).map(|d| SaveData { core: d.core, air: d.air, bodies: d.bodies, ext: SaveExt::upgraded() })
    } else if version == PRE_LOG_VERSION {
        postcard::from_bytes::<SaveDataV19>(payload).map(|d| {
            let seed = d.core.seed;
            let bodies = d.bodies.into_iter().enumerate().map(|(i, b)| b.upgrade(i as u32, seed)).collect();
            SaveData { core: d.core, air: d.air, bodies, ext: SaveExt::upgraded() }
        })
    } else {
        postcard::from_bytes::<SaveCore>(payload).map(|core| SaveData { core, air: Vec::new(), bodies: Vec::new(), ext: SaveExt::upgraded() })
    }
    .map_err(|_| SaveError::Decode);
    crate::items_serde::LEGACY.with(|l| l.set(false));
    r
}

fn readable(version: u32) -> bool {
    matches!(version, FORMAT_VERSION | PRE_CHUNK_VERSION | PRE_NUTRITION_VERSION | PRE_WEAR_VERSION | PRE_EXT_VERSION | PRE_LOG_VERSION | PRE_PHYSICS_VERSION | LEGACY_VERSION)
}
const HEADER_LEN: usize = 4 + 4 + 8;

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u32),
    ChecksumMismatch,
    Decode,
    /// Saved data does not fit the world generated from its seed (different generator version?).
    WorldMismatch,
    /// Rebuilt world hashes differently from the saved one.
    HashMismatch,
    /// Save was made with different game rules (data/rules.toml).
    RulesMismatch,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SaveError::TooShort => "kayıt dosyası çok kısa".to_string(),
            SaveError::BadMagic => "bu bir A.L.A.Z kaydı değil".to_string(),
            SaveError::UnsupportedVersion(v) => format!("desteklenmeyen kayıt sürümü: {v}"),
            SaveError::ChecksumMismatch => "kayıt dosyası bozuk (sağlama toplamı)".to_string(),
            SaveError::Decode => "kayıt içeriği okunamadı".to_string(),
            SaveError::WorldMismatch => "kayıt, üretilen dünyayla uyuşmuyor".to_string(),
            SaveError::HashMismatch => "kayıt yüklendi ama durum doğrulanamadı".to_string(),
            SaveError::RulesMismatch => "kayıt başka oyun kurallarıyla yapılmış".to_string(),
        };
        f.write_str(&s)
    }
}

impl std::error::Error for SaveError {}

/// Everything the formats up to 18 stored, in their order; format 19 adds what is falling after it.
#[derive(Serialize, Deserialize)]
struct SaveCore {
    seed: u64,
    map_w: i32,
    map_h: i32,
    tick: u64,
    known: u64,
    seen: u128,
    rules_checksum: u64,
    #[serde(with = "crate::items_serde")]
    produced: [u32; ItemKind::COUNT],
    milestones: u64,
    hardcore: bool,
    node_changes: Vec<(u32, u16)>,
    buildings: Vec<Building>,
    agents: Vec<Agent>,
    wild: Vec<Wild>,
    state_hash: u64,
}

#[derive(Deserialize)]
struct SaveDataV19 {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::BodyV19>,
}

#[derive(Deserialize)]
struct SaveDataV20 {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::Body>,
}

/// Format 21 on: what the core does not hold. Kept apart so the core stays readable for every older format.
#[derive(Serialize, Deserialize)]
struct SaveExt {
    settings: WorldSettings,
    /// Ids of the recipes in the order the save refers to them by index. A save loads under rules whose recipe
    /// list starts with these (rules may grow at the end, and recipes may be retuned, but not reordered).
    recipe_ids: Vec<String>,
    /// The world's own rules, if it has them (the final's design table writes them).
    custom_rules: Option<String>,
    /// Per building, in order: temperature and soil.
    buildings: Vec<(i16, u16)>,
    /// Per agent, in order: generations from the wild.
    generations: Vec<u8>,
    /// Per agent, in order: wear on its tools.
    wear: Vec<Vec<(ItemKind, u16)>>,
    /// Per agent, in order: food groups (grain, protein, fruit and greens).
    nutrition: Vec<[u16; 3]>,
    /// A world made in chunks: the chunks made, in order (replayed on load, so node ids match).
    chunks: Vec<(i32, i32)>,
}

/// The extension block as format 23 wrote it (no chunk list).
#[derive(Deserialize)]
struct SaveExtV23 {
    settings: WorldSettings,
    recipe_ids: Vec<String>,
    custom_rules: Option<String>,
    buildings: Vec<(i16, u16)>,
    generations: Vec<u8>,
    wear: Vec<Vec<(ItemKind, u16)>>,
    nutrition: Vec<[u16; 3]>,
}

impl SaveExtV23 {
    fn upgrade(self) -> SaveExt {
        SaveExt { settings: self.settings, recipe_ids: self.recipe_ids, custom_rules: self.custom_rules, buildings: self.buildings, generations: self.generations, wear: self.wear, nutrition: self.nutrition, chunks: Vec::new() }
    }
}

#[derive(Deserialize)]
struct SaveDataV23 {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::Body>,
    ext: SaveExtV23,
}

/// The extension block as format 22 wrote it (no food groups).
#[derive(Deserialize)]
struct SaveExtV22 {
    settings: WorldSettings,
    recipe_ids: Vec<String>,
    custom_rules: Option<String>,
    buildings: Vec<(i16, u16)>,
    generations: Vec<u8>,
    wear: Vec<Vec<(ItemKind, u16)>>,
}

impl SaveExtV22 {
    fn upgrade(self) -> SaveExt {
        SaveExt { settings: self.settings, recipe_ids: self.recipe_ids, custom_rules: self.custom_rules, buildings: self.buildings, generations: self.generations, wear: self.wear, nutrition: Vec::new(), chunks: Vec::new() }
    }
}

#[derive(Deserialize)]
struct SaveDataV22 {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::Body>,
    ext: SaveExtV22,
}

/// The extension block as format 21 wrote it (no tool wear).
#[derive(Deserialize)]
struct SaveExtV21 {
    settings: WorldSettings,
    recipe_ids: Vec<String>,
    custom_rules: Option<String>,
    buildings: Vec<(i16, u16)>,
    generations: Vec<u8>,
}

impl SaveExtV21 {
    fn upgrade(self) -> SaveExt {
        SaveExt { settings: self.settings, recipe_ids: self.recipe_ids, custom_rules: self.custom_rules, buildings: self.buildings, generations: self.generations, wear: Vec::new(), nutrition: Vec::new(), chunks: Vec::new() }
    }
}

#[derive(Deserialize)]
struct SaveDataV21 {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::Body>,
    ext: SaveExtV21,
}

impl SaveExt {
    /// What an older save is upgraded with: the forgiving setting and the 3-day seasons it was played with.
    fn upgraded() -> Self {
        Self {
            settings: WorldSettings { difficulty: Default::default(), season_days: crate::time::SEASON_DAYS as u8 },
            recipe_ids: Vec::new(),
            custom_rules: None,
            buildings: Vec::new(),
            generations: Vec::new(),
            wear: Vec::new(),
            nutrition: Vec::new(),
            chunks: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SaveData {
    core: SaveCore,
    air: Vec<crate::physics::Air>,
    bodies: Vec<crate::physics::Body>,
    ext: SaveExt,
}

fn checksum(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// What a save file says about itself, read without rebuilding the world (for slot lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveInfo {
    pub seed: u64,
    pub tick: u64,
    pub map_w: i32,
    pub map_h: i32,
    pub hardcore: bool,
    pub players: usize,
}

/// Validate a save's header and checksum and read its summary.
pub fn peek(bytes: &[u8]) -> Result<SaveInfo, SaveError> {
    if bytes.len() < HEADER_LEN {
        return Err(SaveError::TooShort);
    }
    if &bytes[..4] != MAGIC {
        return Err(SaveError::BadMagic);
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if !readable(version) {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let sum = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let payload = &bytes[HEADER_LEN..];
    if checksum(payload) != sum {
        return Err(SaveError::ChecksumMismatch);
    }
    let data = decode(version, payload)?.core;
    Ok(SaveInfo {
        seed: data.seed,
        tick: data.tick,
        map_w: data.map_w,
        map_h: data.map_h,
        hardcore: data.hardcore,
        players: data.agents.iter().filter(|a| a.kind == crate::agents::AgentKind::Human).count(),
    })
}

impl World {
    pub fn to_bytes(&self) -> Vec<u8> {
        let core = SaveCore {
            seed: self.seed,
            map_w: self.map.w,
            map_h: self.map.h,
            tick: self.tick,
            known: self.known,
            seen: self.seen,
            rules_checksum: rules().checksum,
            produced: self.produced,
            milestones: self.milestones,
            hardcore: self.hardcore,
            // only nodes that are not untouched: the rest regenerates from the seed
            node_changes: self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.amount != n.kind.initial_amount())
                .map(|(i, n)| (i as u32, n.amount))
                .collect(),
            buildings: self.buildings.clone(),
            agents: self.agents.clone(),
            wild: self.wild.clone(),
            state_hash: self.hash(),
        };
        let ext = SaveExt {
            settings: self.settings,
            recipe_ids: self.rules().recipes.iter().map(|r| r.id.clone()).collect(),
            custom_rules: self.custom_rules.as_ref().map(|(t, _)| t.clone()),
            buildings: self.buildings.iter().map(|b| (b.temp, b.fertility)).collect(),
            generations: self.agents.iter().map(|a| a.generation).collect(),
            wear: self.agents.iter().map(|a| a.wear.clone()).collect(),
            nutrition: self.agents.iter().map(|a| a.nutrition).collect(),
            chunks: self.chunks_done.clone(),
        };
        let data = SaveData { core, air: self.air.clone(), bodies: self.bodies.clone(), ext };
        let payload = postcard::to_allocvec(&data).expect("serialize save");
        let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&checksum(&payload).to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<World, SaveError> {
        if bytes.len() < HEADER_LEN {
            return Err(SaveError::TooShort);
        }
        if &bytes[..4] != MAGIC {
            return Err(SaveError::BadMagic);
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if !readable(version) {
            return Err(SaveError::UnsupportedVersion(version));
        }
        let sum = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let payload = &bytes[HEADER_LEN..];
        if checksum(payload) != sum {
            return Err(SaveError::ChecksumMismatch);
        }
        let SaveData { core: data, air, bodies, ext } = decode(version, payload)?;
        // the save's recipe indexes must mean the same recipes: the rules it loads under (its own, or the base
        // rules) must start with the recipes it knew. Rules that only grew at the end, or retuned a recipe, are fine.
        // A save from an older format carries no list; its rules have only grown since, so it carries over.
        let own = match &ext.custom_rules {
            Some(text) => Some(crate::rules::Rules::parse(text).map_err(|_| SaveError::RulesMismatch)?),
            None => None,
        };
        let now = own.as_ref().unwrap_or_else(|| rules());
        let fits = ext.recipe_ids.len() <= now.recipes.len() && ext.recipe_ids.iter().zip(&now.recipes).all(|(a, b)| *a == b.id);
        if !fits {
            return Err(SaveError::RulesMismatch);
        }
        let _ = data.rules_checksum; // written for tools that read saves; the recipe list decides
        let ok_side = |s: i32| (crate::map::MIN_MAP_SIDE..=crate::map::MAX_MAP_SIDE).contains(&s);
        if !ok_side(data.map_w) || !ok_side(data.map_h) || data.map_w as i64 * data.map_h as i64 > crate::map::MAX_MAP_TILES {
            return Err(SaveError::WorldMismatch);
        }
        let mut w = World::for_loading(data.seed, data.map_w, data.map_h);
        // a world made in chunks: make them again in the order they were made (animals come from the save)
        for &(cx, cy) in &ext.chunks {
            w.make_chunk(cx, cy, false);
        }
        for &(i, amount) in &data.node_changes {
            if i as usize >= w.nodes.len() {
                return Err(SaveError::WorldMismatch);
            }
            w.set_node_amount(i as usize, amount);
        }
        w.node_version += 1;
        w.tick = data.tick;
        w.known = data.known;
        w.seen = data.seen;
        w.produced = data.produced;
        w.milestones = data.milestones;
        w.hardcore = data.hardcore;
        w.buildings = data.buildings;
        w.agents = data.agents;
        w.wild = data.wild;
        w.air = air;
        w.bodies = bodies;
        w.settings = ext.settings;
        if let (Some(text), Some(parsed)) = (ext.custom_rules, own) {
            w.custom_rules = Some((text, std::sync::Arc::new(parsed)));
        }
        if !ext.buildings.is_empty() {
            if ext.buildings.len() != w.buildings.len() {
                return Err(SaveError::WorldMismatch);
            }
            for (b, &(temp, soil)) in w.buildings.iter_mut().zip(&ext.buildings) {
                b.temp = temp;
                b.fertility = soil;
            }
        }
        if !ext.nutrition.is_empty() {
            if ext.nutrition.len() != w.agents.len() {
                return Err(SaveError::WorldMismatch);
            }
            for (a, n) in w.agents.iter_mut().zip(ext.nutrition) {
                a.nutrition = n;
            }
        }
        if !ext.wear.is_empty() {
            if ext.wear.len() != w.agents.len() {
                return Err(SaveError::WorldMismatch);
            }
            for (a, wear) in w.agents.iter_mut().zip(ext.wear) {
                a.wear = wear;
            }
        }
        if !ext.generations.is_empty() {
            if ext.generations.len() != w.agents.len() {
                return Err(SaveError::WorldMismatch);
            }
            for (a, &g) in w.agents.iter_mut().zip(&ext.generations) {
                a.generation = g;
            }
        }
        // the hash covers every item slot and every falling thing, so an upgraded save cannot match the one it was
        // written with
        if version == FORMAT_VERSION && w.hash() != data.state_hash {
            return Err(SaveError::HashMismatch);
        }
        Ok(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, BuildingKind};
    use crate::command::{Command, CommandKind, NodeId};
    use crate::map::{ItemKind, NodeKind};

    fn played_world() -> World {
        let mut w = World::new(7);
        let (sx, sy) = w.spawn_mm;
        let a = w.spawn_agent(sx, sy);
        let tree = NodeId(w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap() as u32);
        w.hardcore = true;
        w.step(&[Command { agent: a, kind: CommandKind::Gather { node: tree } }]);
        for _ in 0..4000 {
            w.step(&[]);
        }
        w.agents[0].inventory.counts[ItemKind::Wood as usize] = 3;
        w.step(&[]); // discovery happens at end of step
        let (x, y) = (w.agents[0].x + 3000, w.agents[0].y);
        w.step(&[Command { agent: a, kind: CommandKind::Build { kind: BuildingKind::Campfire, x, y } }]);
        for _ in 0..50 {
            w.step(&[]);
        }
        // M5 state: a depot with goods, a worker on a route, a kiln with auto + buffered input, wild animals
        let (dx, dy) = (w.agents[0].x - 3000, w.agents[0].y);
        w.buildings.push(Building { output: [3; ItemKind::COUNT], ..Building::new(BuildingKind::Depot, dx, dy) });
        let bot = w.spawn_worker(crate::agents::AgentKind::Bot, dx, dy);
        let route = vec![
            crate::agents::RouteStep::Deposit { building: 1 },
            crate::agents::RouteStep::Wait { ticks: 40 },
        ];
        w.step(&[Command { agent: a, kind: CommandKind::SetRoute { worker: bot, route } }]);
        for _ in 0..100 {
            w.step(&[]);
        }
        w
    }

    #[test]
    fn roundtrip_and_continue_identically() {
        let mut a = played_world();
        let mut b = World::from_bytes(&a.to_bytes()).expect("load");
        assert_eq!(a.hash(), b.hash());
        assert!(b.hardcore && !b.buildings.is_empty());
        assert_eq!(b.agents.len(), 2);
        assert_eq!(b.agents[1].route.len(), 2);
        assert_eq!(b.wild.len(), a.wild.len());
        for _ in 0..3000 {
            a.step(&[]);
            b.step(&[]);
        }
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn depleted_nodes_stay_depleted() {
        let mut w = played_world();
        let i = w.nodes.iter().position(|n| n.amount == 0);
        // force one node depleted
        let idx = i.unwrap_or(0);
        let (tx, ty) = (w.nodes[idx].tx, w.nodes[idx].ty);
        w.set_node_amount(idx, 0);
        let b = World::from_bytes(&w.to_bytes()).unwrap();
        assert!(b.node_at(tx, ty).is_none());
    }

    #[test]
    fn rejects_corrupt_and_foreign_data() {
        let good = played_world().to_bytes();
        assert_eq!(World::from_bytes(&good[..5]).err(), Some(SaveError::TooShort));
        let mut bad = good.clone();
        bad[0] = b'X';
        assert_eq!(World::from_bytes(&bad).err(), Some(SaveError::BadMagic));
        let mut bad = good.clone();
        bad[4] = 99;
        assert_eq!(World::from_bytes(&bad).err(), Some(SaveError::UnsupportedVersion(99)));
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        assert_eq!(World::from_bytes(&bad).err(), Some(SaveError::ChecksumMismatch));
    }

    /// Wrap a payload in a header of the given version.
    fn framed(version: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&checksum(payload).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn payload_of(bytes: &[u8]) -> SaveData {
        postcard::from_bytes(&bytes[HEADER_LEN..]).expect("decode")
    }

    #[test]
    fn settings_soil_heat_generations_and_own_rules_survive_a_save() {
        let mut w = played_world();
        w.settings = WorldSettings { difficulty: crate::world::Difficulty::Realistic, season_days: 9 };
        w.buildings[0].temp = 640;
        w.buildings[0].fertility = 720;
        w.agents[1].generation = 4;
        let own = format!("{}\n[[recipe]]\nid = \"own_test\"\nname = \"Deneme\"\ninputs = [{{ item = \"stone\", n = 1 }}]\noutputs = [{{ item = \"stone\", n = 1 }}]\nticks = 0\ndiscovery = \"sharp_stone\"\n", crate::rules::Rules::base_text());
        w.set_custom_rules(own).expect("own rules parse");
        let b = World::from_bytes(&w.to_bytes()).expect("load");
        assert_eq!(b.settings, w.settings);
        assert_eq!((b.buildings[0].temp, b.buildings[0].fertility, b.agents[1].generation), (640, 720, 4));
        assert_eq!(b.rules().recipes.len(), rules().recipes.len() + 1, "the world keeps its own rules");
        assert_eq!(b.hash(), w.hash());
    }

    /// A format-20 save (no extension block) loads: forgiving, the 3-day seasons it was played with, full soil.
    #[test]
    fn a_format_20_save_is_upgraded() {
        let w = played_world();
        let d = payload_of(&w.to_bytes());
        let old = postcard::to_allocvec(&(&d.core, &d.air, &d.bodies)).unwrap();
        let b = World::from_bytes(&framed(PRE_EXT_VERSION, &old)).expect("format 20 loads");
        assert_eq!(b.settings.season_days as u64, crate::time::SEASON_DAYS);
        assert_eq!(b.settings.difficulty, crate::world::Difficulty::Forgiving);
        assert!(b.buildings.iter().all(|x| x.fertility == 1000));
        assert_eq!(b.agents.len(), w.agents.len());
    }

    /// A format-21 save (no tool wear) loads with every tool unworn.
    #[test]
    fn a_format_21_save_is_upgraded() {
        let mut w = played_world();
        w.settings.season_days = 9;
        let d = payload_of(&w.to_bytes());
        let e = &d.ext;
        let old = postcard::to_allocvec(&(&d.core, &d.air, &d.bodies, (&e.settings, &e.recipe_ids, &e.custom_rules, &e.buildings, &e.generations))).unwrap();
        let b = World::from_bytes(&framed(PRE_WEAR_VERSION, &old)).expect("format 21 loads");
        assert_eq!(b.settings.season_days, 9);
        assert!(b.agents.iter().all(|a| a.wear.is_empty()));
    }

    /// A format-23 save (no chunk list) loads as it was.
    #[test]
    fn a_format_23_save_is_upgraded() {
        let w = played_world();
        let d = payload_of(&w.to_bytes());
        let e = &d.ext;
        let old = postcard::to_allocvec(&(&d.core, &d.air, &d.bodies, (&e.settings, &e.recipe_ids, &e.custom_rules, &e.buildings, &e.generations, &e.wear, &e.nutrition))).unwrap();
        let b = World::from_bytes(&framed(PRE_CHUNK_VERSION, &old)).expect("format 23 loads");
        assert_eq!(b.hash(), w.hash());
    }

    /// A format-22 save (no food groups) loads with everyone half fed.
    #[test]
    fn a_format_22_save_is_upgraded() {
        let w = played_world();
        let d = payload_of(&w.to_bytes());
        let e = &d.ext;
        let old = postcard::to_allocvec(&(&d.core, &d.air, &d.bodies, (&e.settings, &e.recipe_ids, &e.custom_rules, &e.buildings, &e.generations, &e.wear))).unwrap();
        let b = World::from_bytes(&framed(PRE_NUTRITION_VERSION, &old)).expect("format 22 loads");
        assert!(b.agents.iter().all(|a| a.nutrition == [600; 3]));
    }

    /// Rules may grow at the end: a save that knew fewer recipes loads; one whose recipes are not a prefix of the
    /// rules (reordered, or from rules this build does not have) is refused.
    #[test]
    fn rules_that_only_grew_still_load_old_saves() {
        let w = played_world();
        let mut d = payload_of(&w.to_bytes());
        d.ext.recipe_ids.truncate(5);
        d.core.state_hash = w.hash();
        let shorter = postcard::to_allocvec(&d).unwrap();
        assert!(World::from_bytes(&framed(FORMAT_VERSION, &shorter)).is_ok(), "a save from before the last recipes were added");
        let mut d = payload_of(&w.to_bytes());
        d.ext.recipe_ids.swap(0, 1);
        let reordered = postcard::to_allocvec(&d).unwrap();
        assert_eq!(World::from_bytes(&framed(FORMAT_VERSION, &reordered)).err(), Some(SaveError::RulesMismatch));
        let mut d = payload_of(&w.to_bytes());
        d.ext.recipe_ids.push("from_the_future".into());
        let future = postcard::to_allocvec(&d).unwrap();
        assert_eq!(World::from_bytes(&framed(FORMAT_VERSION, &future)).err(), Some(SaveError::RulesMismatch));
    }
}
