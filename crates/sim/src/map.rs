use crate::rng::{fbm, hash2};

/// Default map size in tiles. Worlds can be larger (`World::with_size`), up to `MAX_MAP_TILES`.
pub const MAP_W: i32 = 256;
pub const MAP_H: i32 = 256;
pub const MIN_MAP_SIDE: i32 = 32;
pub const MAX_MAP_SIDE: i32 = 262_144;
/// Hard cap on w*h so a corrupt or hostile save cannot ask for terabytes.
pub const MAX_MAP_TILES: i64 = 1 << 36;
/// Worlds up to this many tiles are made all at once; larger ones are made in chunks as people come near.
pub const EAGER_MAX_TILES: i64 = 4096 * 4096;
/// A chunk's side in tiles.
pub const CHUNK: i32 = 64;
/// One tile = 1 m = 1000 mm.
pub const TILE_MM: i64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Terrain {
    Water,
    Sand,
    Grass,
    Forest,
    Rock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Stone,
    Wood,
    Berries,
    SharpStone,
    StoneAxe,
    Clay,
    Charcoal,
    IronOre,
    IronBloom,
    RawMeat,
    CookedMeat,
    Spear,
    IronAxe,
    IronPickaxe,
    Gear,
    Herb,
    Bandage,
    CopperOre,
    TinOre,
    Bronze,
    BronzeAxe,
    BronzePickaxe,
    Branch,
    Fiber,
    Clamp,
    LogPile,
    Hide,
    FiberWrap,
    HideCloak,
    HideBoots,
    Torch,
    Splint,
    /// Struck against stone it throws the sparks that light a fire; kept, not used up.
    Flint,
    // --- Neolithic: the first farmers ---
    /// Wild grass seed, shaken from tall grass; sown in a field.
    Seed,
    /// Grain harvested from a sown field.
    Grain,
    /// Grain ground on a quern.
    Flour,
    /// Flour baked at the fire: the most filling food so far.
    Bread,
    /// A stone blade on a crooked stick, to break the ground for a field; kept.
    Hoe,
    /// A flat grinding stone and its rubber; kept.
    Quern,
    /// Coiled clay, shaped but not yet fired: it crumbles in water.
    RawPot,
    /// A fired clay pot, empty. Filled at the water when drinking.
    Pot,
    /// A pot full of water: drink from it anywhere, the pot is kept.
    WaterPot,
    /// Woven cloth from the loom.
    Cloth,
    /// A woven tunic: worn while carried, warmer than a fibre wrap.
    Tunic,
    /// A woven bag slung over the shoulder: a person carries more; kept.
    Bag,
    // --- Bronze age: draught animals ---
    /// A solid wooden wheel with a bronze-bound hub.
    Wheel,
    /// A two-wheeled cart: hitched to an ox or a horse, it carries far more.
    Cart,
    /// A bronze-tipped plough: hitched to an ox or a horse standing at a field, the crop grows twice as fast.
    Plow,
    // --- Stone age tools, from loose stones and a stick ---
    /// A round stone lashed to a stick: breaks rock seams; kept.
    Hammer,
    /// A knapped blade in a stick: skins game, cuts fibre and herbs fast; kept.
    Knife,
    /// A flat stone on a stick: digs clay and sand fast; kept.
    Shovel,
    // --- Neolithic, the founder crops and the soil ---
    /// Lentils: sown like grain on dry land; cooked into soup.
    Lentil,
    /// Flax: sown where it is wet; its stems give long fibre for linen.
    Flax,
    /// Lentil soup, cooked in a pot of water: filling and warm.
    Soup,
    /// Dung from penned animals: worked into a field it restores the soil. Rotten food turns into it too.
    Manure,
    /// A big storage jar, coiled but not fired.
    RawLargePot,
    /// A big fired storage jar: kept in a depot, it slows the food there from spoiling.
    LargePot,
    // --- Copper age: casting ---
    /// A crucible of tempered clay, not fired.
    RawCrucible,
    /// A fired crucible: holds molten metal in a pit kiln; kept.
    Crucible,
    /// A crucible full of molten copper, ready to pour.
    MoltenCopper,
    /// An axe-head mould pressed in clay, not fired.
    RawMoldAxe,
    /// A fired axe-head mould.
    MoldAxe,
    /// A pick-head mould, not fired.
    RawMoldPick,
    /// A fired pick-head mould.
    MoldPick,
    /// A blade mould, not fired.
    RawMoldKnife,
    /// A fired blade mould.
    MoldKnife,
    /// An ingot mould, not fired.
    RawMoldIngot,
    /// A fired ingot mould.
    MoldIngot,
    /// A cast copper ingot.
    CopperIngot,
    /// A cast copper axe head.
    CopperAxeHead,
    /// A cast copper pick head.
    CopperPickHead,
    /// A cast copper blade.
    CopperKnifeHead,
    /// A copper axe: fells trees faster than stone; kept.
    CopperAxe,
    /// A copper pick: opens tin seams and the second cave chamber; kept.
    CopperPickaxe,
    /// A copper knife; kept.
    CopperKnife,
    // --- Bronze by ratio, and cast ---
    /// Copper and tin melted in the wrong share: brittle, good for nothing but remelting.
    BadBronze,
    /// A crucible full of molten bronze.
    MoltenBronze,
    /// A cast bronze axe head.
    BronzeAxeHead,
    /// A cast bronze pick head.
    BronzePickHead,
    /// A sawn board.
    Plank,
    /// A forged bronze saw: cuts logs into planks; kept.
    Saw,
    /// Iron straight from the bloomery, full of slag: beaten on an anvil it becomes wrought iron.
    SpongeIron,
    /// Limestone from the seam: the flux that carries slag out of a blast furnace.
    Limestone,
    /// Pale clay that stands great heat.
    FireClay,
    /// A fire brick, not fired.
    RawFireBrick,
    /// A fired fire brick.
    FireBrick,
    /// Iron run molten from the blast furnace: brittle, to be cast or beaten to steel.
    PigIron,
    /// Pig iron beaten once: still too hard and brittle.
    HighCarbonSteel,
    /// Steel: hard and tough.
    Steel,
    /// Coal from deep seams: burns hotter and longer than charcoal.
    Coal,
    /// Coal baked in a furnace: the hottest fuel.
    Coke,
    /// A cast-iron machine part: cylinder, plate or wheel.
    CastIronPart,
    /// Clean sand from the bottom of a clay bed.
    Sand,
    /// Glass, melted from sand.
    Glass,
    /// Drawn copper wire.
    CopperWire,
    /// A magnetised steel bar.
    Magnet,
    /// A glass bulb with a carbon filament.
    LightBulb,
    /// Silicon, reduced from sand in the blast furnace.
    Silicon,
    /// A transistor: a switch with no moving parts.
    Transistor,
    /// A circuit board: transistors wired on a board.
    CircuitBoard,
    /// Rock oil from a deep deposit: burns in a steam engine.
    Oil,
    /// A light pick that reads the rock: what lies beneath, and which way the nearest deposit is; kept.
    ProspectorPick,
}

