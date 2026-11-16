//! Everything that turns the world into triangles: the ground, the scenery, buildings and living bodies.
//! Builders push into a `Mesh`, which carries the settings they used to keep in thread-locals.

use crate::assets;
use crate::sprites::{self, Art, Spr};
use crate::Camera;
use alaz_sim::{AgentKind, BuildingKind, NodeKind, Terrain, World};
use bytemuck::{Pod, Zeroable};

mod bodies;
mod buildings;
mod common;
mod creatures;
mod decor;
mod human;
mod machines;
mod nodes;
mod shapes;
mod terrain;
mod water;

pub use common::ground_height;
pub use nodes::node_bounds;
pub(crate) use bodies::*;
pub(crate) use buildings::*;
pub(crate) use common::*;
pub(crate) use creatures::*;
pub(crate) use decor::*;
pub(crate) use human::*;
pub(crate) use machines::*;
pub use human::{look, outfit_bits, Look, OUT_APRON, OUT_BOOTS, OUT_CLOAK, OUT_FELT, OUT_HELM, OUT_HOOD, OUT_LEATHER, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
pub(crate) use nodes::*;
pub(crate) use terrain::*;
pub(crate) use water::*;
