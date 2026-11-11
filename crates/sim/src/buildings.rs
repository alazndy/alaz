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
}

impl BuildingKind {
    pub const ALL: [BuildingKind; 34] = [
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
    ];

    /// Part of the electric grid (generates, carries or uses electricity).
    pub fn on_grid(self) -> bool {
        matches!(self, BuildingKind::Dynamo | BuildingKind::Pole | BuildingKind::ElectricMotor | BuildingKind::ElectricLamp | BuildingKind::Computer)
    }

    /// Burns fuel (wood) over time and warms people nearby while lit.
    pub fn burns(self) -> bool {
        matches!(self, BuildingKind::Campfire)
    }
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
}

fn full_soil() -> u16 {
    1000
}

impl Building {
    /// A fresh building of this kind at (x, y) mm: no fuel, nothing inside, soil at full strength.
    pub fn new(kind: BuildingKind, x: i64, y: i64) -> Self {
        Self { kind, x, y, fuel: 0, job: None, input: [0; ItemKind::COUNT], output: [0; ItemKind::COUNT], auto: None, temp: 0, fertility: full_soil() }
    }
}

impl Building {
    /// A steam engine with fire under the boiler and water boiling: it drives machines nearby.
    pub fn steaming(&self) -> bool {
        self.kind == BuildingKind::SteamEngine && self.job.is_some() && self.temp >= 120
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