impl ItemKind {
    /// Tools stay with their owner; depots never take them.
    pub fn is_tool(self) -> bool {
        matches!(self, ItemKind::StoneAxe
                | ItemKind::IronAxe
                | ItemKind::IronPickaxe
                | ItemKind::BronzeAxe
                | ItemKind::BronzePickaxe
                | ItemKind::Clamp
                | ItemKind::FiberWrap
                | ItemKind::HideCloak
                | ItemKind::HideBoots
                | ItemKind::Spear
                | ItemKind::Flint
                | ItemKind::Hoe
                | ItemKind::Quern
                | ItemKind::Tunic
                | ItemKind::Bag
                | ItemKind::Cart
                | ItemKind::Plow
                | ItemKind::Hammer
                | ItemKind::Knife
                | ItemKind::Shovel
                | ItemKind::Crucible
                | ItemKind::MoldAxe
                | ItemKind::MoldPick
                | ItemKind::MoldKnife
                | ItemKind::MoldIngot
                | ItemKind::CopperAxe
                | ItemKind::CopperPickaxe
                | ItemKind::CopperKnife
                | ItemKind::Saw
                | ItemKind::ProspectorPick
        )
    }

    /// Clothes are worn while they are carried: how much warmer they keep the wearer (in tenths of a degree).
    pub fn warmth_bonus(self) -> i32 {
        match self {
            ItemKind::FiberWrap => 10,
            ItemKind::HideCloak => 25,
            ItemKind::HideBoots => 10,
            ItemKind::Tunic => 20,
            _ => 0,
        }
    }

