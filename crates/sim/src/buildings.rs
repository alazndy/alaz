use crate::map::ItemKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingKind {
    Campfire,
    Shelter,
    CharcoalKiln,
    Furnace,
    Depot,
    Workshop,
    Workbench,
    WaterWheel,
    /// Broken ground, sown with seed: grows grain on its own over time.
    Field,
    /// An upright frame with clay weights holding the warp: fibre in, cloth out.
    Loom,
    /// A fenced pen: two animals of a kind kept in it, and fed, bring young.
    Pen,
    /// A hole dug with a shovel, raw clay inside, straw and logs over it: fires pottery slowly and hot.
    PitKiln,
    /// A flat block of stone to forge on.
    Anvil,
    /// A rotary quern: turned by hand by someone standing at it, or by an animal walking an animal gin beside it.
    HandMill,
    /// A sweep an ox or a horse walks round: drives the mill beside it.
    AnimalGin,
    /// A clay shaft furnace: iron ore and charcoal become sponge iron, but only with a working bellows beside it.
    Bloomery,
    /// Leather bellows: worked by hand, by an animal at a gin, or by a water wheel.
    Bellows,
    /// A water-driven hammer: beats sponge iron into wrought iron by itself.
    TripHammer,
    /// An anvil of wrought iron: iron tools are forged on it.
    IronAnvil,
    /// A tall furnace of fire brick blown by bellows: runs molten pig iron.
    BlastFurnace,
    /// A steel anvil.
    SteelAnvil,
    /// A sail mill: like a water wheel, a source of power for machines within 8 m.
    Windmill,
    /// A powered belt: carries what the stations within 3 m have made to the nearest depot within 12 m.
    Conveyor,
    /// A boiler and a cylinder: while it burns coal and water it drives machines within 10 m.
    SteamEngine,
    /// A piston pump: worked by power, it keeps a mine dry.
    Pump,
    /// Powered rolls: copper ingots drawn to wire.
    RollingMill,
    /// Coils turning in a magnet: mechanical power in, electricity out to the grid.
    Dynamo,
    /// A pole carrying wire: links grid buildings within 12 m.
    Pole,
    /// An electric motor: on a live grid it drives machines within 8 m.
    ElectricMotor,
    /// An electric lamp: lit while the grid is live.
    ElectricLamp,
    /// A computer: on a live grid it opens the design table, where new rules are written, and the way to a new world.
    Computer,
    /// What is left of a building of an earlier world: tumbled stone, of no use but to wonder at.
    Ruin,
    /// A cool dug store: keeps anything like a depot, and food spoils five times slower in it.
    Cellar,
    /// A shaft sunk on a hidden deposit: brings up its ore (or oil).
    MineShaft,
    // --- Industry (wave 1) ---
    /// A brick oven that bakes coal to coke and gives up its tar.
    CokeOven,
    /// A shaft kiln: limestone burns to lime, and lime slakes to mortar.
    LimeKiln,
    /// A hot rotating kiln: lime and clay burn to cement; cement makes concrete.
    CementKiln,
    /// A kiln for bricks by the batch: common brick from clay, fire brick from fire clay.
    BrickKiln,
    /// A reverberatory furnace: pig iron stirred in the flame becomes wrought iron, a batch at a time.
    PuddlingFurnace,
    /// A tilting vessel: a strong blast through molten pig iron blows it to steel.
    Bessemer,
    /// A small shaft furnace for remelting pig iron: pours cast parts into sand moulds.
    Cupola,
    /// A bench with a sand box: packs the sand moulds the cupola pours into.
    MoldingBench,
    /// A powered lathe: shafts, bolts and nuts, pistons.
    Lathe,
    /// A powered drill and miller: precision gears, cylinder blocks.
    DrillMill,
    /// A powered press: sheet, and parts stamped from it.
    Press,
    /// A powered saw frame: one log, four planks.
    Sawmill,
    /// A wheel and a spindle: fibre spun to thread.
    SpinningWheel,
    /// A powered loom: thread to cloth by the bolt.
    PowerLoom,
    /// A great fired boiler: steam for steam engines around it, and for the machines near it.
    Boiler,
    /// A steam-driven hammer: the heavy forgings, great shafts and rails.
    SteamHammer,
    /// Powered rolls: steel to rail and girder.
    RollingLine,
    // --- Life and craft (wave 2) ---
    /// A kick wheel and a lump of clay: pots thrown by the batch, and lamps turned from them.
    PotterWheel,
    /// A wedge press: seed squeezed to oil.
    OilPress,
    /// Pits of bark liquor: hide soaked to leather, leather cut to belts.
    Tannery,
    /// A frame in the sun and the wind: berries, meat and fish dried to keep. It stands still in the rain.
    DryingRack,
    /// A low stone hut over a smouldering fire: meat and fish smoked to keep.
    Smokehouse,
    /// A stone-lined shaft with a windlass: a pot goes down empty and comes up full of water.
    Well,
    /// A hollowed stone and a heavy pestle: grain pounded to bulgur. Worked by a hand, an animal or a stream.
    StoneMortar,
    /// A wheel in a stream turning millstones: four times the hand mill's flour, and no one has to stand at it.
    WaterMill,
    /// A trip hammer worked by power: sheet and wire beaten out of iron and copper.
    ForgeHammer,
    /// A tub and a wooden mallet: fibre beaten with water into felt.
    FeltMallet,
    /// A long walk and a hook on a wheel: fibre twisted into rope.
    Ropewalk,
    /// A tall jib on a wooden mast: heavy buildings within reach go up for less.
    Crane,
    /// A furnace with a glory hole: bottles, lenses and window glass blown and ground.
    GlassFurnace,
    /// A vat of pulp and a mould: paper lifted sheet by sheet.
    PaperVat,
    /// A screw press with movable type: paper and ink made into books.
    PrintingPress,
    /// A powered roller that pulls the seed from the cotton boll.
    CottonGin,
    /// A powered frame of many spindles: a dozen threads where the wheel draws one.
    SpinningJenny,
}

