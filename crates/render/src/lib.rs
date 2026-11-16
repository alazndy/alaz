//! Render layer: reads sim state, never mutates it.
//!
//! Pixel-art renderer: the world is a fixed top-down view drawn from a painted sprite atlas (`sprites.rs`) into a
//! small picture of whole texels (16 per metre), which is then enlarged by a whole number of screen pixels.

pub mod assets;
mod bake;
mod camera;
mod export;
mod hotreload;
mod mesh;
mod pipelines;
mod prof;
mod quality;
mod renderer;
pub mod sprites;
#[cfg(test)]
mod tests;

pub use camera::Camera;
pub use export::export_world_assets;
pub use egui;
pub use glam;
pub use quality::{PixelDetail, Quality, ShadowQuality};
pub use mesh::{evo_pos, evo_stage, smooth, ground_height, look, node_bounds, outfit_bits, Evo, Look, EVO_STAGES, EVO_STARTS};
pub use mesh::{OUT_APRON, OUT_BOOTS, OUT_CLOAK, OUT_FELT, OUT_HELM, OUT_HOOD, OUT_LEATHER, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
pub use renderer::{RenderStats, Renderer};

use alaz_sim::BuildingKind;

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Depth format of the sun's shadow maps: 16 bits are plenty over their 500 m range (a step is under a centimetre,
/// the shader's bias is 40 cm) and halve what the per-frame copy of the static map moves.
pub(crate) const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;

/// Placement preview: footprint + translucent-looking tinted model at a ground position (meters).
#[derive(Clone, Copy, Debug)]
pub struct Ghost {
    pub x_m: f32,
    pub z_m: f32,
    pub kind: BuildingKind,
    pub valid: bool,
    /// Quarter turns, as it will face once placed.
    pub facing: u8,
}

pub(crate) const NIGHT_TINT: [f32; 3] = [0.30, 0.38, 0.62];

/// Direction toward the light (unit, render axes: +x east, -z north, +y up) and how strong its shadows are.
/// The sun rises in the east, crosses the southern sky and sets in the west, so shadows swing from west through
/// north to east over the day: long at morning and evening, short at noon. Summer runs it high, winter low.
/// At night a faint moon casts from high in the south; heavy weather softens it all.
pub fn sun_of(tick: u64, weather: f32, season_days: u64) -> ([f32; 3], f32) {
    use alaz_sim::time::{minute_of_day, TICKS_PER_DAY};
    use std::f32::consts::TAU;
    let m = minute_of_day(tick) as f32;
    // the season sets how high the sun climbs: 0 at mid-spring, highest at mid-summer, lowest at mid-winter
    let year = (TICKS_PER_DAY * season_days.max(1) * 4) as f32;
    let y = (tick as f32 % year) / year;
    let decl = (23.4f32).to_radians() * (TAU * (y - 0.125)).sin();
    let lat = (40.0f32).to_radians();
    let a = (m - 330.0) / (1230.0 - 330.0);
    let (dir, k) = if (0.0..=1.0).contains(&a) {
        // the game's daylight (05:30-20:30) spans the sun's path from horizon to horizon
        let h0 = (-lat.tan() * decl.tan()).clamp(-1.0, 1.0).acos();
        let h = (a * 2.0 - 1.0) * h0;
        let east = -decl.cos() * h.sin();
        let north = decl.sin() * lat.cos() - decl.cos() * h.cos() * lat.sin();
        let up = decl.sin() * lat.sin() + decl.cos() * h.cos() * lat.cos();
        let rise = (a / 0.06).min(1.0).min((1.0 - a) / 0.06);
        // never quite flat on the horizon, or the shadows would stretch forever
        ([east, up.max(0.16), -north], 0.8 * rise)
    } else {
        ([0.15, 0.85, 0.45], 0.18)
    };
    let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    ([dir[0] / l, dir[1] / l, dir[2] / l], k * (1.0 - 0.6 * weather))
}
