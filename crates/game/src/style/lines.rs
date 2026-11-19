//! Info lines: what the info panel says, as data. `inv::info` produces them; the renderer draws them.

use alaz_sim::ItemKind;

use super::icons::Icon;

pub(crate) enum Picto {
    Item(ItemKind),
    Icon(Icon),
    Flames { lit: u8, of: u8 },
    Glasses { lit: u8, of: u8 },
}

pub(crate) enum Chip {
    Item(ItemKind, u16),
    HeatC { need: i16, now: Option<i16> },
    HeatWord(&'static str),
    Secs(u32),
    PerMin(f32),
}

pub(crate) enum Line {
    Title(String),
    Label(&'static str),
    Pictos(Vec<Picto>),
    Chips(Vec<Chip>),
    Text(String),
    Mono(Vec<String>),
    Warn(String),
}