    pub const COUNT: usize = 102;
    pub const ALL: [ItemKind; Self::COUNT] = [
        ItemKind::Stone,
        ItemKind::Wood,
        ItemKind::Berries,
        ItemKind::SharpStone,
        ItemKind::StoneAxe,
        ItemKind::Clay,
        ItemKind::Charcoal,
        ItemKind::IronOre,
        ItemKind::IronBloom,
        ItemKind::RawMeat,
        ItemKind::CookedMeat,
        ItemKind::Spear,
        ItemKind::IronAxe,
        ItemKind::IronPickaxe,
        ItemKind::Gear,
        ItemKind::Herb,
        ItemKind::Bandage,
        ItemKind::CopperOre,
        ItemKind::TinOre,
        ItemKind::Bronze,
        ItemKind::BronzeAxe,
        ItemKind::BronzePickaxe,
        ItemKind::Branch,
        ItemKind::Fiber,
        ItemKind::Clamp,
        ItemKind::LogPile,
        ItemKind::Hide,
        ItemKind::FiberWrap,
        ItemKind::HideCloak,
        ItemKind::HideBoots,
        ItemKind::Torch,
        ItemKind::Splint,
        ItemKind::Flint,
        ItemKind::Seed,
        ItemKind::Grain,
        ItemKind::Flour,
        ItemKind::Bread,
        ItemKind::Hoe,
        ItemKind::Quern,
        ItemKind::RawPot,
        ItemKind::Pot,
        ItemKind::WaterPot,
        ItemKind::Cloth,
        ItemKind::Tunic,
        ItemKind::Bag,
        ItemKind::Wheel,
        ItemKind::Cart,
        ItemKind::Plow,
        ItemKind::Hammer,
        ItemKind::Knife,
        ItemKind::Shovel,
        ItemKind::Lentil,
        ItemKind::Flax,
        ItemKind::Soup,
        ItemKind::Manure,
        ItemKind::RawLargePot,
        ItemKind::LargePot,
        ItemKind::RawCrucible,
        ItemKind::Crucible,
        ItemKind::MoltenCopper,
        ItemKind::RawMoldAxe,
        ItemKind::MoldAxe,
        ItemKind::RawMoldPick,
        ItemKind::MoldPick,
        ItemKind::RawMoldKnife,
        ItemKind::MoldKnife,
        ItemKind::RawMoldIngot,
        ItemKind::MoldIngot,
        ItemKind::CopperIngot,
        ItemKind::CopperAxeHead,
        ItemKind::CopperPickHead,
        ItemKind::CopperKnifeHead,
        ItemKind::CopperAxe,
        ItemKind::CopperPickaxe,
        ItemKind::CopperKnife,
        ItemKind::BadBronze,
        ItemKind::MoltenBronze,
        ItemKind::BronzeAxeHead,
        ItemKind::BronzePickHead,
        ItemKind::Plank,
        ItemKind::Saw,
        ItemKind::SpongeIron,
        ItemKind::Limestone,
        ItemKind::FireClay,
        ItemKind::RawFireBrick,
        ItemKind::FireBrick,
        ItemKind::PigIron,
        ItemKind::HighCarbonSteel,
        ItemKind::Steel,
        ItemKind::Coal,
        ItemKind::Coke,
        ItemKind::CastIronPart,
        ItemKind::Sand,
        ItemKind::Glass,
        ItemKind::CopperWire,
        ItemKind::Magnet,
        ItemKind::LightBulb,
        ItemKind::Silicon,
        ItemKind::Transistor,
        ItemKind::CircuitBoard,
        ItemKind::Oil,
        ItemKind::ProspectorPick,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    StoneOutcrop,
    Tree,
    BerryBush,
    ClayDeposit,
    IronOutcrop,
    HerbPatch,
    CopperOutcrop,
    TinOutcrop,
    /// A fallen branch on the ground.
    Deadwood,
    /// Tall grass with long fibres.
    FiberPlant,
    /// A cave mouth in the rocky heights: a rich seam of stone just inside.
    CaveMouth,
}

impl NodeKind {
    pub fn item(self) -> ItemKind {
        match self {
            NodeKind::StoneOutcrop => ItemKind::Stone,
            NodeKind::Tree => ItemKind::Wood,
            NodeKind::BerryBush => ItemKind::Berries,
            NodeKind::ClayDeposit => ItemKind::Clay,
            NodeKind::IronOutcrop => ItemKind::IronOre,
            NodeKind::HerbPatch => ItemKind::Herb,
            NodeKind::CopperOutcrop => ItemKind::CopperOre,
            NodeKind::TinOutcrop => ItemKind::TinOre,
            NodeKind::Deadwood => ItemKind::Branch,
            NodeKind::FiberPlant => ItemKind::Fiber,
            NodeKind::CaveMouth => ItemKind::Stone,
        }
    }
    pub fn initial_amount(self) -> u16 {
        match self {
            NodeKind::StoneOutcrop => 5,
            NodeKind::Tree => 8,
            NodeKind::BerryBush => 6,
            NodeKind::ClayDeposit => 6,
            NodeKind::IronOutcrop => 4,
            NodeKind::HerbPatch => 3,
            NodeKind::CopperOutcrop => 5,
            NodeKind::TinOutcrop => 3,
            NodeKind::Deadwood => 3,
            NodeKind::FiberPlant => 4,
            NodeKind::CaveMouth => 16,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Map {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Terrain>,
    pub seed: u64,
}

/// Run `f(y0, y1)` over horizontal bands of rows on several threads and join the results in row order,
/// so the output is identical to a single-threaded pass. Small jobs stay on the calling thread.
pub(crate) fn par_rows<T: Send>(w: i32, h: i32, f: impl Fn(i32, i32) -> Vec<T> + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8) as i32;
    if threads <= 1 || (w as i64 * h as i64) < 1 << 20 {
        return f(0, h);
    }
    let band = (h + threads - 1) / threads;
    std::thread::scope(|s| {
        let f = &f;
        let jobs: Vec<_> = (0..threads)
            .map(|i| (i * band, ((i + 1) * band).min(h)))
            .filter(|(a, b)| a < b)
            .map(|(a, b)| s.spawn(move || f(a, b)))
            .collect();
        jobs.into_iter().flat_map(|j| j.join().expect("worker thread")).collect()
    })
}

/// Ground height above the shoreline in millimetres at the corner of tile (tx, ty): flat water and beach, then gentle
/// hills over grass and forest, and rocky heights where the rock begins. Integer-only, so every peer agrees.
pub fn elevation_mm(seed: u64, tx: i32, ty: i32) -> i32 {
    let h = height_field(seed, tx, ty);
    base_height(h) + ledges(seed, tx, ty, h)
}

/// The top 16 bits of a seed that marks a world made by the second generator (rivers and lakes). Older worlds, whose
/// seeds never carry it (clock-based or typed), keep the land they were made with.
const GEN_V2_TAG: u64 = 0xA1A2;

/// Does this world have rivers and lakes (the second generator, or the third)?
pub fn gen_v2(seed: u64) -> bool {
    seed >> 48 == GEN_V2_TAG || gen_v3(seed)
}

/// The third generator's mark: continents and oceans, mountain ranges, climate belts.
const GEN_V3_TAG: u64 = 0xA1A3;

/// Was this world made by the third generator?
pub fn gen_v3(seed: u64) -> bool {
    seed >> 48 == GEN_V3_TAG
}

/// The seed a new world gets: the third generator's mark.
pub fn tag_seed_v3(seed: u64) -> u64 {
    (seed & 0xFFFF_FFFF_FFFF) | GEN_V3_TAG << 48
}

/// The land's height before rivers are cut, 0..=255 (below 95 is sea).
pub fn raw_height(seed: u64, tx: i32, ty: i32) -> i32 {
    if gen_v3(seed) { height_v3(seed, tx, ty) } else { fbm(seed ^ 0x1111, tx, ty) as i32 }
}

/// The third generator's land: continents a few kilometres across with oceans between, coasts bent and frayed by a
/// warped field, rolling hills on them, and long mountain ranges (sharp ridges) running through belts of the land.
fn height_v3(seed: u64, x: i32, y: i32) -> i32 {
    let vn = |salt: u64, x: i32, y: i32, cell: i32| crate::rng::value_noise(seed ^ salt, x, y, cell) as i32;
    // bend the coasts and ranges so nothing runs in straight noise-grid lines
    let wx = x + (vn(0x31, x, y, 2200) - 128) * 5 + (vn(0x33, x, y, 260) - 128) / 3;
    let wy = y + (vn(0x32, x, y, 2200) - 128) * 5 + (vn(0x34, x, y, 260) - 128) / 3;
    let cont = (vn(0x41, wx, wy, 9000) * 4 + vn(0x42, wx, wy, 3800) * 2 + vn(0x43, wx, wy, 1500)) / 7;
    let hills = (vn(0x51, wx, wy, 420) * 4 + vn(0x52, wx, wy, 150) * 2 + vn(0x53, x, y, 40)) / 7;
    let ridge = 255 - (2 * vn(0x61, wx, wy, 1100) - 255).abs();
    let belt = ((vn(0x71, wx, wy, 6000) - 140) * 4).clamp(0, 255);
    let land = (cont - 122) * 3 / 4;
    // mountains rise in their belts, on land, along the ridge lines
    let mountains = if land > 0 { (ridge - 150).max(0) * belt / 255 * 13 / 10 } else { 0 };
    let rolling = (hills - 128) * if land > 0 { 35 } else { 20 } / 100;
    (100 + land + rolling + mountains).clamp(0, 255)
}

/// The seed a new world gets: the second generator's mark on its top bits.
pub fn tag_seed_v2(seed: u64) -> u64 {
    (seed & 0xFFFF_FFFF_FFFF) | GEN_V2_TAG << 48
}

/// The noise whose mid-line (128) the rivers follow. The third generator's is far broader: fewer, longer rivers
/// that wind across a continent.
fn river_field(seed: u64, x: i32, y: i32) -> i64 {
    let (a, b) = if gen_v3(seed) { (1400, 320) } else { (160, 40) };
    (crate::rng::value_noise(seed ^ 0x9191, x, y, a) * 3 + crate::rng::value_noise(seed ^ 0x9292, x, y, b)) / 4
}

/// The noise whose peaks are lakes (rarer and larger in the third generator).
fn lake_field(seed: u64, x: i32, y: i32) -> i64 {
    if gen_v3(seed) {
        // squeezed so only the highest tenth or so of its peaks clear 214
        (crate::rng::value_noise(seed ^ 0x7A7A, x, y, 420) - 30).max(0)
    } else {
        crate::rng::value_noise(seed ^ 0x7A7A, x, y, 90)
    }
}

/// The raw height field (0..=255, below 95 is water). The second generator cuts rivers and lakes into it: rivers
/// follow the mid-line of a broad noise field, so they run on unbroken and need nothing beyond the tile itself
/// (ready for a world made in chunks); they rise as streams in the hills and widen into rivers in the lowlands, with
/// banks sloping down to them. Lakes fill hollows where a second noise field is high.
pub fn height_field(seed: u64, tx: i32, ty: i32) -> i32 {
    // the older generator is cheap enough to work out every time; the newer ones are remembered by 64x64 block
    if !gen_v2(seed) {
        return height_field_uncached(seed, tx, ty);
    }
    land_block(seed, tx, ty, |b, i| b.h[i] as i32)
}

/// A 64x64 block of land worked out together: height, ground, warmth and wetness.
struct LandBlock {
    h: [u8; 4096],
    t: [u8; 4096],
    warm: [u16; 4096],
    wet: [u16; 4096],
}

/// Remembered blocks: about 24 MB at most (1024 blocks); when full, the farthest quarter is let go. Only pure functions of the seed are
/// kept, so what is remembered never changes what the game does.
const LAND_CACHE_BLOCKS: usize = 1024;
type LandBlocks = std::sync::Mutex<std::collections::HashMap<(u64, i32, i32), std::sync::Arc<LandBlock>>>;
static LAND_CACHE: std::sync::OnceLock<LandBlocks> = std::sync::OnceLock::new();

/// Look a tile up in its remembered block, working the block out first if need be.
fn land_block<T>(seed: u64, tx: i32, ty: i32, read: impl Fn(&LandBlock, usize) -> T) -> T {
    let key = (seed, tx.div_euclid(64), ty.div_euclid(64));
    let at = (ty.rem_euclid(64) * 64 + tx.rem_euclid(64)) as usize;
    // neighbouring lookups mostly fall in the same block: each thread keeps the last one at hand, no lock needed
    if let Some(out) = LAST_BLOCK.with(|l| l.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, b)| read(b, at))) {
        return out;
    }
    let cache = LAND_CACHE.get_or_init(Default::default);
    let found = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
    if let Some(b) = found {
        let out = read(&b, at);
        LAST_BLOCK.with(|l| *l.borrow_mut() = Some((key, b)));
        return out;
    }
    // worked out outside the lock, so other threads are not held up
    let (bx, by) = (key.1 * 64, key.2 * 64);
    let mut b = Box::new(LandBlock { h: [0; 4096], t: [0; 4096], warm: [0; 4096], wet: [0; 4096] });
    for i in 0..4096 {
        let (x, y) = (bx + i as i32 % 64, by + i as i32 / 64);
        let h = height_field_uncached(seed, x, y);
        let (warm, wet) = climate_raw(seed, x, y, base_height(h));
        b.h[i] = h as u8;
        b.warm[i] = warm as u16;
        b.wet[i] = wet as u16;
        b.t[i] = terrain_raw(seed, x, y, h, warm, wet) as u8;
    }
    let b: std::sync::Arc<LandBlock> = b.into();
    let out = read(&b, at);
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if c.len() >= LAND_CACHE_BLOCKS {
        // full: let go of the quarter farthest from where we are now looking, keep the rest
        let mut keys: Vec<(i64, (u64, i32, i32))> = c
            .keys()
            .map(|&k| (if k.0 == seed { (k.1 - key.1).abs().max((k.2 - key.2).abs()) as i64 } else { i64::MAX }, k))
            .collect();
        keys.sort_unstable();
        for (_, k) in &keys[LAND_CACHE_BLOCKS * 3 / 4..] {
            c.remove(k);
        }
    }
    c.insert(key, b.clone());
    drop(c);
    LAST_BLOCK.with(|l| *l.borrow_mut() = Some((key, b)));
    out
}

thread_local! {
    static LAST_BLOCK: std::cell::RefCell<Option<((u64, i32, i32), std::sync::Arc<LandBlock>)>> = const { std::cell::RefCell::new(None) };
}

fn height_field_uncached(seed: u64, tx: i32, ty: i32) -> i32 {
    let h = raw_height(seed, tx, ty);
    if !gen_v2(seed) || h <= 94 {
        return h;
    }
    let mut out = h;
    // how lowland this is: 0 high in the hills (no river), 60 down by the sea
    let low = (165 - h).clamp(0, 60);
    if low > 0 {
        let n = river_field(seed, tx, ty) as i32;
        let d = (n - 128).abs();
        let core = 1 + low / 15;
        let bank = core + 5;
        if d < bank {
            let t = (d - core).max(0) * 1000 / (bank - core);
            out = out.min(94 + (h - 94) * t / 1000);
        }
    }
    // lakes in the hollows of the lower country
    let l = lake_field(seed, tx, ty) as i32;
    if l > 214 && (108..150).contains(&h) {
        let t = (224 - l).max(0) * 1000 / 10;
        out = out.min(94 + (h - 94) * t / 1000);
    }
    out
}

/// Gentle rises over the lowlands, and the rock above them climbing on into real peaks.
fn base_height(h: i32) -> i32 {
    (h - 108).max(0) * 80 + (h - 150).max(0) * 90
}

/// The height without the cliffs: what the world was generated on (caves, climate), so cliffs do not move them.
pub(crate) fn base_elevation_mm(seed: u64, tx: i32, ty: i32) -> i32 {
    base_height(height_field(seed, tx, ty))
}

/// Fbm height where the rock starts breaking into ledges, how many fbm steps each ledge spans, and its drop.
const LEDGE_FROM: i32 = 160;
const LEDGE_STEP: i32 = 9;

/// How high ledge `i` of a mountain stands above the one below it (mm): 1.6 to 4 m.
fn ledge_rise(seed: u64, i: i32) -> i32 {
    1_600 + (crate::rng::hash2(seed ^ 0x1ED6, i, 0) % 2_400) as i32
}

/// The rocky heights climb in ledges: in some places a sheer drop between two levels (a cliff), in others the same
/// rise spread out into a scree ramp, so every height can still be walked up somewhere.
fn ledges(seed: u64, tx: i32, ty: i32, h: i32) -> i32 {
    let r = h - LEDGE_FROM;
    if r <= 0 {
        return 0;
    }
    let (q, rem) = (r / LEDGE_STEP, r % LEDGE_STEP);
    let below: i32 = (0..q).map(|i| ledge_rise(seed, i)).sum();
    let ramp = below + ledge_rise(seed, q) * rem / LEDGE_STEP;
    // where the rock is sheer, in broad patches
    let n = crate::rng::value_noise(seed ^ 0xC11F, tx, ty, 18) as i32;
    let sheer = ((n - 118) * 1000 / 30).clamp(0, 1000);
    (ramp * (1000 - sheer) + below * sheer) / 1000
}

/// How much the ground drops between two neighbouring tile corners for it to count as a cliff (mm).
pub const CLIFF_MM: i32 = 1_300;

/// Climate at tile corner (tx, ty), each 0..=1000: (warmth, wetness). Broad patches, colder up in the mountains.
/// Purely cosmetic and integer-only: it picks which kind of country the ground, trees and flowers belong to.
pub fn climate(seed: u64, tx: i32, ty: i32) -> (i32, i32) {
    if gen_v2(seed) {
        return land_block(seed, tx, ty, |b, i| (b.warm[i] as i32, b.wet[i] as i32));
    }
    climate_raw(seed, tx, ty, base_elevation_mm(seed, tx, ty))
}

/// The climate worked out, given the tile's height without cliffs (mm).
fn climate_raw(seed: u64, tx: i32, ty: i32, base_mm: i32) -> (i32, i32) {
    if gen_v3(seed) {
        return climate_v3(seed, tx, ty, base_mm);
    }
    let n = |salt: u64| (crate::rng::value_noise(seed ^ salt, tx, ty, 72) * 3 + crate::rng::value_noise(seed ^ salt ^ 0x55, tx, ty, 28)) as i32 * 1000 / (255 * 4);
    let warm = n(0x7E3F) + 110 - base_mm / 45;
    (warm.clamp(0, 1000), n(0x3A11).clamp(0, 1000))
}

/// Smoothly interpolated ground height (mm) at a point given in mm, integer maths only.
pub fn elevation_at_mm(seed: u64, x: i64, y: i64) -> i32 {
    let (tx, ty) = (x.div_euclid(TILE_MM), y.div_euclid(TILE_MM));
    let (fx, fy) = ((x - tx * TILE_MM) as i32, (y - ty * TILE_MM) as i32);
    let e = |dx: i64, dy: i64| elevation_mm(seed, (tx + dx) as i32, (ty + dy) as i32) as i64;
    let a = e(0, 0) * (1000 - fx as i64) + e(1, 0) * fx as i64;
    let b = e(0, 1) * (1000 - fx as i64) + e(1, 1) * fx as i64;
    ((a * (1000 - fy as i64) + b * fy as i64) / 1_000_000) as i32
}

impl Map {
    pub fn generate(seed: u64, w: i32, h: i32) -> Self {
        // a very large world keeps no tile table: every tile is worked out from the seed when it is asked for
        if w as i64 * h as i64 > EAGER_MAX_TILES {
            return Self { w, h, tiles: Vec::new(), seed };
        }
        let tiles = par_rows(w, h, |y0, y1| {
            let mut tiles = Vec::with_capacity(((y1 - y0) * w) as usize);
            for y in y0..y1 {
                for x in 0..w {
                    tiles.push(terrain_at(seed, x, y));
                }
            }
            tiles
        });
        Self { w, h, tiles, seed }
    }

