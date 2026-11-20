//! Seeded cave density in the surface terrain's world coordinates.

use crate::{map::elevation_at_mm, rng::value_noise_3d};

mod navigation;
pub use navigation::{CaveEntrance, CaveMouth, CaveTile};
pub(crate) use navigation::{CaveNav, SIDES, cave_mouth};
mod small_world;
pub(crate) use small_world::SmallWorldCaves;

const BEDROCK_Z_MM: i64 = -96_000;

/// Absolute world position in millimetres. X and Y are horizontal, and Z grows upwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldPoint {
    pub x_mm: i64,
    pub y_mm: i64,
    pub z_mm: i64,
}

/// Geometry before fluids and player edits are applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    OpenAir,
    Rock,
    CaveAir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DensitySample {
    /// Positive inside the ground envelope, zero or negative on or above its surface.
    pub ground: i32,
    /// Positive where ground remains solid, zero or negative where a cave cuts through it.
    /// The magnitude is a noise value, not a distance in millimetres.
    pub cave: i32,
}

impl DensitySample {
    pub fn material(self) -> Material {
        if self.ground <= 0 {
            Material::OpenAir
        } else if self.cave <= 0 {
            Material::CaveAir
        } else {
            Material::Rock
        }
    }
}

/// A world-specific field. Queries for the same point must agree across chunk boundaries
/// and be independent of the order in which chunks are sampled.
pub trait DensityField {
    fn sample(&self, point: WorldPoint) -> DensitySample;
}

#[derive(Clone, Copy, Debug)]
pub struct CaveTerrain {
    seed: u64,
    noise_seed: u64,
    roof_weight: i32,
}

impl CaveTerrain {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            noise_seed: seed,
            roof_weight: 2,
        }
    }

    fn with_noise_seed(seed: u64, noise_seed: u64, thin_roof: bool) -> Self {
        Self {
            seed,
            noise_seed,
            roof_weight: if thin_roof { 0 } else { 2 },
        }
    }

    /// Reuse the surface height and roof bias when sampling several heights in one column.
    pub fn column(&self, x_mm: i64, y_mm: i64) -> CaveColumn {
        let hill_cover = value_noise_3d(
            self.noise_seed ^ 0xC41C,
            [x_mm, y_mm, 0],
            [72_000, 72_000, 1],
        );
        CaveColumn {
            seed: self.noise_seed,
            x_mm,
            y_mm,
            surface_mm: elevation_at_mm(self.seed, x_mm, y_mm),
            roof_bias: ((hill_cover + 220) * self.roof_weight / 3).clamp(0, 480),
        }
    }
}

impl DensityField for CaveTerrain {
    fn sample(&self, point: WorldPoint) -> DensitySample {
        self.column(point.x_mm, point.y_mm).sample(point.z_mm)
    }
}

pub struct CaveColumn {
    seed: u64,
    x_mm: i64,
    y_mm: i64,
    surface_mm: i32,
    roof_bias: i32,
}

impl CaveColumn {
    pub fn sample(&self, z_mm: i64) -> DensitySample {
        let ground = i64::from(self.surface_mm)
            .saturating_sub(z_mm)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let p = [self.x_mm, self.y_mm, z_mm];
        let noise = |salt, spacing| value_noise_3d(self.seed ^ salt, p, spacing);
        // Two zero surfaces intersect along curving lines. Open a tube around each line.
        let a = noise(0xC400, [58_000, 43_000, 29_000]);
        let b = noise(0xC401, [47_000, 61_000, 24_000]);
        let pockets = noise(0xC402, [70_000, 65_000, 42_000]);
        let wall = noise(0xC403, [13_000, 11_000, 9_000]);
        // Caverns grow out of the same tubes. Taper the widening near thin roofs.
        let cover = ground.clamp(0, 8_000);
        let widening = (pockets - 90).clamp(0, 560) * 3 / 5 * (cover + 2_000) / 10_000;
        let main = radial(a, b) - (124 + widening + wall / 32);

        // The branch follows the first surface and peels away from the second one.
        // Keep branch growth near the main passage so their intersections can be explored.
        let branch_curve = noise(0xC404, [32_000, 38_000, 20_000]);
        let spur = (radial(a + wall / 5, branch_curve) - 66).max(b.abs() - 490);
        let shallow_bias = (8_000 - cover) * self.roof_bias / 8_000;
        let bedrock = (BEDROCK_Z_MM + 3_000).saturating_sub(z_mm) / 8;
        let cave = i64::from(main.min(spur) + shallow_bias)
            .max(bedrock)
            .min(i64::from(i32::MAX)) as i32;
        DensitySample { ground, cave }
    }
}

fn radial(a: i32, b: i32) -> i32 {
    let (a, b) = (a.abs(), b.abs());
    a.max(b) + a.min(b) * 3 / 8
}

#[cfg(test)]
mod tests;