impl BuildingKind {
    pub const ALL: [BuildingKind; 68] = [
        BuildingKind::Campfire,
        BuildingKind::Shelter,
        BuildingKind::CharcoalKiln,
        BuildingKind::Furnace,
        BuildingKind::Depot,
        BuildingKind::Workshop,
        BuildingKind::Workbench,
        BuildingKind::WaterWheel,
        BuildingKind::Field,
        BuildingKind::Loom,
        BuildingKind::Pen,
        BuildingKind::PitKiln,
        BuildingKind::Anvil,
        BuildingKind::HandMill,
        BuildingKind::AnimalGin,
        BuildingKind::Bloomery,
        BuildingKind::Bellows,
        BuildingKind::TripHammer,
        BuildingKind::IronAnvil,
        BuildingKind::BlastFurnace,
        BuildingKind::SteelAnvil,
        BuildingKind::Windmill,
        BuildingKind::Conveyor,
        BuildingKind::SteamEngine,
        BuildingKind::Pump,
        BuildingKind::RollingMill,
        BuildingKind::Dynamo,
        BuildingKind::Pole,
        BuildingKind::ElectricMotor,
        BuildingKind::ElectricLamp,
        BuildingKind::Computer,
        BuildingKind::Ruin,
        BuildingKind::Cellar,
        BuildingKind::MineShaft,
        BuildingKind::CokeOven,
        BuildingKind::LimeKiln,
        BuildingKind::CementKiln,
        BuildingKind::BrickKiln,
        BuildingKind::PuddlingFurnace,
        BuildingKind::Bessemer,
        BuildingKind::Cupola,
        BuildingKind::MoldingBench,
        BuildingKind::Lathe,
        BuildingKind::DrillMill,
        BuildingKind::Press,
        BuildingKind::Sawmill,
        BuildingKind::SpinningWheel,
        BuildingKind::PowerLoom,
        BuildingKind::Boiler,
        BuildingKind::SteamHammer,
        BuildingKind::RollingLine,
        BuildingKind::PotterWheel,
        BuildingKind::OilPress,
        BuildingKind::Tannery,
        BuildingKind::DryingRack,
        BuildingKind::Smokehouse,
        BuildingKind::Well,
        BuildingKind::StoneMortar,
        BuildingKind::WaterMill,
        BuildingKind::ForgeHammer,
        BuildingKind::FeltMallet,
        BuildingKind::Ropewalk,
        BuildingKind::Crane,
        BuildingKind::GlassFurnace,
        BuildingKind::PaperVat,
        BuildingKind::PrintingPress,
        BuildingKind::CottonGin,
        BuildingKind::SpinningJenny,
    ];