    /// Made in chunks (no tile table)?
    pub fn lazy(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn in_bounds(&self, tx: i32, ty: i32) -> bool {
        tx >= 0 && ty >= 0 && tx < self.w && ty < self.h
    }

    pub fn terrain(&self, tx: i32, ty: i32) -> Option<Terrain> {
        if !self.in_bounds(tx, ty) {
            return None;
        }
        Some(if self.tiles.is_empty() { terrain_at(self.seed, tx, ty) } else { self.tiles[(ty * self.w + tx) as usize] })
    }

    /// Terrain under a fixed-point (mm) position.
    pub fn terrain_mm(&self, x: i64, y: i64) -> Option<Terrain> {
        self.terrain(x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32)
    }

    pub fn water_within(&self, tx: i32, ty: i32, r: i32) -> bool {
        (-r..=r).any(|dy| (-r..=r).any(|dx| self.terrain(tx + dx, ty + dy) == Some(Terrain::Water)))
    }

    /// Deterministic resource node spawn for a tile, if any.
    pub fn node_for(&self, seed: u64, tx: i32, ty: i32) -> Option<NodeKind> {
        let r = hash2(seed ^ 0x3333, tx, ty) % 1000;
        match self.terrain(tx, ty)? {
            Terrain::Rock if (69..73).contains(&r) && base_elevation_mm(seed, tx, ty) >= 3600 => Some(NodeKind::CaveMouth),
            Terrain::Rock if r < 50 => Some(NodeKind::StoneOutcrop),
            Terrain::Rock if r < 58 => Some(NodeKind::IronOutcrop),
            Terrain::Rock if r < 66 => Some(NodeKind::CopperOutcrop),
            Terrain::Rock if r < 69 => Some(NodeKind::TinOutcrop),
            Terrain::Grass if r < 6 => Some(NodeKind::StoneOutcrop),
            Terrain::Grass if (6..26).contains(&r) => Some(NodeKind::BerryBush),
            Terrain::Grass if (26..40).contains(&r) => Some(NodeKind::Tree),
            Terrain::Grass if (40..46).contains(&r) => Some(NodeKind::HerbPatch),
            Terrain::Grass if (46..58).contains(&r) => Some(NodeKind::Deadwood),
            Terrain::Grass if (58..88).contains(&r) => Some(NodeKind::FiberPlant),
            // a small new world may have little or no rock to hold ores: some crop out of the grass instead, so the
            // ages of metal are still within reach (older worlds keep their land as it was)
            Terrain::Grass if self.w <= 512 && gen_v2(seed) && (88..91).contains(&r) => Some(NodeKind::CopperOutcrop),
            Terrain::Grass if self.w <= 512 && gen_v2(seed) && r == 91 => Some(NodeKind::TinOutcrop),
            Terrain::Grass if self.w <= 512 && gen_v2(seed) && (92..95).contains(&r) => Some(NodeKind::IronOutcrop),
            Terrain::Forest if r < 160 => Some(NodeKind::Tree),
            Terrain::Forest if (160..175).contains(&r) => Some(NodeKind::BerryBush),
            Terrain::Forest if (175..181).contains(&r) => Some(NodeKind::HerbPatch),
            Terrain::Forest if (181..235).contains(&r) => Some(NodeKind::Deadwood),
            Terrain::Forest if (235..255).contains(&r) => Some(NodeKind::FiberPlant),
            Terrain::Sand if r < 120 && self.water_within(tx, ty, 2) => Some(NodeKind::ClayDeposit),
            Terrain::Sand if r < 5 => Some(NodeKind::StoneOutcrop),
            _ => None,
        }
    }
}

/// How tall the tree on tile (tx, ty) stands (mm): saplings, grown trees and a few giants. The renderer draws it
/// from the same hash, so a falling trunk is exactly as long as the tree that was standing there.
pub fn tree_height_mm(seed: u64, tx: i32, ty: i32) -> i32 {
    let s = crate::rng::hash2(seed ^ 0x51, tx, ty) as u32;
    let class = (s >> 8) % 100;
    let pick = |k: u32| (s.rotate_left(k) % 1000) as i32;
    // saplings, young trees, grown trees, tall ones, and a rare old giant
    if class < 18 {
        1_100 + 1_300 * pick(13) / 1000
    } else if class < 45 {
        2_600 + 1_800 * pick(17) / 1000
    } else if class < 80 {
        4_400 + 2_600 * pick(19) / 1000
    } else if class < 95 {
        7_000 + 3_000 * pick(23) / 1000
    } else {
        10_000 + 5_000 * pick(29) / 1000
    }
}

/// Where the model of the node on tile (tx, ty) actually stands (mm): a little off the tile centre, as drawn.
pub fn node_anchor_mm(seed: u64, tx: i32, ty: i32) -> (i64, i64) {
    let s = crate::rng::hash2(seed ^ 0x51, tx, ty) as u32;
    let jit = |k: u32| 180 * ((s.rotate_left(k) % 1000) as i64 * 2 - 1000) / 1000;
    (tx as i64 * TILE_MM + TILE_MM / 2 + jit(3), ty as i64 * TILE_MM + TILE_MM / 2 + jit(7))
}

/// How deep the water on a tile is, as a body feels it: 0 dry land, 1 shallow (land within a tile: wade), 2 deep
/// (swim).
pub fn water_depth(map: &Map, tx: i32, ty: i32) -> u8 {
    if map.terrain(tx, ty) != Some(Terrain::Water) {
        return 0;
    }
    let shore = (-1..=1).any(|dy| (-1..=1).any(|dx| map.terrain(tx + dx, ty + dy).is_some_and(|t| t != Terrain::Water)));
    if shore { 1 } else { 2 }
}

/// The river current on a tile (mm per tick), zero off rivers (lakes and the sea lie still). A river runs along the
/// mid-line it was cut on, downhill: whichever way along it the land falls.
pub fn current_mm(map: &Map, tx: i32, ty: i32) -> (i64, i64) {
    let seed = map.seed;
    if !gen_v2(seed) || map.terrain(tx, ty) != Some(Terrain::Water) {
        return (0, 0);
    }
    let raw = |x: i32, y: i32| raw_height(seed, x, y) as i64;
    // the sea and lakes: water that was water before any river was cut, or a lake hollow
    if raw(tx, ty) < 95 || lake_field(seed, tx, ty) > 214 {
        return (0, 0);
    }
    let n = |x: i32, y: i32| river_field(seed, x, y);
    let (gx, gy) = (n(tx + 2, ty) - n(tx - 2, ty), n(tx, ty + 2) - n(tx, ty - 2));
    let len = ((gx * gx + gy * gy) as u64).isqrt() as i64;
    if len == 0 {
        return (0, 0);
    }
    // along the river: across the noise's slope
    let (mut ux, mut uy) = (-gy * 1000 / len, gx * 1000 / len);
    let ahead = |s: i64| raw(tx + (ux * s / 1000) as i32, ty + (uy * s / 1000) as i32);
    if ahead(6) > ahead(-6) {
        ux = -ux;
        uy = -uy;
    }
    // narrow upland streams run quicker than broad lowland rivers
    let speed = 4 + (raw(tx, ty) - 95).clamp(0, 60) / 10;
    (ux * speed / 1000, uy * speed / 1000)
}

/// The ground of a tile, from the seed alone.
pub fn terrain_at(seed: u64, x: i32, y: i32) -> Terrain {
    if gen_v2(seed) {
        return land_block(seed, x, y, |b, i| [Terrain::Water, Terrain::Sand, Terrain::Grass, Terrain::Forest, Terrain::Rock][b.t[i] as usize]);
    }
    let h = height_field(seed, x, y);
    terrain_raw(seed, x, y, h, 0, 0)
}

/// The ground worked out, given the tile's height and (third generator) its climate.
fn terrain_raw(seed: u64, x: i32, y: i32, h: i32, warm: i32, wet: i32) -> Terrain {
    if gen_v3(seed) {
        if h < 95 {
            return Terrain::Water;
        }
        if h < 99 {
            return Terrain::Sand;
        }
        if h > 168 {
            return Terrain::Rock;
        }
        return if warm > 660 && wet < 330 {
            // desert
            Terrain::Sand
        } else if wet > 540 || (warm < 380 && wet > 420) {
            Terrain::Forest
        } else {
            Terrain::Grass
        };
    }
    let height = h as i64;
    let moist = fbm(seed ^ 0x2222, x, y);
    if height < 95 {
        Terrain::Water
    } else if height < 108 {
        Terrain::Sand
    } else if height > 160 {
        Terrain::Rock
    } else if moist > 135 {
        Terrain::Forest
    } else {
        Terrain::Grass
    }
}

/// How long a climate belt cycle is, pole to pole and back, in tiles (24 km).
const CLIMATE_CYCLE: i32 = 24_000;

/// The third generator's climate: belts by latitude (warm in the middle of a cycle, cold at its ends; the middle
/// of the largest world sits in the temperate belt), colder with height, wetter and drier in broad patches.
fn climate_v3(seed: u64, tx: i32, ty: i32, base_mm: i32) -> (i32, i32) {
    let vn = |salt: u64, cell: i32| crate::rng::value_noise(seed ^ salt, tx, ty, cell) as i32;
    let t = (ty - MAX_MAP_SIDE / 2 + CLIMATE_CYCLE / 4).rem_euclid(CLIMATE_CYCLE);
    let lat = (t - CLIMATE_CYCLE / 2).abs() * 2000 / CLIMATE_CYCLE; // 0 at a pole .. 1000 at the equator
    let warm = 120 + lat * 82 / 100 + (vn(0x7E3F, 900) - 128) * 3 / 2 - base_mm / 40;
    let wet = (vn(0x3A11, 700) * 3 + vn(0x3A12, 160)) * 1000 / (255 * 4);
    (warm.clamp(0, 1000), wet.clamp(0, 1000))
}

/// sin of an angle in milliradians, permille (integer: Bhaskara's approximation, within about 2 permille).
pub fn sin_mrad(a: i64) -> i64 {
    let a = a.rem_euclid(6283);
    let (x, sign) = if a > 3141 { (a - 3141, -1) } else { (a, 1) };
    let p = x * (3141 - x);
    sign * 16 * p * 1000 / (5 * 3141 * 3141 - 4 * p)
}

/// cos of an angle in milliradians, permille.
pub fn cos_mrad(a: i64) -> i64 {
    sin_mrad(a + 1571)
}
