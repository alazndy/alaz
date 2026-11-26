//! Everything that turns the world into triangles: the ground, the scenery, buildings and living bodies.
//! Builders push into a `Mesh`, which carries the settings they used to keep in thread-locals.

use crate::assets;
use crate::sprites::{self, Art, Spr};
use crate::Camera;
use alaz_sim::{AgentKind, BuildingKind, NodeKind, Terrain, World};
use bytemuck::{Pod, Zeroable};

/// Sines, exponents and the like for the mesh builders, from pure Rust `libm` instead of the system's maths library.
/// Each system rounds the last bit of these its own way, so a mesh built with the `std` methods differs by a bit
/// between Linux, Windows and the development machine; built with these it is the same everywhere (the
/// `mesh_hashes` test holds on every system). Named with a `d` so they never resolve to the `std` methods.
pub(crate) trait DetMath: Sized + Copy {
    fn dsin(self) -> Self;
    fn dcos(self) -> Self;
    fn dsin_cos(self) -> (Self, Self) {
        (self.dsin(), self.dcos())
    }
    fn dexp(self) -> Self;
    fn dpowf(self, e: Self) -> Self;
    fn datan(self) -> Self;
    fn datan2(self, x: Self) -> Self;
    fn dhypot(self, o: Self) -> Self;
}
impl DetMath for f32 {
    fn dsin(self) -> f32 {
        libm::sinf(self)
    }
    fn dcos(self) -> f32 {
        libm::cosf(self)
    }
    fn dexp(self) -> f32 {
        libm::expf(self)
    }
    fn dpowf(self, e: f32) -> f32 {
        libm::powf(self, e)
    }
    fn datan(self) -> f32 {
        libm::atanf(self)
    }
    fn datan2(self, x: f32) -> f32 {
        libm::atan2f(self, x)
    }
    fn dhypot(self, o: f32) -> f32 {
        libm::hypotf(self, o)
    }
}
impl DetMath for f64 {
    fn dsin(self) -> f64 {
        libm::sin(self)
    }
    fn dcos(self) -> f64 {
        libm::cos(self)
    }
    fn dexp(self) -> f64 {
        libm::exp(self)
    }
    fn dpowf(self, e: f64) -> f64 {
        libm::pow(self, e)
    }
    fn datan(self) -> f64 {
        libm::atan(self)
    }
    fn datan2(self, x: f64) -> f64 {
        libm::atan2(self, x)
    }
    fn dhypot(self, o: f64) -> f64 {
        libm::hypot(self, o)
    }
}

mod bodies;
mod buildings;
mod caves;
mod common;
mod creatures;
mod decor;
mod evo;
mod human;
mod machines;
mod nodes;
mod shapes;
mod terrain;
mod water;

pub use common::ground_height;
pub use evo::{evo_pos, evo_stage, evolve_bump, smooth, Evo, EVOLVE_SECS, EVO_STAGES, EVO_STARTS};
pub use nodes::node_bounds;
pub(crate) use bodies::*;
pub(crate) use buildings::*;
pub(crate) use caves::*;
pub(crate) use common::*;
pub(crate) use creatures::*;
pub(crate) use decor::*;
pub(crate) use human::*;
pub(crate) use machines::*;
pub use human::{look, outfit_bits, Look, OUT_APRON, OUT_BOOTS, OUT_CLOAK, OUT_FELT, OUT_FRAME_PACK, OUT_HELM, OUT_HOOD, OUT_LEATHER, OUT_PACK, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
pub(crate) use nodes::*;
pub(crate) use terrain::*;
pub(crate) use water::*;