    /// Part of the electric grid (generates, carries or uses electricity).
    pub fn on_grid(self) -> bool {
        matches!(self, BuildingKind::Dynamo | BuildingKind::Pole | BuildingKind::ElectricMotor | BuildingKind::ElectricLamp | BuildingKind::Computer)
    }

    /// Machines that do nothing without mechanical power (a wheel, wind, steam or a motor within reach, or an animal at a gin).
    pub fn needs_power(self) -> bool {
        matches!(self, BuildingKind::Lathe | BuildingKind::DrillMill | BuildingKind::Press | BuildingKind::Sawmill | BuildingKind::PowerLoom | BuildingKind::RollingLine | BuildingKind::ForgeHammer | BuildingKind::CottonGin | BuildingKind::SpinningJenny)
    }

    /// What the machine has to have around it to work, beyond its inputs.
    pub fn requirement(self) -> Option<Requirement> {
        match self {
            k if k.needs_power() => Some(Requirement::Power),
            BuildingKind::SteamHammer => Some(Requirement::Steam),
            BuildingKind::Bessemer => Some(Requirement::StrongBlast),
            BuildingKind::Bloomery | BuildingKind::BlastFurnace | BuildingKind::Cupola => Some(Requirement::Blast),
            _ => None,
        }
    }

    /// Burns fuel (wood) over time and warms people nearby while lit.
    pub fn burns(self) -> bool {
        matches!(self, BuildingKind::Campfire)
    }
}

/// What a machine needs around it, besides what it is loaded with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    /// Mechanical power within reach.
    Power,
    /// A steaming boiler or engine within 8 m.
    Steam,
    /// A bellows within 3 m, worked by anything (a hand will do).
    Blast,
    /// A bellows within 3 m driven by mechanical power: the blast a converter needs.
    StrongBlast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// Index into `rules().recipes`.
    pub recipe: u16,
    pub remaining: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub kind: BuildingKind,
    pub x: i64,
    pub y: i64,
    /// Remaining burn ticks (campfire only).
    pub fuel: u32,
    pub job: Option<Job>,
    /// Delivered ingredients waiting to be used (stations only).
    #[serde(with = "crate::items_serde")]
    pub input: [u16; ItemKind::COUNT],
    /// Finished goods waiting to be collected (a depot keeps everything here).
    #[serde(with = "crate::items_serde")]
    pub output: [u16; ItemKind::COUNT],
    /// Recipe (index) to start by itself whenever the input buffer allows.
    pub auto: Option<u16>,
    /// Temperature, °C (heat sources and kilns). Saved in the save's extension block, not in the core.
    #[serde(skip)]
    pub temp: i16,
    /// Fields: how much the soil still gives, permille (tired by harvests, restored by rest and manure).
    #[serde(skip, default = "full_soil")]
    pub fertility: u16,
    /// Which way it faces, in quarter turns (0..3). Saved in the extension block.
    #[serde(skip)]
    pub facing: u8,
}

fn full_soil() -> u16 {
    1000
}

impl Building {
    /// A fresh building of this kind at (x, y) mm: no fuel, nothing inside, soil at full strength.
    pub fn new(kind: BuildingKind, x: i64, y: i64) -> Self {
        Self { kind, x, y, fuel: 0, job: None, input: [0; ItemKind::COUNT], output: [0; ItemKind::COUNT], auto: None, temp: 0, fertility: full_soil(), facing: 0 }
    }
}

impl Building {
    /// A steam engine or a boiler with fire under it and water boiling: it drives machines nearby.
    pub fn steaming(&self) -> bool {
        matches!(self.kind, BuildingKind::SteamEngine | BuildingKind::Boiler) && self.job.is_some() && self.temp >= 120
    }

    pub fn output_total(&self) -> u16 {
        self.output.iter().sum()
    }

    pub fn input_total(&self) -> u16 {
        self.input.iter().sum()
    }

    /// Lit campfire or a station that is currently working.
    pub fn active(&self) -> bool {
        if self.kind.burns() { self.fuel > 0 } else { self.job.is_some() }
    }
}
