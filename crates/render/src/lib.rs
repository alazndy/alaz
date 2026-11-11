//! Render layer: reads sim state, never mutates it.
//!
//! Pixel-art renderer: the world is a fixed top-down view drawn from a painted sprite atlas (`sprites.rs`) into a
//! small picture of whole texels (16 per metre), which is then enlarged by a whole number of screen pixels.

mod camera;
mod shapes;
pub mod sprites;
pub use camera::Camera;
pub use egui;
pub use glam;

use alaz_sim::time::daylight_permille;
use alaz_sim::{rules, AgentKind, BuildingKind, NodeKind, Terrain, World};
use bytemuck::{Pod, Zeroable};
use sprites::{Art, Spr};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::event::WindowEvent;
use winit::window::Window;

/// kind: 0 solid overlay, 1 ground tile, 2 sprite, 3 decal. `off` is a whole-texel offset from the anchor (y up).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    color: [f32; 3],
    glow: f32,
    uv: [f32; 2],
    off: [f32; 2],
    kind: f32,
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Texels on a side of the sun's shadow map.
const SHADOW_RES: u32 = 2048;
/// Placement preview: footprint + translucent-looking tinted model at a ground position (meters).
#[derive(Clone, Copy, Debug)]
pub struct Ghost {
    pub x_m: f32,
    pub z_m: f32,
    pub kind: BuildingKind,
    pub valid: bool,
}

const NIGHT_TINT: [f32; 3] = [0.30, 0.38, 0.62];

const CHUNK_TILES: i32 = 32;
const MAX_CACHED_CHUNKS: usize = 600;
/// Milliseconds per frame spent building chunk meshes (at least one chunk is always built).
const BUILD_BUDGET_MS: f64 = 2.5;

struct Chunk {
    terrain: Option<(wgpu::Buffer, u32)>,
    nodes: Option<(wgpu::Buffer, u32)>,
    nodes_version: (u64, u64),
    /// What had been seen when the nodes were built (unknown things look plain).
    seen: u128,
    level: u8,
    used: u64,
}

/// Cheap counters for performance testing (`ALAZ_PERF`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub chunks_drawn: u32,
    pub vertices: u64,
    pub chunks_built: u64,
    pub build_ms: f64,
}

/// (x0, z0, x1, z1) with a margin for tall objects, used to skip off-screen creatures and buildings.
fn visible_chunks(cam: &Camera, aspect: f32, world: &World) -> (i32, i32, i32, i32, (f32, f32, f32, f32)) {
    let (mut x0, mut z0, mut x1, mut z1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut ok = true;
    for (nx, ny) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        match cam.ground_at(nx, ny, aspect) {
            Some(p) if p.x.is_finite() && p.z.is_finite() => {
                x0 = x0.min(p.x);
                x1 = x1.max(p.x);
                z0 = z0.min(p.z);
                z1 = z1.max(p.z);
            }
            _ => ok = false,
        }
    }
    if !ok || x1 - x0 > 600.0 || z1 - z0 > 600.0 {
        (x0, z0, x1, z1) = (cam.target.x - 150.0, cam.target.z - 150.0, cam.target.x + 150.0, cam.target.z + 150.0);
    }
    let m = 10.0;
    let view = (x0 - m, z0 - m, x1 + m, z1 + m);
    let c = |v: f32, max_tiles: i32| ((v.floor() as i32).div_euclid(CHUNK_TILES)).clamp(0, (max_tiles - 1) / CHUNK_TILES);
    (c(view.0, world.map.w), c(view.1, world.map.h), c(view.2, world.map.w), c(view.3, world.map.h), view)
}

/// Fullscreen pass that enlarges the small picture by a whole number of pixels and grades it.
struct PixelPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// 0 = plain, 1 = film grade.
    cinema: f32,
}

/// Draws ink outlines from the scene's depth, at the small picture's resolution.
struct OutlinePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
}

pub struct Renderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    scene: wgpu::TextureView,
    inked: wgpu::TextureView,
    outline: OutlinePass,
    pixel: PixelPass,
    tri_pipeline: wgpu::RenderPipeline,
    /// The player's body marked in the picture's alpha (all of it, then the hidden part), for the outline pass to
    /// draw a bright line round whatever of it stands behind something.
    mark_pipeline: wgpu::RenderPipeline,
    hidden_pipeline: wgpu::RenderPipeline,
    chunks: HashMap<(i32, i32), Chunk>,
    frame_no: u64,
    stats: RenderStats,
    globals: wgpu::Buffer,
    height_tex: wgpu::Texture,
    /// Per tile of the height window: its ground picture (atlas texel corner), ground kind and shade, so the shader
    /// can draw a neighbour's ground across a ragged boundary instead of along tile edges.
    tile_tex: wgpu::Texture,
    /// (seed, tile corner of the window's first texel) of what the height texture holds now
    height_at: Option<(u64, i32, i32)>,
    weather_kind: f32,
    weather_amount: f32,
    wind: f32,
    bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    shadow_bind_group: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,
    art: Art,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    /// Per-creature walking animation state, keyed by agent id.
    anim: HashMap<u32, Walk>,
    /// The same for wild animals, keyed by their wild id.
    wild_anim: HashMap<u32, Walk>,
    last_frame: std::time::Instant,
    tick_seen: u64,
    tick_at: std::time::Instant,
    tick_gap: f32,
    /// Tile of the resource node under the pointer: drawn with a bright rim so it is clear what can be taken.
    pub hover_node: Option<(i32, i32)>,
    /// How far the game is between the last simulated tick and the next (0..1), when the caller knows it
    /// exactly; otherwise it is guessed from the time between ticks.
    pub sim_alpha: Option<f32>,
    /// The sun direction the shadows are cast along: it follows the sun in small steps, not every frame, so
    /// shadow edges hold still instead of shimmering as the sun moves.
    shadow_dir: [f32; 3],
    /// Where the cloud field has drifted to (it moves with the wind, so it is summed frame by frame).
    cloud_off: [f32; 2],
}

/// Render-side walking state: where a creature was, which way it faces, and how far into its stride it is.
#[derive(Clone, Copy)]
struct Walk {
    x: f32,
    z: f32,
    heading: f32,
    phase: f32,
    amp: f32,
    /// Health last frame, and how much of a flinch is left after a blow (1 to 0).
    hp: i32,
    hurt: f32,
    /// The two latest simulated positions and the tick of the newer one: drawing slides between them, so movement
    /// is smooth whatever the frame rate and however many ticks a frame runs.
    prev: [f32; 2],
    sim: [f32; 2],
    tick: u64,
    /// How far a foot swings forward and back (metres); the walk cycle advances so that a planted foot stays put.
    stride: f32,
    speed: f32,
    /// Crafting: the last strike count seen and seconds since the hand last came down.
    craft: u64,
    strike: f32,
    /// How fast the body is turning (radians a second): turns ease in and out, and the body leans into them.
    spin: f32,
}

impl Walk {
    fn at(x: f32, z: f32, heading: f32, hp: i32) -> Walk {
        Walk { x, z, heading, phase: 0.0, amp: 0.0, hp, hurt: 0.0, prev: [x, z], sim: [x, z], tick: 0, stride: 0.3, speed: 0.0, craft: 0, strike: 9.0, spin: 0.0 }
    }

    /// Where to draw it this frame: between the last two simulated positions.
    fn smooth(&mut self, x: f32, z: f32, tick: u64, alpha: f32) -> (f32, f32) {
        if tick != self.tick {
            let jump = (x - self.sim[0]).hypot(z - self.sim[1]);
            self.prev = if jump > 3.0 { [x, z] } else { self.sim };
            self.sim = [x, z];
            self.tick = tick;
        }
        (self.prev[0] + (self.sim[0] - self.prev[0]) * alpha, self.prev[1] + (self.sim[1] - self.prev[1]) * alpha)
    }
}

/// Foot swing (metres) for a four-legged animal with legs `leg_h` long.
fn quad_stride(leg_h: f32) -> f32 {
    0.55 * leg_h + 0.04
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("request device");

        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface unsupported");
        // ALAZ_NOVSYNC lifts the 60 fps cap so performance tests measure real frame cost
        config.present_mode =
            if std::env::var_os("ALAZ_NOVSYNC").is_some() { wgpu::PresentMode::AutoNoVsync } else { wgpu::PresentMode::AutoVsync };
        surface.configure(&device, &config);

        // --- the painted atlas
        let (art, atlas) = sprites::build();
        let atlas_tex = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("atlas"),
                size: wgpu::Extent3d { width: sprites::ATLAS, height: sprites::ATLAS, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::default(),
            &atlas.data,
        );
        let atlas_view = atlas_tex.create_view(&Default::default());

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 656,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let height_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("height window"),
            size: wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let height_view = height_tex.create_view(&Default::default());
        let tile_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile window"),
            size: wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tile_view = tile_tex.create_view(&Default::default());
        // sun shadows: depth seen from the sun, over the part of the world around the camera
        let shadow_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("sun shadow"),
                size: wgpu::Extent3d { width: SHADOW_RES, height: SHADOW_RES, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        // the shadow pass binds everything but the shadow map it is drawing into
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow pass"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&height_view) },
            ],
        });
        let bgl_main = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("main"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl_main,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&height_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&tile_view) },
            ],
        });
        let vbuf = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32, 3 => Float32x2, 4 => Float32x2, 5 => Float32],
        };
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadow"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_shadow"), compilation_options: Default::default(), buffers: &[Some(vbuf.clone())] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_shadow"), compilation_options: Default::default(), targets: &[] }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl_main)], immediate_size: 0 });
        let tri_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32, 3 => Float32x2, 4 => Float32x2, 5 => Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        // the player drawn again into the picture's alpha only: where its body is, and where something hides it
        let mark_pipeline = Self::make_mark_pipeline(&device, &layout, &shader, config.format, "vs", "fs_mark", wgpu::CompareFunction::Always);
        let hidden_pipeline = Self::make_mark_pipeline(&device, &layout, &shader, config.format, "vs_hidden", "fs_hidden", wgpu::CompareFunction::Greater);
        let (w, h) = (config.width, config.height);
        let depth = Self::make_depth(&device, w, h);
        let scene = Self::make_color(&device, config.format, w, h);
        let inked = Self::make_color(&device, config.format, w, h);
        let outline = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("outline"),
                source: wgpu::ShaderSource::Wgsl(include_str!("outline.wgsl").into()),
            });
            let tex = |binding, sample_type| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("outline"),
                entries: &[tex(0, wgpu::TextureSampleType::Float { filterable: false }), tex(1, wgpu::TextureSampleType::Depth)],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("outline"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(config.format.into())] }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let group = Self::make_outline_group(&device, &layout, &scene, &depth);
            OutlinePass { pipeline, layout, group }
        };
        let pixel = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pixel"),
                source: wgpu::ShaderSource::Wgsl(include_str!("pixel.wgsl").into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("pixel"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                ],
            });
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pixel uniform"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pixel"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(config.format.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let group = Self::make_pixel_group(&device, &layout, &inked, &uniform);
            let cinema = if std::env::var_os("ALAZ_NOCINEMA").is_some() { 0.0 } else { 1.0 };
            PixelPass { pipeline, layout, uniform, group, cinema }
        };

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());

        Self {
            window,
            surface,
            device,
            queue,
            config,
            depth,
            scene,
            inked,
            outline,
            pixel,
            tri_pipeline,
            mark_pipeline,
            hidden_pipeline,
            chunks: HashMap::new(),
            anim: HashMap::new(),
            wild_anim: HashMap::new(),
            sim_alpha: None,
            shadow_dir: [0.0, 1.0, 0.0],
            cloud_off: [0.0, 0.0],
            hover_node: None,
            last_frame: std::time::Instant::now(),
            tick_seen: 0,
            tick_at: std::time::Instant::now(),
            tick_gap: 1.0 / 60.0,
            frame_no: 0,
            stats: RenderStats::default(),
            globals,
            height_tex,
            tile_tex,
            height_at: None,
            weather_kind: 1.0,
            weather_amount: 0.0,
            wind: 0.0,
            bind_group,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline,
            art,
            egui_ctx,
            egui_state,
            egui_renderer,
        }
    }

    fn make_color(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    fn make_mark_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat, vs: &str, fs: &str, compare: wgpu::CompareFunction) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(fs),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32, 3 => Float32x2, 4 => Float32x2, 5 => Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALPHA })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    fn make_outline_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, scene: &wgpu::TextureView, depth: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(scene) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        })
    }

    fn make_pixel_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, scene: &wgpu::TextureView, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pixel"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(scene) },
                wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
            ],
        })
    }

    fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::make_depth(&self.device, w, h);
        self.scene = Self::make_color(&self.device, self.config.format, w, h);
        self.inked = Self::make_color(&self.device, self.config.format, w, h);
        self.outline.group = Self::make_outline_group(&self.device, &self.outline.layout, &self.scene, &self.depth);
        self.pixel.group = Self::make_pixel_group(&self.device, &self.pixel.layout, &self.inked, &self.pixel.uniform);
    }

    fn vertex_buffer(&self, v: &[Vertex]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(v), usage: wgpu::BufferUsages::VERTEX })
    }

    /// Drop cached scene buffers (call when the world is replaced).
    pub fn reset_scene(&mut self) {
        self.chunks.clear();
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Feed a window event to egui; true if the UI consumed it.
    pub fn on_window_event(&mut self, event: &WindowEvent) -> bool {
        self.egui_state.on_window_event(&self.window, event).consumed
    }

    /// True while the pointer is over (or dragging) an egui widget.
    /// Weather as it looks right now (it fades in and out): (rain 0..1, snow 0..1, wind 0..2).
    pub fn weather_now(&self) -> (f32, f32, f32) {
        let rain = if self.weather_kind < 1.5 { self.weather_amount } else { 0.0 };
        let snow = if self.weather_kind > 1.5 { self.weather_amount } else { 0.0 };
        (rain, snow, self.wind)
    }

    pub fn pointer_over_ui(&self) -> bool {
        self.egui_ctx.egui_wants_pointer_input() || self.egui_ctx.is_pointer_over_egui()
    }

    pub fn render(
        &mut self,
        world: &World,
        cam: &Camera,
        ghost: Option<Ghost>,
        me: alaz_sim::AgentId,
        routes: &[usize],
        mut hud: impl FnMut(&egui::Context),
    ) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                // the old frame must be gone before the surface is configured again, so skip this one
                drop(f);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let (win_w, win_h) = (self.config.width, self.config.height);
        let mut cam = *cam;
        cam.win = (win_w, win_h);
        let scale = cam.scale();
        let (sw, sh) = cam.tex_dims();

        // --- globals: view-projection from the texel-snapped camera + time-of-day light tint
        let l = daylight_permille(world.tick) as f32 / 1000.0;
        let t = ((1.0 - l) / 0.7).clamp(0.0, 1.0);
        // the season colours the whole picture a little: fresh spring, warm summer, rusty autumn, cold winter
        let season_tint: [f32; 3] = [[1.0, 1.03, 1.0], [1.06, 1.02, 0.95], [1.10, 0.97, 0.84], [0.94, 1.0, 1.10]][world.season() as usize];
        let tint: [f32; 3] = std::array::from_fn(|i| (1.0 + (NIGHT_TINT[i] - 1.0) * t) * season_tint[i]);
        let (snap_cam, off) = cam.snapped();
        let sky = match world.weather() {
            alaz_sim::time::Weather::Clear => 0.0,
            alaz_sim::time::Weather::Rain => 1.0,
            alaz_sim::time::Weather::Snow => 2.0,
        };
        // the weather fades in and out instead of switching
        let wtarget = if sky > 0.0 { 1.0 } else { 0.0 };
        self.weather_amount += (wtarget - self.weather_amount) * 0.01;
        if sky > 0.0 {
            self.weather_kind = sky;
        }
        // the ground-height window follows the camera in jumps of 64 tiles
        let (hx, hy) = (((cam.target.x as i32 - HWIN as i32 / 2) as f32 / 64.0).round() as i32 * 64, ((cam.target.z as i32 - HWIN as i32 / 2) as f32 / 64.0).round() as i32 * 64);
        if self.height_at != Some((world.seed, hx, hy)) {
            let shore = shore_field(world, hx, hy);
            let data: Vec<f32> = (0..HWIN as i32 * HWIN as i32)
                .flat_map(|i| {
                    let (tx, ty) = (hx + i % HWIN as i32, hy + i / HWIN as i32);
                    let (warm, wet) = alaz_sim::climate(world.seed, tx, ty);
                    [alaz_sim::elevation_mm(world.seed, tx, ty) as f32 / 1000.0, warm as f32 / 1000.0, wet as f32 / 1000.0, shore[i as usize]]
                })
                .collect();
            self.queue.write_texture(
                self.height_tex.as_image_copy(),
                bytemuck::cast_slice(&data),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(HWIN * 16), rows_per_image: Some(HWIN) },
                wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            );
            // and what ground each tile of the window wears
            let tiles: Vec<f32> = (0..HWIN as i32 * HWIN as i32)
                .flat_map(|i| {
                    let (tx, ty) = (hx + i % HWIN as i32, hy + i / HWIN as i32);
                    let t = world.map.terrain(tx, ty).unwrap_or(Terrain::Water);
                    let h = (alaz_sim::rng::hash2(world.seed ^ 0x7772, tx, ty) >> 20) as u32;
                    let s = tile_spr(&self.art, t, h);
                    let k = if t == Terrain::Water { 1.0 } else { tile_shade(world, tx, ty) };
                    [s.x as f32, s.y as f32, t as u8 as f32, k]
                })
                .collect();
            self.queue.write_texture(
                self.tile_tex.as_image_copy(),
                bytemuck::cast_slice(&tiles),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(HWIN * 16), rows_per_image: Some(HWIN) },
                wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            );
            self.height_at = Some((world.seed, hx, hy));
        }
        let mut g = [0f32; 164];
        g[..16].copy_from_slice(&snap_cam.view_proj(1.0).to_cols_array());
        // the sun crosses the sky from morning (east, +x) to evening (west); at night a weak moon casts from above
        let (sun_dir, sun_strength) = sun_of(world.tick, self.weather_amount, world.settings.season_days as u64);
        {
            let (_, _, _, _, vm) = visible_chunks(&cam, self.aspect(), world);
            let (cx, cz) = ((vm.0 + vm.2) * 0.5, (vm.1 + vm.3) * 0.5);
            // the box grows and shrinks in 8 m steps: a size changing every frame changes the shadow texel size
            let r = ((0.5 * ((vm.2 - vm.0).powi(2) + (vm.3 - vm.1).powi(2)).sqrt() / 8.0).ceil() * 8.0).clamp(16.0, 128.0);
            let center = glam::Vec3::new(cx, ground_height(world.seed, cx, cz), cz);
            let now_dir = glam::Vec3::from(sun_dir);
            if glam::Vec3::from(self.shadow_dir).dot(now_dir) < 0.99995 {
                self.shadow_dir = sun_dir;
            }
            let d = glam::Vec3::from(self.shadow_dir);
            let up = if d.y.abs() > 0.97 { glam::Vec3::Z } else { glam::Vec3::Y };
            // the sun's view is fixed in the world; only the box moves, in whole shadow texels, so the shadows do not
            // crawl as the camera slides
            let view = glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, -d, up);
            let texel = 2.0 * r / SHADOW_RES as f32;
            let lc = view.transform_point3(center);
            let (sx, sy) = ((lc.x / texel).round() * texel, (lc.y / texel).round() * texel);
            let proj = glam::camera::rh::proj::directx::orthographic(sx - r, sx + r, sy - r, sy + r, -lc.z - 250.0, -lc.z + 250.0);
            g[32..48].copy_from_slice(&(proj * view).to_cols_array());
        }
        g[48..51].copy_from_slice(&sun_dir);
        g[51] = sun_strength;
        // clouds: more of them in bad weather; they drift with the wind
        let clock = world.tick as f32 / 60.0;
        g[52] = 0.35 + 0.55 * self.weather_amount;
        // the one wind of the world: everything that moves in the air or on the water goes its way
        let (heading, strength) = alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64);
        let (wa, ws) = (heading as f32 / 1000.0, strength as f32 / 1000.0);
        WIND.with(|w| w.set([wa.cos() * ws, wa.sin() * ws, ws]));
        let drift = 0.004 + 0.02 * ws;
        let cdt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.cloud_off[0] -= wa.cos() * drift * cdt;
        self.cloud_off[1] -= wa.sin() * drift * cdt;
        g[53] = self.cloud_off[0];
        g[54] = self.cloud_off[1];
        g[55] = wa;
        // the swell's six wave trains: each one's phase at the height window's corner, in double precision
        {
            const DIRS: [f64; 6] = [0.0, 63.0, 118.0, 182.0, 238.0, 301.0];
            const LENS: [f64; 6] = [14.0, 11.0, 8.5, 6.5, 5.3, 3.1];
            let t = world.tick as f64 / 60.0 + self.sim_alpha.unwrap_or(0.0) as f64 / 60.0;
            let slots = [22, 23, 26, 27, 63, 67];
            for i in 0..6 {
                let a = DIRS[i].to_radians();
                let k = std::f64::consts::TAU / LENS[i];
                let w = (9.8 * k).sqrt();
                let ph = k * (a.cos() * hx as f64 + a.sin() * hy as f64) - w * t + i as f64 * 1.7;
                g[slots[i]] = ph.rem_euclid(std::f64::consts::TAU) as f32;
            }
        }
        let _ = clock;
        // the player's own body, so whatever stands between it and the camera can open up around it
        if let Some(a) = world.agents.iter().find(|a| a.id == me) {
            let (px, pz) = (a.x as f32 / 1000.0, a.y as f32 / 1000.0);
            g[56..60].copy_from_slice(&[px, ground_height(world.seed, px, pz) + 0.9, pz, 1.0]);
        }
        {
            let (p, y) = (cam.pitch_deg.to_radians(), cam.yaw_deg.to_radians());
            let f = -glam::Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos());
            let r = glam::Vec3::new(y.cos(), 0.0, -y.sin());
            let u = r.cross(f);
            g[60..63].copy_from_slice(&r.to_array());
            g[64..67].copy_from_slice(&u.to_array());
        }
        // fires light their surroundings: the eight burning ones nearest the camera (xyz, w = reach in metres)
        {
            let mut fires: Vec<(f32, [f32; 4])> = world
                .buildings
                .iter()
                .filter(|b| b.active())
                .filter_map(|b| {
                    let reach = match b.kind {
                        // a fire well fed throws its light further than one burning low
                        BuildingKind::Campfire => 5.0 + 4.0 * (b.fuel as f32 / alaz_sim::world::FIRE_FUEL_MAX as f32).min(1.0),
                        BuildingKind::CharcoalKiln | BuildingKind::Furnace => 5.0,
                        _ => return None,
                    };
                    let (x, z) = (b.x as f32 / 1000.0, b.y as f32 / 1000.0);
                    let d = (x - cam.target.x).powi(2) + (z - cam.target.z).powi(2);
                    Some((d, [x, ground_height(world.seed, x, z) + 0.45, z, reach]))
                })
                .collect();
            fires.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (i, (_, f)) in fires.iter().take(8).enumerate() {
                g[68 + i * 4..72 + i * 4].copy_from_slice(f);
            }
        }
        g[16..19].copy_from_slice(&tint);
        g[19] = world.perception_permille() as f32 / 1000.0;
        g[20] = cam.target.x;
        g[21] = cam.target.z;
        g[24] = hx as f32;
        g[25] = hy as f32;
        let storm = if self.weather_kind < 1.5 { self.weather_amount } else { 0.0 };
        let snowing = if self.weather_kind > 1.5 { self.weather_amount } else { 0.0 };
        let winter = (world.season() == 3) as u8 as f32 * 0.22;
        g[28] = world.tick as f32 / 60.0;
        g[29] = 0.1 + 1.8 * alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64).1 as f32 / 1000.0;
        self.wind = g[29];
        g[30] = storm;
        g[31] = (snowing * 0.8).max(winter);
        self.queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&g));
        {
            // the window shows the middle of the picture, shifted by the sub-texel camera remainder (screen px)
            let k = scale;
            let crop_x = (sw as f32 * scale - win_w as f32).max(0.0).div_euclid(2.0);
            let crop_y = (sh as f32 * scale - win_h as f32).max(0.0).div_euclid(2.0);
            // rain and snow slant with the wind as it crosses the screen
            let [wx, wz, _] = WIND.with(|w| w.get());
            let yaw = cam.yaw_deg.to_radians();
            let slant = wx * yaw.cos() - wz * yaw.sin();
            let p = [k, 0.0, crop_x + off[0] * k, crop_y - off[1] * k, win_w as f32, win_h as f32, t, self.pixel.cinema, self.weather_kind, self.weather_amount, world.tick as f32 / 60.0, slant];
            self.queue.write_buffer(&self.pixel.uniform, 0, bytemuck::cast_slice(&p));
        }

        // --- scene geometry: only chunks the camera can see are built and drawn
        self.frame_no += 1;
        let (cx0, cy0, cx1, cy1, view_m) = visible_chunks(&cam, self.aspect(), world);
        let mut visible: Vec<(i32, i32)> = Vec::new();
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                visible.push((cx, cy));
            }
        }
        let mut built = 0;
        let mut build_ms = 0.0;
        let budget_ms = if self.chunks.is_empty() { 250.0 } else { BUILD_BUDGET_MS };
        for &key in &visible {
            let fresh = self.chunks.get(&key).is_some_and(|c| c.terrain.is_some() && c.nodes_version == world.node_stamp(key.0 * CHUNK_TILES, key.1 * CHUNK_TILES) && c.seen == world.seen_bits() && c.level == world.perception_level());
            // meshes are built within a per-frame time budget; the rest wait for the next frame
            // (a stale mesh keeps showing meanwhile), so flying over new ground never hitches
            if !fresh && built >= 1 && build_ms > budget_ms {
                continue;
            }
            if !fresh {
                let t = std::time::Instant::now();
                let (x0, y0) = (key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
                let (x1, y1) = ((x0 + CHUNK_TILES).min(world.map.w), (y0 + CHUNK_TILES).min(world.map.h));
                let old_terrain = self.chunks.get_mut(&key).and_then(|c| c.terrain.take());
                let terrain = old_terrain.or_else(|| {
                    let mut v = Vec::new();
                    build_terrain(&mut v, &self.art, world, (x0, y0, x1, y1));
                    Some((self.vertex_buffer(&v), v.len() as u32))
                });
                let mut v = Vec::new();
                build_decor(&mut v, world, (x0, y0, x1, y1));
                build_nodes(&mut v, world, (x0, y0, x1, y1), world.perception_level());
                let nodes = (!v.is_empty()).then(|| (self.vertex_buffer(&v), v.len() as u32));
                self.chunks.insert(key, Chunk { terrain, nodes, nodes_version: world.node_stamp(x0, y0), seen: world.seen_bits(), level: world.perception_level(), used: 0 });
                built += 1;
                build_ms += t.elapsed().as_secs_f64() * 1000.0;
            }
            if let Some(c) = self.chunks.get_mut(&key) {
                c.used = self.frame_no;
            }
        }
        if self.chunks.len() > MAX_CACHED_CHUNKS {
            let frame = self.frame_no;
            let mut old: Vec<((i32, i32), u64)> = self.chunks.iter().map(|(k, c)| (*k, c.used)).filter(|(_, u)| *u != frame).collect();
            old.sort_by_key(|(_, u)| *u);
            let drop = self.chunks.len() - MAX_CACHED_CHUNKS * 3 / 4;
            for (k, _) in old.into_iter().take(drop) {
                self.chunks.remove(&k);
            }
        }
        self.stats.chunks_built += built as u64;
        self.stats.build_ms += build_ms;
        let mut dynamic = Vec::new();
        let mut me_body = 0u32..0;
        let art = &self.art;
        let tick = world.tick;
        let secs = tick as f32 / 60.0;
        let inside = |x_m: f32, z_m: f32| x_m >= view_m.0 && x_m <= view_m.2 && z_m >= view_m.1 && z_m <= view_m.3;
        if let Some(tile) = self.hover_node {
            push_hover_rim(&mut dynamic, world, tile, world.perception_level(), &cam, secs);
        }
        for b in world.buildings.iter().filter(|b| inside(b.x as f32 / 1000.0, b.y as f32 / 1000.0)) {
            let (bx, bz) = (b.x as f32 / 1000.0, b.y as f32 / 1000.0);
            decal(&mut dynamic, art.shadow_l, bx + 0.2, bz + 0.15, WHITE, 0.0);
            if b.kind == BuildingKind::Campfire {
                push_campfire(&mut dynamic, bx, bz, b.fuel as f32 / alaz_sim::world::FIRE_FUEL_MAX as f32, secs, None);
            } else {
                push_building(&mut dynamic, b.kind, bx, bz, b.active(), tick, None);
            }
        }
        if let Some(g) = ghost {
            let tint = if g.valid { [0.35, 1.0, 0.45] } else { [1.0, 0.25, 0.25] };
            let r = rules().building(g.kind).radius_mm as f32 / 1000.0;
            set_anchor(g.x_m, g.z_m);
            push_box(&mut dynamic, [g.x_m, 0.03, g.z_m], [r, 0.02, r], tint, 1.0);
            push_building(&mut dynamic, g.kind, g.x_m, g.z_m, false, tick, Some(tint));
        }
        let mm = |v: f32| (v * 1000.0) as i64;
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        // how far drawing is between the last simulated tick and the next one
        if world.tick != self.tick_seen {
            let gap = (now - self.tick_at).as_secs_f32();
            if gap < 0.25 {
                self.tick_gap += (gap - self.tick_gap) * 0.2;
            }
            self.tick_seen = world.tick;
            self.tick_at = now;
        }
        let alpha = self.sim_alpha.unwrap_or_else(|| ((now - self.tick_at).as_secs_f32() / self.tick_gap.max(1e-3)).clamp(0.0, 1.0));
        push_bodies(&mut dynamic, world, alpha, &inside);
        push_fish(&mut dynamic, world, view_m, (world.tick as f32 + alpha) / 60.0, alpha);
        for w in world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)) {
            let (wx, wz) = (w.x as f32 / 1000.0, w.y as f32 / 1000.0);
            // wild animals walk like everyone else: they face where they go and their legs follow the ground covered
            let st = self.wild_anim.entry(w.id).or_insert(Walk::at(wx, wz, (w.id as f32 * 2.399) % std::f32::consts::TAU, 0));
            let (wx, wz) = st.smooth(wx, wz, world.tick, alpha);
            let leg = match w.species {
                alaz_sim::WildSpecies::Dog => 0.22,
                alaz_sim::WildSpecies::Ox => 0.45,
                alaz_sim::WildSpecies::Horse => 0.7,
                alaz_sim::WildSpecies::Wolf => 0.35,
                alaz_sim::WildSpecies::Deer => 0.65,
            };
            let moved = step_walk(st, wx, wz, dt, quad_stride(leg));
            let idle = Pose { heading: st.heading, phase: st.phase, amp: st.amp, id: w.id, stride: st.stride, ..Pose::idle(secs) };
            let _ = moved;
            decal(&mut dynamic, art.shadow_m, wx, wz, WHITE, 0.0);
            match w.species {
                alaz_sim::WildSpecies::Dog => push_creature(&mut dynamic, AgentKind::Dog, wx, wz, false, idle),
                alaz_sim::WildSpecies::Ox => push_creature(&mut dynamic, AgentKind::Ox, wx, wz, false, idle),
                alaz_sim::WildSpecies::Horse => push_creature(&mut dynamic, AgentKind::Horse, wx, wz, false, idle),
                alaz_sim::WildSpecies::Wolf => {
                    push_quad(&mut dynamic, wx, wz, idle, Quad { body: [0.32, 0.32, 0.35], head: [0.36, 0.36, 0.40], len: 0.4, leg_h: 0.35, thick: 0.2, head_r: 0.13, neck: 0.25, tail: true, horns: false });
                }
                alaz_sim::WildSpecies::Deer => {
                    push_quad(&mut dynamic, wx, wz, idle, Quad { body: [0.66, 0.46, 0.26], head: [0.70, 0.50, 0.30], len: 0.42, leg_h: 0.65, thick: 0.2, head_r: 0.11, neck: 0.5, tail: true, horns: true });
                }
            }
        }
        // The stray dog that came to the fire waits for a friend: a soft ring marks it until it is tamed.
        if world.milestone_done(alaz_sim::Milestone::Visitor) && !world.milestone_done(alaz_sim::Milestone::Companion) {
            let pulse = 0.5 + 0.5 * (tick as f32 / 20.0).sin();
            for w in world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)).into_iter().filter(|w| w.species == alaz_sim::WildSpecies::Dog) {
                let (wx, wz) = (w.x as f32 / 1000.0, w.y as f32 / 1000.0);
                let r = 1.1 + 0.15 * pulse;
                for k in 0..16 {
                    let (a0, a1) = (k as f32 * std::f32::consts::TAU / 16.0, (k as f32 + 0.6) * std::f32::consts::TAU / 16.0);
                    push_segment(&mut dynamic, [wx + r * a0.cos(), wz + r * a0.sin()], [wx + r * a1.cos(), wz + r * a1.sin()], 0.07, 0.04, [0.95, 0.85, 0.45], 1.0);
                }
            }
        }
        // Once the tribe makes more sense of the world, it notices the tracks animals leave behind.
        if world.perception_level() >= 2 {
            for w in world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)) {
                push_tracks(&mut dynamic, w.id, w.x as f32 / 1000.0, w.y as f32 / 1000.0, w.heading_mm(), w.species == alaz_sim::WildSpecies::Wolf);
            }
        }
        if self.wild_anim.len() > 512 {
            let live: std::collections::HashSet<u32> = world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)).iter().map(|w| w.id).collect();
            self.wild_anim.retain(|k, _| live.contains(k));
        }
        for a in &world.agents {
            let (x, z) = (a.x as f32 / 1000.0, a.y as f32 / 1000.0);
            let w = self.anim.entry(a.id.0).or_insert(Walk::at(x, z, 0.0, a.health));
            let (x, z) = w.smooth(x, z, world.tick, alpha);
            // a blow makes the body flinch and flash
            if w.hp - a.health >= 150 {
                w.hurt = 1.0;
            }
            w.hp = a.health;
            w.hurt = (w.hurt - dt * 2.4).max(0.0);
            step_walk(w, x, z, dt, match a.kind {
                AgentKind::Human => 0.3,
                AgentKind::Dog => quad_stride(0.22),
                AgentKind::Ox => quad_stride(0.45),
                AgentKind::Horse => quad_stride(0.7),
                AgentKind::Bot => 0.25,
            });
            let (work, tool, target) = work_of(world, a);
            if work != Work::None && w.amp < 0.3 {
                // turn toward what we are working on
                let want = (target.0 - x).atan2(target.1 - z);
                w.heading += wrap_pi(want - w.heading) * (1.0 - (-10.0 * dt).exp());
            }
            let working = work != Work::None && w.amp < 0.3;
            let cyc = work_cycle(world, a, work, w, alpha, secs, dt);
            let worn = |k: alaz_sim::ItemKind, bit: u8| if a.inventory.counts[k as usize] > 0 { bit } else { 0 };
            let outfit = worn(alaz_sim::ItemKind::FiberWrap, 1) | worn(alaz_sim::ItemKind::HideCloak, 2) | worn(alaz_sim::ItemKind::HideBoots, 4);
            let slope = {
                let (sx, sz) = (w.heading.sin() * 0.6, w.heading.cos() * 0.6);
                (ground_height(world.seed, x + sx, z + sz) - ground_height(world.seed, x - sx, z - sz)) / 1.2
            };
            let pose = Pose { heading: w.heading, phase: w.phase, amp: w.amp, t: secs, work: if working { work } else { Work::None }, tool, outfit, id: a.id.0, stage: world.perception_level(), slope, hurt: w.hurt, stride: w.stride, cyc, air: 0.0 };
            if working {
                push_chips(&mut dynamic, work, target, (x, z), cyc, pose.tool);
            }
            // off the ground: the body rides up the flight, tucking its legs on the way up and flailing on the way down
            let flight = world.airborne(a.id).map(|f| {
                let q = alaz_sim::physics::Q as f32;
                let z_m = (f.z as f32 + f.vz as f32 * alpha) / q / 1000.0;
                (z_m - ground_height(world.seed, x, z), (f.vz as f32 / q / 60.0).clamp(-1.0, 1.0))
            });
            let pose = match flight {
                Some((_, v)) => Pose { air: if v.abs() < 0.05 { 0.05 } else { v }, amp: 0.0, work: Work::None, ..pose },
                None => pose,
            };
            decal(&mut dynamic, art.shadow_s, x, z, WHITE, 0.0);
            let body_start = dynamic.len();
            push_creature(&mut dynamic, a.kind, x, z, a.kind.is_worker() || (a.kind == AgentKind::Human && a.id != me), pose);
            if a.id == me {
                me_body = body_start as u32..dynamic.len() as u32;
            }
            if let Some((lift, _)) = flight {
                for v in &mut dynamic[body_start..] {
                    v.pos[1] += lift.max(0.0);
                }
            }
            // the body leans into a turn and a little forward when it hurries (not when swimming or in the air)
            let depth_here = alaz_sim::map::water_depth(&world.map, a.x.div_euclid(1000) as i32, a.y.div_euclid(1000) as i32);
            if a.kind == AgentKind::Human && flight.is_none() && depth_here == 0 {
                let roll = (-w.spin * w.speed * 0.07).clamp(-0.22, 0.22);
                let lean = (w.speed * 0.035).clamp(0.0, 0.11);
                if roll.abs() > 0.002 || lean > 0.002 {
                    let h = w.heading;
                    let fwd = glam::Vec3::new(h.sin(), 0.0, h.cos());
                    let right = glam::Vec3::new(h.cos(), 0.0, -h.sin());
                    let rot = glam::Quat::from_axis_angle(fwd, roll) * glam::Quat::from_axis_angle(right, lean);
                    let foot = glam::Vec3::new(x, 0.0, z);
                    for v in &mut dynamic[body_start..] {
                        let p = glam::Vec3::from(v.pos);
                        v.pos = (rot * (p - foot) + foot).to_array();
                    }
                }
            }
            // in the water: wading, the body goes in to the waist; out of its depth, it lies forward on the surface
            // and swims (the walk's arm swing becomes the stroke), riding up and down on the very swell drawn there
            let depth = alaz_sim::map::water_depth(&world.map, a.x.div_euclid(1000) as i32, a.y.div_euclid(1000) as i32);
            if depth == 1 && flight.is_none() {
                let wave = swell_height(world, x, z, alpha) * 0.5;
                for v in &mut dynamic[body_start..] {
                    v.pos[1] += wave - 0.5;
                }
            } else if depth == 2 && flight.is_none() {
                let wave = swell_height(world, x, z, alpha);
                let h = pose.heading;
                let (fwd, right) = (glam::Vec3::new(h.sin(), 0.0, h.cos()), glam::Vec3::new(h.cos(), 0.0, -h.sin()));
                // tip forward about the hips, a little more as the stroke pulls, and rock with the waves' slope
                let tilt = 1.30 + 0.06 * (secs * 2.2 + a.id.0 as f32).sin();
                let rot = glam::Quat::from_axis_angle(right, tilt);
                let pivot = glam::Vec3::new(x, 1.0, z);
                // the head and shoulders ride just above the surface
                let lift = glam::Vec3::new(0.0, wave - 0.78, 0.0) + fwd * 0.4;
                for v in &mut dynamic[body_start..] {
                    let p = glam::Vec3::from(v.pos);
                    let q = rot * (p - pivot) + pivot + lift;
                    v.pos = q.to_array();
                }
            }
        }
        if self.anim.len() > world.agents.len() * 2 + 8 {
            let live: std::collections::HashSet<u32> = world.agents.iter().map(|a| a.id.0).collect();
            self.anim.retain(|k, _| live.contains(k));
        }
        for &i in routes {
            if let Some(a) = world.agents.get(i) {
                push_route(&mut dynamic, world, a, tick);
                // a resource being gone for glows itself, as under the pointer, instead of carrying a flag
                if let alaz_sim::world::Task::Gather { node, .. } = a.task {
                    if let Some(n) = world.nodes.get(node.0 as usize) {
                        push_hover_rim(&mut dynamic, world, (n.tx, n.ty), world.perception_level(), &cam, secs);
                    }
                }
            }
        }
        let dyn_buf = self.vertex_buffer(&dynamic);

        // --- egui
        let raw = self.egui_state.take_egui_input(&self.window);
        let full = self.egui_ctx.run_pass(raw, |ctx| hud(ctx));
        self.egui_state.handle_platform_output(&self.window, full.platform_output);
        let prims = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
        let mut tex = full.textures_delta;
        for (id, deltas) in std::mem::take(&mut tex.set) {
            for delta in &deltas {
                self.egui_renderer.update_texture(&self.device, &self.queue, id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: full.pixels_per_point,
        };

        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            // what the sun sees: only models cast; the ground and flat decals just receive
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, Some(&self.shadow_bind_group), &[]);
            for key in &visible {
                let Some(c) = self.chunks.get(key) else { continue };
                if let Some((b, n)) = &c.nodes {
                    pass.set_vertex_buffer(0, b.slice(..));
                    pass.draw(0..*n, 0..1);
                }
            }
            pass.set_vertex_buffer(0, dyn_buf.slice(..));
            pass.draw(0..dynamic.len() as u32, 0..1);
        }
        let extra = self.egui_renderer.update_buffers(&self.device, &self.queue, &mut enc, &prims, &screen);
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.01 * tint[0] as f64, g: 0.012 * tint[1] as f64, b: 0.02 * tint[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // the picture is drawn into the top-left sw x sh texels of the scene texture
            pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
            pass.set_bind_group(0, Some(&self.bind_group), &[]);
            pass.set_pipeline(&self.tri_pipeline);
            let mut verts = 0u64;
            for key in &visible {
                let Some(c) = self.chunks.get(key) else { continue };
                if let Some((b, n)) = &c.terrain {
                    pass.set_vertex_buffer(0, b.slice(..));
                    pass.draw(0..*n, 0..1);
                    verts += *n as u64;
                }
                if let Some((b, n)) = &c.nodes {
                    pass.set_vertex_buffer(0, b.slice(..));
                    pass.draw(0..*n, 0..1);
                    verts += *n as u64;
                }
            }
            self.stats.chunks_drawn = visible.len() as u32;
            self.stats.vertices = verts + dynamic.len() as u64;
            pass.set_vertex_buffer(0, dyn_buf.slice(..));
            pass.draw(0..dynamic.len() as u32, 0..1);
            if !me_body.is_empty() {
                pass.set_pipeline(&self.mark_pipeline);
                pass.draw(me_body.clone(), 0..1);
                pass.set_pipeline(&self.hidden_pipeline);
                pass.draw(me_body.clone(), 0..1);
            }
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.inked,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
            pass.set_pipeline(&self.outline.pipeline);
            pass.set_bind_group(0, Some(&self.outline.group), &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pixel"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pixel.pipeline);
            pass.set_bind_group(0, Some(&self.pixel.group), &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.egui_renderer.render(&mut pass, &prims, &screen);
        }
        self.queue.submit(extra.into_iter().chain([enc.finish()]));
        for id in &tex.free {
            self.egui_renderer.free_texture(id);
        }
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}

/// Follow a creature from where it was to where it is now: turn toward the way it goes, advance the stride by the
/// ground covered, and ease the stride size toward its pace (a run swings harder). True when it moved.
/// Follows a creature's drawn position: turns it the way it goes and runs its walk cycle. `base` is how far a foot
/// swings at a walk (metres); at a run the stride lengthens. The cycle advances by exactly the ground covered, so
/// a planted foot moves back as fast as the body moves forward and nothing slides.
fn step_walk(w: &mut Walk, x: f32, z: f32, dt: f32, base: f32) -> bool {
    let (dx, dz) = (x - w.x, z - w.z);
    let step = (dx * dx + dz * dz).sqrt();
    let moved = step > 0.0002 && step < 3.0;
    let v = if moved { step / dt.max(1e-3) } else { 0.0 };
    w.speed += (v - w.speed) * (1.0 - (-10.0 * dt).exp());
    // turning: a spring toward where it is going (no snapping round), its speed kept for the lean
    let want = if moved { dx.atan2(dz) } else { w.heading };
    let omega = 11.0;
    let off = wrap_pi(want - w.heading);
    w.spin += (omega * omega * off - 2.0 * omega * w.spin) * dt.min(0.05);
    w.heading += w.spin * dt;
    // walking is about 1.5 m/s; faster is a run, with longer strides
    let pace = if w.speed > 0.05 { (w.speed / 1.5).clamp(1.0, 1.8) } else { 0.0 };
    w.amp += (pace - w.amp) * (1.0 - (-12.0 * dt).exp());
    let stride = base * (1.0 + 0.45 * (pace.max(1.0) - 1.0));
    w.stride += (stride - w.stride) * (1.0 - (-8.0 * dt).exp());
    if moved {
        // a foot travels 2 * stride while planted, over half a cycle (pi)
        w.phase += step * std::f32::consts::PI / (2.0 * w.stride.max(0.05));
    }
    (w.x, w.z) = (x, z);
    moved
}

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

fn wrap_pi(mut d: f32) -> f32 {
    while d > std::f32::consts::PI {
        d -= 2.0 * std::f32::consts::PI;
    }
    while d < -std::f32::consts::PI {
        d += 2.0 * std::f32::consts::PI;
    }
    d
}


const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

thread_local! {
    /// Ground point of the object being built: its surface texture is laid out relative to this, so the texture
    /// stays on the object when it moves.
    static ANCHOR: std::cell::Cell<[f32; 2]> = const { std::cell::Cell::new([0.0, 0.0]) };
    /// While set, everything pushed is leaf material whatever its colour (autumn crowns are orange, not green).
    static FOLIAGE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const HWIN: u32 = 256;

/// Ground height in metres at a point, bilinear between tile corners (what the shader lifts everything by).
/// Distance to the shore at every tile corner of the height window, in tiles: positive out on the water (how deep
/// it is, as far as the eye can tell), negative on land, 0 along the water's edge. Two chamfer passes over the window.
fn shore_field(world: &World, hx: i32, hy: i32) -> Vec<f32> {
    const CAP: f32 = 6.0;
    let n = HWIN as i32 + 1;
    let wet = |tx: i32, ty: i32| world.map.terrain(tx, ty).is_none_or(|t| t == Terrain::Water);
    // per tile (one wider than the window, so every corner has its four tiles): distance to the other kind
    let mut d = vec![CAP; (n * n) as usize];
    let water: Vec<bool> = (0..n * n).map(|i| wet(hx - 1 + i % n, hy - 1 + i / n)).collect();
    let idx = |x: i32, y: i32| (y * n + x) as usize;
    for y in 0..n {
        for x in 0..n {
            let me = water[idx(x, y)];
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let (a, b) = (x + dx, y + dy);
                a >= 0 && b >= 0 && a < n && b < n && water[idx(a, b)] != me
            });
            if edge {
                d[idx(x, y)] = 1.0;
            }
        }
    }
    let pass = |d: &mut Vec<f32>, order: &mut dyn Iterator<Item = (i32, i32)>, back: bool| {
        let s = if back { 1 } else { -1 };
        for (x, y) in order {
            let mut best = d[idx(x, y)];
            for (dx, dy, c) in [(s, 0, 1.0), (0, s, 1.0), (s, s, 1.414), (-s, s, 1.414)] {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < n && b < n {
                    best = best.min(d[idx(a, b)] + c);
                }
            }
            d[idx(x, y)] = best.min(CAP);
        }
    };
    pass(&mut d, &mut (0..n).flat_map(|y| (0..n).map(move |x| (x, y))), false);
    pass(&mut d, &mut (0..n).rev().flat_map(|y| (0..n).rev().map(move |x| (x, y))), true);
    let signed = |x: i32, y: i32| if water[idx(x, y)] { d[idx(x, y)] - 0.5 } else { -(d[idx(x, y)] - 0.5) };
    (0..HWIN as i32 * HWIN as i32)
        .map(|i| {
            let (x, y) = (i % HWIN as i32 + 1, i / HWIN as i32 + 1);
            // a corner sits between four tiles
            (signed(x - 1, y - 1) + signed(x, y - 1) + signed(x - 1, y) + signed(x, y)) * 0.25
        })
        .collect()
}

pub fn ground_height(seed: u64, x: f32, z: f32) -> f32 {
    let (fx, fz) = (x.floor(), z.floor());
    let (tx, tz) = (x - fx, z - fz);
    let h = |dx: i32, dz: i32| alaz_sim::elevation_mm(seed, fx as i32 + dx, fz as i32 + dz) as f32 / 1000.0;
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * tx;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * tx;
    a + (b - a) * tz
}

fn foliage(on: bool) {
    FOLIAGE.with(|f| f.set(on));
}

fn set_anchor(x: f32, z: f32) {
    ANCHOR.with(|a| a.set([x, z]));
}

/// Which painted material a surface of this colour is made of: 0 none, 1 wood, 2 leaves, 3 stone, 4 skin and cloth.
fn material_of(c: [f32; 3]) -> f32 {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let sat = (mx - mn) / mx.max(1e-3);
    if sat < 0.18 || c[2] > c[0] {
        3.0
    } else if c[1] >= c[0] * 1.05 && c[1] >= c[2] {
        2.0
    } else if c[0] >= c[1] && mx < 0.5 {
        1.0
    } else {
        4.0
    }
}

fn vl(pos: [f32; 3], color: [f32; 3], glow: f32, light: f32) -> Vertex {
    let mat = if glow > 0.5 { 0.0 } else if FOLIAGE.with(|f| f.get()) { 2.0 } else { material_of(color) };
    Vertex { pos, color, glow, uv: [light, mat], off: ANCHOR.with(|a| a.get()), kind: 4.0 }
}

fn v(pos: [f32; 3], color: [f32; 3], glow: f32) -> Vertex {
    vl(pos, color, glow, 1.0)
}


/// A flat painted rectangle lying on the ground (shadow, pool of firelight), `w` x `d` metres, centred on (x, z).
fn decal(out: &mut Vec<Vertex>, s: Spr, x: f32, z: f32, tint: [f32; 3], glow: f32) {
    decal_sized(out, s, x, z, s.w as f32 / sprites::PPM, s.h as f32 / sprites::PPM, tint, glow);
}

/// A decal stretched to `w` x `d` metres whatever its texel size (ground fringes are one tile).
#[allow(clippy::too_many_arguments)]
fn decal_sized(out: &mut Vec<Vertex>, s: Spr, x: f32, z: f32, w: f32, d: f32, tint: [f32; 3], glow: f32) {
    let (u0, u1, v0, v1) = (s.x as f32, (s.x + s.w) as f32, s.y as f32, (s.y + s.h) as f32);
    let q = |dx: f32, dz: f32, u: f32, vv: f32| Vertex { pos: [x + dx, 0.015, z + dz], color: tint, glow, uv: [u, vv], off: [0.0, 0.0], kind: 5.0 };
    let (a, b, c, e) = (q(-w / 2.0, -d / 2.0, u0, v0), q(w / 2.0, -d / 2.0, u1, v0), q(w / 2.0, d / 2.0, u1, v1), q(-w / 2.0, d / 2.0, u0, v1));
    out.extend_from_slice(&[a, b, c, a, c, e]);
}

/// Flat colour tint of one ground tile: broad light and dark patches (close to 1).
fn tile_shade(world: &World, tx: i32, ty: i32) -> f32 {
    1.0 + 0.09 * noise(world, 0x7771, tx as f32 * 0.16, ty as f32 * 0.16) + 0.025 * noise(world, 0x7773, tx as f32 * 0.9, ty as f32 * 0.9)
}

fn tile_spr(art: &Art, t: Terrain, h: u32) -> Spr {
    let i = (h % 4) as usize;
    match t {
        Terrain::Water => art.water[i],
        Terrain::Sand => art.sand[i],
        Terrain::Grass => art.grass[i],
        Terrain::Forest => art.forest[i],
        Terrain::Rock => art.rock[i],
    }
}

fn build_terrain(out: &mut Vec<Vertex>, art: &Art, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    for ty in y0..y1 {
        for tx in x0..x1 {
            let t = world.map.terrain(tx, ty).unwrap_or(Terrain::Water);
            let h = (alaz_sim::rng::hash2(world.seed ^ 0x7772, tx, ty) >> 20) as u32;
            let s = tile_spr(art, t, h);
            let k = if t == Terrain::Water { 1.0 } else { tile_shade(world, tx, ty) };
            let (fx, fz) = (tx as f32, ty as f32);
            let c = |dx: f32, dz: f32| Vertex { pos: [fx + dx, 0.0, fz + dz], color: [k, k, k], glow: 0.0, uv: [s.x as f32, s.y as f32], off: [if t == Terrain::Water { 1.0 } else { 0.0 }, 0.0], kind: 1.0 };
            let (a, b, cc, d) = (c(0.0, 0.0), c(1.0, 0.0), c(1.0, 1.0), c(0.0, 1.0));
            out.extend_from_slice(&[a, b, cc, a, cc, d]);
            // (ground boundaries are drawn ragged by the shader now, from the tile window)
            let _ = edge_blend;
        }
    }
}

/// Higher-ranked ground creeps over lower-ranked neighbours with a ragged, dithered fringe.
fn edge_blend(out: &mut Vec<Vertex>, art: &Art, world: &World, tx: i32, ty: i32, t: Terrain) {
    let rank = |t: Terrain| -> i32 { match t {
        Terrain::Water => 0,
        Terrain::Sand => 1,
        Terrain::Grass => 2,
        Terrain::Forest => 3,
        Terrain::Rock => 4,
    } };
    // (dx, dy, direction index): edges N E S W, then corners NE SE SW NW
    const DIRS: [(i32, i32, usize); 8] = [(0, -1, 0), (1, 0, 1), (0, 1, 2), (-1, 0, 3), (1, -1, 4), (1, 1, 5), (-1, 1, 6), (-1, -1, 7)];
    let at = |x: i32, y: i32| world.map.terrain(x, y);
    let mut found: [Option<(Terrain, usize)>; 8] = [None; 8];
    for (i, &(dx, dy, dir)) in DIRS.iter().enumerate() {
        let Some(n) = at(tx + dx, ty + dy) else { continue };
        if rank(n) <= rank(t) {
            continue;
        }
        if dir >= 4 {
            // a corner only matters when neither adjacent edge already carries the same ground
            if at(tx + dx, ty) == Some(n) || at(tx, ty + dy) == Some(n) {
                continue;
            }
        }
        found[i] = Some((n, dir));
    }
    found.sort_by_key(|f| f.map_or(-1, |(n, _)| rank(n)));
    for (n, dir) in found.into_iter().flatten() {
        let (fx, fz) = (tx as f32 + 0.5, ty as f32 + 0.5);
        let k = if n == Terrain::Water { 1.0 } else { tile_shade(world, tx, ty) };
        decal_sized(out, art.edge[rank(n) as usize * 8 + dir], fx, fz, 1.0, 1.0, [k, k, k], 0.0);
    }
}

/// A box with a flat light level per face (top lit, sides in between, bottom dark).
fn push_box(out: &mut Vec<Vertex>, base: [f32; 3], half: [f32; 3], col: [f32; 3], glow: f32) {
    let [cx, cy, cz] = base;
    let [hx, h, hz] = half;
    let p = |sx: f32, sy: f32, sz: f32| [cx + sx * hx, cy + sy * h, cz + sz * hz];
    let faces: [([[f32; 3]; 4], f32); 6] = [
        ([p(-1., 1., -1.), p(1., 1., -1.), p(1., 1., 1.), p(-1., 1., 1.)], 1.0),
        ([p(-1., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(-1., 1., 1.)], 0.7),
        ([p(1., 0., -1.), p(-1., 0., -1.), p(-1., 1., -1.), p(1., 1., -1.)], 0.5),
        ([p(1., 0., 1.), p(1., 0., -1.), p(1., 1., -1.), p(1., 1., 1.)], 0.64),
        ([p(-1., 0., -1.), p(-1., 0., 1.), p(-1., 1., 1.), p(-1., 1., -1.)], 0.48),
        ([p(-1., 0., -1.), p(1., 0., -1.), p(1., 0., 1.), p(-1., 0., 1.)], 0.3),
    ];
    for (q, k) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(vl(q[i], col, glow, k));
        }
    }
}


/// Small living detail on the ground: grass tufts, wild flowers, ferns, mushrooms, reeds, pebbles. Purely visual and
/// derived from the seed, so nothing here is saved or simulated.
fn build_decor(out: &mut Vec<Vertex>, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    use shapes::{limb, rock, sphere};
    for ty in y0..y1 {
        for tx in x0..x1 {
            let Some(t) = world.map.terrain(tx, ty) else { continue };
            if t == Terrain::Water || alaz_sim::elevation_mm(world.seed, tx, ty) > 9_000 {
                continue;
            }
            let hs = alaz_sim::rng::hash2(world.seed ^ 0xDEC0, tx, ty);
            let roll = hs % 1000;
            let seed = (hs >> 10) as u32;
            let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
            let (x, z) = (tx as f32 + 0.5 + 0.38 * jit(3), ty as f32 + 0.5 + 0.38 * jit(9));
            // dry or lush ground tints the grass
            let bio = noise(world, 0x51DE, tx as f32 * 0.035, ty as f32 * 0.035);
            let green = [0.30 + 0.12 * jit(5) - 0.10 * (bio < -0.2) as u8 as f32, 0.55 + 0.10 * jit(6) + 0.08 * (bio > 0.3) as u8 as f32, 0.20 + 0.06 * jit(7)];
            let (warm, wet) = alaz_sim::climate(world.seed, tx, ty);
            let (warm, wet) = (warm as f32 / 1000.0, wet as f32 / 1000.0);
            let cold = warm < 0.38;
            let steppe = warm > 0.56 && wet < 0.5;
            let jungle = warm > 0.56 && wet >= 0.5;
            let autumn = !cold && warm < 0.55 && warm > 0.32 && wet > 0.4;
            // the colour of the grass is the colour of the country
            let green = if cold {
                [0.40 + 0.05 * jit(5), 0.56, 0.46]
            } else if steppe {
                [0.80 + 0.06 * jit(5), 0.66 + 0.05 * jit(6), 0.28]
            } else if jungle {
                [0.10 + 0.04 * jit(5), 0.54 + 0.06 * jit(6), 0.16]
            } else if autumn {
                [0.76 + 0.06 * jit(5), 0.46 + 0.08 * jit(6), 0.16]
            } else {
                green
            };
            let dry = [0.62 + 0.08 * jit(5), 0.56 + 0.06 * jit(6), 0.28];
            let blade = |out: &mut Vec<Vertex>, bx: f32, bz: f32, h: f32, lean: (f32, f32), col: [f32; 3]| {
                limb(out, [bx, 0.0, bz], [bx + lean.0, h * 1.25, bz + lean.1], 0.034, 0.006, col, 3);
            };
            match t {
                Terrain::Grass => {
                    let flowery = if wet > 0.55 && !cold { 330 } else if steppe { 40 } else if cold { 25 } else { 90 };
                    if roll < 380 {
                        set_anchor(x, z);
                        let col = if bio < -0.25 && !steppe && !jungle { dry } else { green };
                        for k in 0..4u32 {
                            let a = k as f32 * 1.7 + jit(2);
                            let tall = if steppe { 1.7 } else if jungle { 1.35 } else { 1.0 };
                            blade(out, x + a.cos() * 0.05, z + a.sin() * 0.05, (0.22 + 0.16 * ((k + seed) % 3) as f32 / 2.0) * tall, (a.cos() * 0.1, a.sin() * 0.1), col);
                        }
                    } else if roll < 380 + flowery {
                        // wild flowers in one of several colours, a few to a clump
                        const PETALS: [[f32; 3]; 6] = [[0.96, 0.84, 0.25], [0.97, 0.97, 0.93], [0.93, 0.50, 0.68], [0.66, 0.46, 0.88], [0.92, 0.28, 0.24], [0.40, 0.62, 0.95]];
                        let pc = if steppe {
                            [[0.96, 0.78, 0.18], [0.94, 0.52, 0.16], [0.88, 0.26, 0.2]][(seed % 3) as usize]
                        } else if cold {
                            [[0.96, 0.97, 0.99], [0.55, 0.7, 0.96], [0.72, 0.6, 0.92]][(seed % 3) as usize]
                        } else if jungle {
                            [[0.95, 0.2, 0.45], [0.98, 0.55, 0.12], [0.62, 0.3, 0.9], [0.98, 0.9, 0.2]][(seed % 4) as usize]
                        } else {
                            PETALS[(seed % 6) as usize]
                        };
                        set_anchor(x, z);
                        for k in 0..(2 + seed % 3) {
                            let (dx, dz) = (jit(11 + k) * 0.09, jit(17 + k) * 0.09);
                            let h = 0.22 + 0.1 * ((seed >> k) % 3) as f32;
                            blade(out, x + dx, z + dz, h, (0.01, 0.0), green);
                            sphere(out, [x + dx + 0.01, h + 0.02, z + dz], [0.075, 0.045, 0.075], pc, 6, 3, 0.0, 1);
                            sphere(out, [x + dx + 0.01, h + 0.04, z + dz], [0.03, 0.022, 0.03], [0.98, 0.82, 0.2], 4, 3, 0.0, 1);
                        }
                    }
                }
                Terrain::Forest => {
                    if roll < 90 {
                        // a fern: arching fronds from one point
                        set_anchor(x, z);
                        for k in 0..7u32 {
                            let a = k as f32 * 0.9 + jit(2);
                            limb(out, [x, 0.02, z], [x + a.cos() * 0.26, 0.2, z + a.sin() * 0.26], 0.014, 0.004, [0.16, 0.42 + 0.06 * jit(4), 0.2], 3);
                        }
                    } else if roll < 115 {
                        let cap = if seed % 3 == 0 { [0.86, 0.22, 0.18] } else { [0.62, 0.44, 0.28] };
                        set_anchor(x, z);
                        limb(out, [x, 0.0, z], [x, 0.09, z], 0.02, 0.016, [0.92, 0.9, 0.82], 4);
                        sphere(out, [x, 0.1, z], [0.06, 0.035, 0.06], cap, 7, 3, 0.0, 2);
                    }
                }
                Terrain::Sand => {
                    let near_water = world.map.water_within(tx, ty, 1);
                    if near_water && roll < 160 {
                        // reeds with brown heads, tall and thin
                        set_anchor(x, z);
                        for k in 0..4u32 {
                            let (dx, dz) = (jit(21 + k) * 0.1, jit(27 + k) * 0.1);
                            let h = 0.7 + 0.35 * ((seed >> k) % 3) as f32 / 2.0;
                            blade(out, x + dx, z + dz, h, (0.05 * jit(31 + k), 0.04), [0.42, 0.55, 0.26]);
                            limb(out, [x + dx, h - 0.1, z + dz], [x + dx + 0.01, h + 0.06, z + dz], 0.022, 0.018, [0.42, 0.26, 0.14], 4);
                        }
                    } else if roll < 50 {
                        set_anchor(x, z);
                        rock(out, [x, 0.03, z], [0.07, 0.04, 0.06], [0.72, 0.68, 0.6], seed);
                        rock(out, [x + 0.09, 0.02, z + 0.04], [0.04, 0.025, 0.04], [0.66, 0.62, 0.56], seed ^ 3);
                    } else if roll < 80 {
                        set_anchor(x, z);
                        for k in 0..3u32 {
                            let a = k as f32 * 2.1 + jit(2);
                            blade(out, x + a.cos() * 0.03, z + a.sin() * 0.03, 0.2, (a.cos() * 0.08, a.sin() * 0.08), [0.74, 0.66, 0.34]);
                        }
                    }
                }
                Terrain::Rock => {
                    if roll < 80 {
                        set_anchor(x, z);
                        let g = 0.5 + 0.08 * jit(4);
                        rock(out, [x, 0.07, z], [0.14 + 0.06 * jit(5).abs(), 0.09, 0.12], [g, g, g + 0.02], seed);
                    } else if roll < 140 {
                        set_anchor(x, z);
                        for k in 0..3u32 {
                            let g = 0.46 + 0.1 * jit(k + 4);
                            rock(out, [x + jit(k + 8) * 0.12, 0.03, z + jit(k + 14) * 0.12], [0.05, 0.03, 0.05], [g, g, g + 0.02], seed ^ k);
                        }
                    }
                }
                Terrain::Water => {}
            }
        }
    }
}

fn build_nodes(out: &mut Vec<Vertex>, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32), level: u8) {
    for n in world.nodes_in(x0, y0, x1, y1) {
        push_node(out, world, n, level);
    }
}

/// The node under the pointer again, puffed out a little and in one bright colour, sitting just behind the real
/// one: what shows around the real model is a glowing rim.
fn push_hover_rim(out: &mut Vec<Vertex>, world: &World, tile: (i32, i32), level: u8, cam: &Camera, t: f32) {
    let Some(id) = world.node_at(tile.0, tile.1) else { return };
    let Some(n) = world.nodes.get(id.0 as usize) else { return };
    let mut verts = Vec::new();
    push_node(&mut verts, world, n, level);
    set_anchor(0.0, 0.0);
    foliage(false);
    if verts.is_empty() {
        return;
    }
    let inv = 1.0 / verts.len() as f32;
    let c = verts.iter().fold([0.0f32; 3], |a, v| [a[0] + v.pos[0] * inv, a[1] + v.pos[1] * inv, a[2] + v.pos[2] * inv]);
    let (p, y) = (cam.pitch_deg.to_radians(), cam.yaw_deg.to_radians());
    let fwd = [-(p.cos() * y.sin()), -p.sin(), -(p.cos() * y.cos())];
    let pulse = 0.5 + 0.5 * (t * 4.0).sin();
    let col = [1.0, 0.86 + 0.1 * pulse, 0.42 + 0.3 * pulse];
    for mut v in verts {
        let d = [v.pos[0] - c[0], (v.pos[1] - c[1]) * 0.8, v.pos[2] - c[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
        let grow = 0.09;
        for k in 0..3 {
            v.pos[k] += d[k] / len * grow + fwd[k] * 0.25;
        }
        v.color = col;
        v.glow = 2.0;
        out.push(v);
    }
}

/// Where a node stands (its model's anchor), and how wide and tall it is: used to tell what hides the player.
fn node_shape(world: &World, n: &alaz_sim::world::Node) -> ([f32; 2], f32, f32) {
    let seed = alaz_sim::rng::hash2(world.seed ^ 0x51, n.tx, n.ty) as u32;
    let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
    let at = [n.tx as f32 + 0.5 + 0.18 * jit(3), n.ty as f32 + 0.5 + 0.18 * jit(7)];
    let (r, h) = match n.kind {
        NodeKind::Tree => {
            let h = alaz_sim::map::tree_height_mm(world.seed, n.tx, n.ty) as f32 / 1000.0;
            (h * 0.2 + 0.3, h)
        }
        NodeKind::CaveMouth => (1.4, 2.2),
        _ => (0.6, 0.9),
    };
    (at, r, h)
}

/// Where a node's model stands and how wide and tall it is, for pointer picking.
pub fn node_bounds(world: &World, n: &alaz_sim::world::Node) -> ([f32; 2], f32, f32) {
    node_shape(world, n)
}

fn push_node(out: &mut Vec<Vertex>, world: &World, n: &alaz_sim::world::Node, level: u8) {
    use shapes::{limb, rock, sphere};
    {
        let seed = alaz_sim::rng::hash2(world.seed ^ 0x51, n.tx, n.ty) as u32;
        let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
        // things you have never handled look like the ordinary thing they resemble
        let known = world.has_seen(n.kind.item());
        let (x, z) = (n.tx as f32 + 0.5 + 0.18 * jit(3), n.ty as f32 + 0.5 + 0.18 * jit(7));
        set_anchor(x, z);
        let stone = |out: &mut Vec<Vertex>, x: f32, z: f32| {
            let g = 0.58 + 0.06 * jit(11);
            rock(out, [x, 0.2, z], [0.42 + 0.06 * jit(5), 0.26, 0.36], [g, g, g + 0.02], seed);
            rock(out, [x + 0.3, 0.1, z + 0.15], [0.2, 0.14, 0.18], [g * 0.9, g * 0.9, g * 0.92], seed ^ 77);
        };
        let tuft = |out: &mut Vec<Vertex>, x: f32, z: f32, col: [f32; 3]| {
            for k in 0..5 {
                let a = k as f32 * 1.3 + jit(2);
                let (dx, dz) = (a.cos() * 0.14, a.sin() * 0.14);
                limb(out, [x + dx * 0.4, 0.0, z + dz * 0.4], [x + dx * 1.4, 0.34 + 0.08 * (k % 3) as f32, z + dz * 1.4], 0.025, 0.004, col, 4);
            }
        };
        match n.kind {
            NodeKind::StoneOutcrop => stone(out, x, z),
            NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop => {
                stone(out, x, z);
                if known {
                    let vein = match n.kind {
                        NodeKind::IronOutcrop => [0.66, 0.34, 0.16],
                        NodeKind::CopperOutcrop => [0.22, 0.66, 0.52],
                        _ => [0.82, 0.84, 0.90],
                    };
                    sphere(out, [x + 0.22, 0.34, z - 0.08], [0.1, 0.07, 0.09], vein, 6, 4, 0.1, seed);
                    sphere(out, [x - 0.18, 0.28, z + 0.12], [0.08, 0.06, 0.08], vein, 6, 4, 0.1, seed ^ 5);
                    sphere(out, [x + 0.02, 0.44, z + 0.16], [0.07, 0.05, 0.07], vein, 6, 4, 0.1, seed ^ 9);
                } else if level >= 3 {
                    // not handled yet, but a sharper eye notices something odd in the rock
                    sphere(out, [x + 0.2, 0.34, z - 0.06], [0.07, 0.05, 0.06], [0.72, 0.66, 0.58], 6, 4, 0.1, seed ^ 13);
                }
            }
            NodeKind::Tree => {
                // saplings, grown trees and a few giants; forests lean to pines. A person stands about 1.8 m tall.
                let height = alaz_sim::map::tree_height_mm(world.seed, n.tx, n.ty) as f32 / 1000.0;
                let (warm, wet) = alaz_sim::climate(world.seed, n.tx, n.ty);
                let (warm, wet) = (warm as f32 / 1000.0, wet as f32 / 1000.0);
                let cold = warm < 0.38;
                let steppe = warm > 0.56 && wet < 0.5;
                let jungle = warm > 0.56 && wet >= 0.5;
                let autumn = !cold && warm < 0.55 && warm > 0.32 && wet > 0.4;
                let desert = warm > 0.66 && wet < 0.34;
                let swamp = wet > 0.7 && warm > 0.4 && warm < 0.6;
                let terrain = world.map.terrain(n.tx, n.ty);
                let pine = cold && (seed >> 4) % 100 < 85
                    || terrain == Some(Terrain::Forest) && !steppe && !jungle && (seed >> 4) % 100 < 45
                    || terrain == Some(Terrain::Grass) && (seed >> 4) % 100 < 8;
                let trunk_r = 0.05 + height * 0.022;
                let bark = [0.34 + 0.04 * jit(4), 0.24, 0.15];
                if desert {
                    // a cactus: a fat ribbed column with an arm or two turned up
                    let h = (height * 0.55).clamp(1.2, 3.4);
                    let col = [0.34 + 0.04 * jit(6), 0.52 + 0.05 * jit(8), 0.28];
                    let r = 0.16 + 0.03 * jit(3);
                    limb(out, [x, 0.0, z], [x, h, z], r, r * 0.9, col, 8);
                    sphere(out, [x, h, z], [r * 0.9, r * 0.6, r * 0.9], col, 8, 4, 0.0, 1);
                    let arms = 1 + (seed >> 9) % 2;
                    for k in 0..arms {
                        let a = k as f32 * std::f32::consts::PI + jit(5);
                        let (ax, az) = (a.cos(), a.sin());
                        let y0 = h * (0.4 + 0.15 * k as f32);
                        let out_p = [x + ax * 0.42, y0 + 0.05, z + az * 0.42];
                        limb(out, [x, y0, z], out_p, r * 0.6, r * 0.6, col, 6);
                        let top = [out_p[0], y0 + h * 0.35, out_p[2]];
                        limb(out, out_p, top, r * 0.62, r * 0.55, col, 6);
                        sphere(out, top, [r * 0.55, r * 0.4, r * 0.55], col, 6, 3, 0.0, 2);
                    }
                    // a red flower on top of some
                    if seed % 3 == 0 {
                        sphere(out, [x, h + r * 0.55, z], [0.07, 0.05, 0.07], [0.86, 0.24, 0.3], 5, 3, 0.0, 3);
                    }
                } else if swamp && !pine {
                    // a willow: a leaning trunk and a crown that hangs down in long strands
                    let lean = 0.2 * jit(4) * height;
                    let top = [x + lean, height * 0.62, z];
                    limb(out, [x, 0.0, z], top, trunk_r * 1.2, trunk_r * 0.7, [0.30, 0.25, 0.17], 7);
                    let cr = height * 0.2 + 0.4;
                    let g = [0.42 + 0.04 * jit(6), 0.52 + 0.05 * jit(8), 0.22];
                    foliage(true);
                    sphere(out, [top[0], top[1] + cr * 0.3, top[2]], [cr, cr * 0.55, cr], g, 10, 5, 0.12, seed);
                    for k in 0..14u32 {
                        let a = k as f32 * 0.449 + jit(k + 7) * 0.3;
                        let (ax, az) = (a.cos() * cr * 0.9, a.sin() * cr * 0.9);
                        let len = height * (0.32 + 0.1 * jit(k + 11).abs());
                        limb(out, [top[0] + ax, top[1] + cr * 0.2, top[2] + az], [top[0] + ax * 1.08, (top[1] - len).max(height * 0.3), top[2] + az * 1.08], 0.09, 0.04, [g[0] * 0.92, g[1] * 0.95, g[2]], 5);
                    }
                    foliage(false);
                } else if pine {
                    let g = if cold { [0.12 + 0.03 * jit(6), 0.36 + 0.05 * jit(8), 0.30] } else { [0.15 + 0.04 * jit(6), 0.42 + 0.06 * jit(8), 0.22 + 0.04 * jit(10)] };
                    // the trunk runs up into the crown and shows bare below the lowest tier
                    limb(out, [x, 0.0, z], [x, height * 0.85, z], trunk_r, trunk_r * 0.35, bark, 7);
                    // tiers of needles, widest at the bottom
                    foliage(true);
                    for tier in 0..4u32 {
                        let f = tier as f32 / 4.0;
                        let (y0, y1) = (height * (0.32 + 0.15 * f), height * (0.5 + 0.15 * f));
                        let rad = height * (0.19 - 0.035 * tier as f32);
                        let shade = 1.0 + 0.05 * tier as f32;
                        limb(out, [x, y0, z], [x, y1, z], rad, 0.02, [g[0] * shade, g[1] * shade, g[2] * shade], 8);
                    }
                    limb(out, [x, height * 0.82, z], [x, height, z], height * 0.1, 0.01, [g[0] * 1.2, g[1] * 1.2, g[2] * 1.1], 7);
                    foliage(false);
                } else if steppe {
                    // an acacia: a thin leaning trunk and one flat umbrella of leaves
                    let lean = 0.25 * jit(4) * height;
                    limb(out, [x, 0.0, z], [x + lean, height * 0.62, z], trunk_r * 0.8, trunk_r * 0.5, [0.38, 0.3, 0.2], 6);
                    let cr = height * 0.17 + 0.25;
                    let top = [0.58 + 0.06 * jit(6), 0.62 + 0.05 * jit(8), 0.24];
                    foliage(true);
                    sphere(out, [x + lean, height * 0.68, z], [cr * 1.3, cr * 0.3, cr * 1.3], top, 10, 5, 0.06, seed);
                    sphere(out, [x + lean + cr * 0.5, height * 0.62, z + cr * 0.3], [cr * 0.9, cr * 0.2, cr * 0.9], [top[0] * 0.9, top[1] * 0.92, top[2]], 8, 4, 0.06, seed ^ 3);
                    foliage(false);
                } else {
                    // leaves: green, golden-red in autumn country, deep and vivid in the jungle
                    let g = if autumn {
                        [[0.84, 0.38, 0.10], [0.88, 0.62, 0.12], [0.68, 0.20, 0.12], [0.62, 0.58, 0.14]][((seed >> 6) % 4) as usize]
                    } else if jungle {
                        [0.06 + 0.04 * jit(6), 0.46 + 0.08 * jit(8), 0.13]
                    } else if wet > 0.62 {
                        [0.12 + 0.04 * jit(6), 0.52 + 0.06 * jit(8), 0.18]
                    } else {
                        [0.18 + 0.05 * jit(6), 0.44 + 0.07 * jit(8), 0.17]
                    };
                    let birch = !jungle && !autumn && (seed >> 5) % 100 < 22;
                    let height = if jungle { height * 1.25 } else { height };
                    let trunk_top = height * 0.5;
                    let bark = if birch { [0.88, 0.86, 0.8] } else { [0.34 + 0.04 * jit(4), 0.24, 0.15] };
                    limb(out, [x, 0.0, z], [x + 0.03 * jit(4) * height, trunk_top, z], trunk_r, trunk_r * 0.6, bark, 7);
                    let cr = (height * 0.16 + 0.26) * if jungle { 1.3 } else { 1.0 };
                    // a few branches reaching out under the crown
                    for k in 0..3u32 {
                        let a = k as f32 * 2.1 + jit(2);
                        limb(out, [x, trunk_top * 0.8, z], [x + a.cos() * cr * 0.8, trunk_top + cr * 0.3, z + a.sin() * cr * 0.8], trunk_r * 0.45, trunk_r * 0.2, bark, 5);
                    }
                    let g = if birch { [g[0] * 1.2, g[1] * 1.15, g[2] * 1.1] } else { g };
                    // the crown is a loose heap of leaf clusters, not one ball
                    foliage(true);
                    let lobes = 7 + (seed % 3);
                    for k in 0..lobes {
                        let a = k as f32 * 2.4 + jit(20 + k % 5) * 0.7;
                        let ring = if k == 0 { 0.0 } else { 0.55 + 0.25 * jit(30 + k % 7).abs() };
                        let up = if k == 0 { 1.15 } else { 0.55 + 0.7 * ((k * 5) % 7) as f32 / 7.0 };
                        let lr = cr * (0.5 + 0.2 * jit(40 + k % 6).abs());
                        let shade = 0.9 + 0.15 * ((k * 3) % 5) as f32 / 4.0;
                        sphere(out, [x + a.cos() * cr * ring, trunk_top + cr * up, z + a.sin() * cr * ring], [lr, lr * 0.8, lr], [g[0] * shade, g[1] * shade, g[2]], 7, 4, 0.18, seed ^ (k * 7 + 3));
                    }
                    foliage(false);
                }
            }
            NodeKind::BerryBush => {
                let g = [0.2, 0.44, 0.18];
                sphere(out, [x, 0.3, z], [0.42, 0.3, 0.4], g, 8, 5, 0.08, seed);
                sphere(out, [x + 0.22, 0.24, z + 0.12], [0.3, 0.22, 0.3], [g[0] * 1.1, g[1] * 1.05, g[2]], 7, 4, 0.08, seed ^ 2);
                for k in 0..5 {
                    let a = k as f32 * 1.25 + jit(9);
                    sphere(out, [x + a.cos() * 0.3, 0.32 + 0.1 * (k % 2) as f32, z + a.sin() * 0.3], [0.05; 3], [0.78, 0.1, 0.2], 5, 4, 0.0, 1);
                }
            }
            NodeKind::ClayDeposit => {
                let c = if known { [0.62, 0.40, 0.28] } else { [0.78, 0.70, 0.52] };
                sphere(out, [x, 0.0, z], [0.46, 0.14, 0.4], c, 9, 4, 0.08, seed);
                sphere(out, [x + 0.14, 0.05, z - 0.08], [0.26, 0.1, 0.24], [c[0] * 1.08, c[1] * 1.08, c[2] * 1.06], 7, 4, 0.08, seed ^ 6);
            }
            NodeKind::HerbPatch => {
                tuft(out, x, z, [0.30, 0.52, 0.24]);
                if known {
                    sphere(out, [x - 0.1, 0.4, z + 0.06], [0.05; 3], [0.88, 0.5, 0.7], 5, 4, 0.0, 1);
                    sphere(out, [x + 0.12, 0.36, z - 0.1], [0.05; 3], [0.96, 0.86, 0.4], 5, 4, 0.0, 1);
                    sphere(out, [x + 0.02, 0.44, z + 0.14], [0.045; 3], [0.88, 0.5, 0.7], 5, 4, 0.0, 1);
                } else if level >= 2 {
                    // a pale speck among the green: this one is not just grass
                    sphere(out, [x - 0.06, 0.38, z + 0.04], [0.04; 3], [0.82, 0.78, 0.7], 5, 4, 0.0, 1);
                }
            }
            NodeKind::CaveMouth => {
                // a craggy hillside with a dark arched mouth, mossy ledges and stalagmites by the entrance
                let g = 0.5 + 0.05 * jit(11);
                rock(out, [x, 0.7, z - 0.1], [1.25, 0.95, 1.05], [g, g, g + 0.02], seed);
                rock(out, [x - 0.8, 0.35, z + 0.2], [0.55, 0.4, 0.5], [g * 0.9, g * 0.9, g * 0.94], seed ^ 21);
                rock(out, [x + 0.95, 0.3, z - 0.1], [0.45, 0.34, 0.42], [g * 0.95, g * 0.95, g], seed ^ 22);
                // the mouth: a black hollow framed by an arch of stones
                sphere(out, [x + 0.15, 0.36, z + 0.78], [0.5, 0.46, 0.3], [0.02, 0.02, 0.03], 10, 6, 0.04, seed ^ 31);
                for k in 0..5u32 {
                    let a = std::f32::consts::PI * (k as f32 / 4.0);
                    rock(out, [x + 0.15 - a.cos() * 0.6, 0.3 + a.sin() * 0.62, z + 0.74], [0.17, 0.15, 0.16], [g * 1.05, g * 1.05, g * 1.08], seed ^ (41 + k));
                }
                // moss on the ledges
                for k in 0..4u32 {
                    sphere(out, [x - 0.6 + 0.4 * k as f32 + 0.1 * jit(k + 3), 1.15 - 0.12 * (k % 2) as f32, z - 0.15 + 0.2 * jit(k + 9)], [0.22, 0.07, 0.2], [0.26, 0.42, 0.2], 6, 3, 0.1, seed ^ (51 + k));
                }
                // stalagmites in front
                for (dx, h) in [(-0.35f32, 0.34f32), (0.62, 0.26)] {
                    limb(out, [x + dx, 0.0, z + 1.0], [x + dx, h, z + 1.0], 0.08, 0.012, [g * 0.95, g * 0.93, g], 5);
                }
            }
            NodeKind::FiberPlant => {
                // a tall clump of pale straw-gold stalks with seed heads: it stands out of the green grass
                let c = if known { [0.86, 0.78, 0.42] } else { [0.74, 0.70, 0.40] };
                let head = [0.62, 0.48, 0.26];
                for k in 0..14u32 {
                    let a = k as f32 * 0.449 + jit(5 + k % 4) * 0.4;
                    let lean = 0.18 + 0.14 * jit(k + 2).abs();
                    let hgt = 1.0 + 0.3 * jit(k + 1);
                    let top = [x + a.cos() * lean, hgt, z + a.sin() * lean];
                    limb(out, [x + a.cos() * 0.05, 0.0, z + a.sin() * 0.05], top, 0.03, 0.008, c, 4);
                    if k % 2 == 0 {
                        sphere(out, top, [0.035, 0.09, 0.035], head, 5, 3, 0.0, k);
                    }
                }
            }
            NodeKind::Deadwood => {
                // a heap of fallen branches, grey-silver with age, a few sticking up so it shows over the grass
                let c = if known { [0.62, 0.56, 0.48] } else { [0.56, 0.52, 0.46] };
                let dark = [c[0] * 0.75, c[1] * 0.72, c[2] * 0.7];
                limb(out, [x - 0.6, 0.08, z - 0.1], [x + 0.6, 0.1, z + 0.1], 0.07, 0.05, c, 6);
                limb(out, [x - 0.3, 0.1, z + 0.4], [x + 0.35, 0.12, z - 0.4], 0.06, 0.04, dark, 6);
                limb(out, [x + 0.2, 0.1, z + 0.3], [x - 0.45, 0.14, z - 0.25], 0.05, 0.035, c, 5);
                for k in 0..3u32 {
                    let a = k as f32 * 2.1 + jit(k + 3);
                    limb(out, [x + a.cos() * 0.2, 0.08, z + a.sin() * 0.2], [x + a.cos() * 0.45, 0.55 + 0.15 * jit(k + 6), z + a.sin() * 0.45], 0.035, 0.012, dark, 4);
                }
            }
        }
    }
}

/// Make what was drawn since `start` shine by itself (flames, embers, sparks).
fn glowify(out: &mut [Vertex], start: usize) {
    for v in &mut out[start..] {
        v.glow = 1.0;
        v.uv[1] = 0.0;
    }
}

/// A campfire: a ring of stones round an ash bed, a teepee of logs that sinks into a heap as the wood burns down,
/// and layered tongues of flame (red outside, yellow, a white heart) that lick and sway, with sparks rising and a
/// little smoke. The flames grow with the fuel left; a dead fire is charred wood smouldering in grey ash.
/// `tint` draws the whole thing in one flat colour (placement preview).
fn push_campfire(out: &mut Vec<Vertex>, x: f32, z: f32, fuel: f32, t: f32, tint: Option<[f32; 3]>) {
    use shapes::{limb, sphere};
    set_anchor(x, z);
    let fuel = fuel.clamp(0.0, 1.0);
    let lit = fuel > 0.0;
    let col = |c: [f32; 3]| tint.unwrap_or(c);
    let seed = ((x * 7.0 + z * 13.0).abs() as u32).wrapping_mul(2654435761);
    let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
    // the stone ring, uneven
    for k in 0..9u32 {
        let a = k as f32 * std::f32::consts::TAU / 9.0 + 0.2 * jit(k);
        let r = 0.42 + 0.03 * jit(k + 9);
        let sz = 0.1 + 0.035 * jit(k + 3).abs();
        let g = 0.5 + 0.08 * jit(k + 5);
        sphere(out, [x + a.cos() * r, sz * 0.6, z + a.sin() * r], [sz * 1.2, sz * 0.8, sz], col([g, g * 0.97, g * 0.93]), 7, 4, 0.18, seed ^ k);
    }
    // ash bed, scorched black at the middle
    sphere(out, [x, 0.0, z], [0.36, 0.035, 0.36], col([0.36, 0.34, 0.32]), 12, 3, 0.05, seed);
    sphere(out, [x, 0.012, z], [0.22, 0.03, 0.22], col([0.12, 0.1, 0.09]), 10, 3, 0.05, seed ^ 3);
    // logs: a teepee while there is wood to burn, slumping into a low heap as it goes
    let stand = if lit { 0.25 + 0.75 * fuel.sqrt() } else { 0.0 };
    for k in 0..5u32 {
        let a = k as f32 * std::f32::consts::TAU / 5.0 + 0.5 + 0.15 * jit(k + 20);
        let (ca, sa) = (a.cos(), a.sin());
        let foot = [x + ca * 0.32, 0.05, z + sa * 0.32];
        let tip = [x + ca * (0.08 + 0.12 * (1.0 - stand)), 0.06 + 0.26 * stand, z + sa * (0.08 + 0.12 * (1.0 - stand))];
        let mid = [(foot[0] + tip[0]) * 0.5, (foot[1] + tip[1]) * 0.5, (foot[2] + tip[2]) * 0.5];
        limb(out, foot, mid, 0.05, 0.046, col([0.42, 0.28, 0.16]), 6);
        // burnt black toward the flames
        limb(out, mid, tip, 0.04, 0.026, col([0.22, 0.12, 0.08]), 6);
        if lit && tint.is_none() {
            // glowing coals along the burnt end
            let s = out.len();
            let e = [mid[0] * 0.4 + tip[0] * 0.6, mid[1] * 0.4 + tip[1] * 0.6 + 0.02, mid[2] * 0.4 + tip[2] * 0.6];
            sphere(out, e, [0.03, 0.02, 0.03], [1.0, 0.32 + 0.1 * (t * 3.0 + k as f32).sin(), 0.08], 5, 3, 0.0, k);
            glowify(out, s);
        }
    }
    if tint.is_some() {
        // the preview shows where the flames will be
        let s = out.len();
        limb(out, [x, 0.08, z], [x, 0.6, z], 0.14, 0.02, col([1.0, 0.6, 0.2]), 6);
        glowify(out, s);
        return;
    }
    if !lit {
        // smouldering: a thin grey wisp curling up from the ash
        for k in 0..3u32 {
            let age = (t * 0.35 + k as f32 / 3.0).fract();
            let p = [x + (t * 0.9 + k as f32 * 2.0).sin() * 0.06 * (1.0 + age * 2.0), 0.15 + age * 1.3, z + (t * 0.7 + k as f32).cos() * 0.05];
            let r = 0.04 + 0.08 * age;
            let g = 0.55 + 0.1 * age;
            sphere(out, p, [r, r * 0.8, r], [g, g, g], 5, 3, 0.2, k);
        }
        return;
    }
    let size = 0.45 + 0.55 * fuel.sqrt();
    let s = out.len();
    // a bed of embers under the flames
    for k in 0..6u32 {
        let a = k as f32 * 1.05 + jit(k + 30);
        let r = 0.08 + 0.08 * jit(k + 40).abs();
        let pulse = 0.5 + 0.5 * (t * 4.0 + k as f32 * 1.9).sin();
        sphere(out, [x + a.cos() * r, 0.05, z + a.sin() * r], [0.05, 0.03, 0.05], [1.0, 0.25 + 0.25 * pulse, 0.05], 5, 3, 0.1, k);
    }
    // tongues of flame: each licks up and down at its own pace, the tips swaying
    let tongue = |out: &mut Vec<Vertex>, k: u32, ring: f32, base_r: f32, h: f32, c: [f32; 3]| {
        let a = k as f32 * 2.399 + jit(k + 50);
        let ph = t * (7.0 + 1.3 * (k % 3) as f32) + k as f32 * 1.7;
        let lick = 0.75 + 0.25 * ph.sin() + 0.12 * (ph * 2.3).sin();
        let hh = h * lick * size;
        let foot = [x + a.cos() * ring, 0.06, z + a.sin() * ring];
        let sway = |f: f32| [(ph * 0.7 + f).sin() * 0.07 * size, (ph * 0.6 + f + 1.0).cos() * 0.07 * size];
        let [s1x, s1z] = sway(0.0);
        let [s2x, s2z] = sway(1.4);
        let mid = [foot[0] * 0.6 + x * 0.4 + s1x * 0.5, 0.06 + hh * 0.45, foot[2] * 0.6 + z * 0.4 + s1z * 0.5];
        let tip = [x + s2x, 0.06 + hh, z + s2z];
        limb(out, foot, mid, base_r * size, base_r * 0.75 * size, c, 6);
        limb(out, mid, tip, base_r * 0.75 * size, 0.008, c, 6);
    };
    for k in 0..7 {
        tongue(out, k, 0.15, 0.1, 0.75, [1.0, 0.34, 0.07]);
    }
    for k in 7..11 {
        tongue(out, k, 0.08, 0.1, 1.0, [1.0, 0.64, 0.14]);
    }
    for k in 11..13 {
        tongue(out, k, 0.02, 0.075, 0.65, [1.0, 0.93, 0.62]);
    }
    // sparks drifting up out of the fire and winking out
    for k in 0..6u32 {
        let age = (t * 0.55 + k as f32 / 6.0 + 0.3 * jit(k + 60)).fract();
        let cyc = (t * 0.55 + k as f32 / 6.0).floor();
        let a = k as f32 * 2.1 + cyc * 1.3;
        let p = [x + a.cos() * 0.25 * age + (t * 3.0 + k as f32).sin() * 0.04, 0.4 + age * 1.8 * size, z + a.sin() * 0.25 * age];
        let r = 0.022 * (1.0 - age);
        if r > 0.004 {
            push_box(out, p, [r, r, r], [1.0, 0.75 - 0.4 * age, 0.25], 1.0);
        }
    }
    glowify(out, s);
    // a faint plume of smoke above the flames
    for k in 0..4u32 {
        let age = (t * 0.22 + k as f32 / 4.0).fract();
        // smoke leans off downwind, the more the stronger it blows
        let [wx, wz, _] = WIND.with(|w| w.get());
        let p = [x + (t * 0.5 + k as f32 * 1.7).sin() * 0.12 * age + wx * 1.6 * age * age, 0.9 * size + age * 2.2 * (1.0 - 0.4 * (wx * wx + wz * wz).sqrt()), z + (t * 0.4 + k as f32).cos() * 0.1 * age + wz * 1.6 * age * age];
        let r = (0.06 + 0.2 * age) * (1.0 - age * 0.5);
        let g = 0.42 + 0.2 * age;
        sphere(out, p, [r, r * 0.85, r], [g, g, g * 1.02], 6, 4, 0.25, k);
    }
}

/// Building models from simple boxes. `tint` turns it into a flat bright preview colour.
fn push_building(out: &mut Vec<Vertex>, kind: BuildingKind, x: f32, z: f32, active: bool, tick: u64, tint: Option<[f32; 3]>) {
    set_anchor(x, z);
    fn round(out: &mut Vec<Vertex>, c: [f32; 3], r: [f32; 3], col: [f32; 3], tint: Option<[f32; 3]>) {
        match tint {
            Some(t) => shapes::sphere(out, c, r, t, 12, 8, 0.0, 11),
            None => shapes::sphere(out, c, r, col, 12, 8, 0.02, 11),
        }
    }
    let bx = |out: &mut Vec<Vertex>, base: [f32; 3], half: [f32; 3], col: [f32; 3], glow: f32| match tint {
        Some(t) => push_box(out, base, half, t, 1.0),
        None => push_box(out, base, half, col, glow),
    };
    match kind {
        BuildingKind::Campfire => {
            push_campfire(out, x, z, 1.0, tick as f32 / 60.0, tint);
        }
        BuildingKind::Shelter => {
            for (sx, sz) in [(-1.1, -0.9), (1.1, -0.9), (-1.1, 0.9), (1.1, 0.9)] {
                bx(out, [x + sx, 0.0, z + sz], [0.06, 1.3, 0.06], [0.40, 0.27, 0.15], 0.0);
            }
            round(out, [x, 1.4, z], [1.6, 0.28, 1.4], [0.25, 0.40, 0.18], tint);
            bx(out, [x, 0.0, z - 0.95], [1.1, 0.9, 0.05], [0.32, 0.30, 0.16], 0.0);
        }
        BuildingKind::CharcoalKiln => {
            round(out, [x, 0.4, z], [0.9, 0.5, 0.9], [0.25, 0.17, 0.12], tint);
            round(out, [x, 0.8, z], [0.55, 0.3, 0.55], [0.20, 0.14, 0.10], tint);
            if active {
                bx(out, [x, 0.7, z], [0.16, 0.10, 0.16], [1.0, 0.45, 0.10], 1.0);
            }
        }
        BuildingKind::Depot => {
            bx(out, [x, 0.0, z], [0.85, 0.55, 0.85], [0.45, 0.32, 0.18], 0.0);
            bx(out, [x, 0.55, z], [0.95, 0.08, 0.95], [0.34, 0.24, 0.14], 0.0);
            round(out, [x - 0.35, 0.85, z + 0.2], [0.26, 0.28, 0.26], [0.55, 0.42, 0.24], tint);
            round(out, [x + 0.3, 0.82, z - 0.25], [0.24, 0.24, 0.24], [0.60, 0.46, 0.28], tint);
            round(out, [x + 0.35, 0.8, z + 0.35], [0.2, 0.18, 0.2], [0.5, 0.4, 0.3], tint);
        }
        BuildingKind::Workshop => {
            bx(out, [x, 0.0, z], [1.1, 0.9, 0.9], [0.50, 0.46, 0.42], 0.0);
            round(out, [x, 1.0, z], [1.35, 0.32, 1.1], [0.28, 0.22, 0.20], tint);
            bx(out, [x + 0.5, 0.9, z + 0.3], [0.12, 0.7, 0.12], [0.35, 0.33, 0.34], 0.0);
            if active {
                bx(out, [x + 0.5, 1.6, z + 0.3], [0.14, 0.08, 0.14], [1.0, 0.45, 0.10], 1.0);
                bx(out, [x, 0.1, z + 0.92], [0.5, 0.35, 0.03], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::WaterWheel => {
            bx(out, [x, 0.0, z], [0.9, 0.35, 0.5], [0.45, 0.32, 0.18], 0.0);
            // spokes of the wheel, turning with the day clock is overkill: draw it upright
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0 + tick as f32 * if active { 0.02 } else { 0.0 };
                shapes::limb(out, [x, 1.05, z], [x + a.cos() * 1.0, 1.05 + a.sin() * 1.0, z], 0.06, 0.09, [0.55, 0.40, 0.22], 6);
            }
            round(out, [x, 1.05, z], [0.18, 0.18, 0.6], [0.35, 0.35, 0.40], tint);
            bx(out, [x, 0.2, z], [0.25, 0.25, 0.7], [0.35, 0.35, 0.40], 0.0);
        }
        BuildingKind::Workbench => {
            bx(out, [x, 0.5, z], [0.9, 0.12, 0.5], [0.52, 0.36, 0.20], 0.0);
            for (dx, dz) in [(-0.38, -0.2), (0.38, -0.2), (-0.38, 0.2), (0.38, 0.2)] {
                bx(out, [x + dx, 0.0, z + dz], [0.08, 0.5, 0.08], [0.40, 0.28, 0.16], 0.0);
            }
            bx(out, [x - 0.2, 0.62, z], [0.2, 0.08, 0.14], [0.62, 0.62, 0.66], 0.0);
            if active {
                bx(out, [x + 0.25, 0.62, z], [0.14, 0.05, 0.14], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::Field => {
            // broken ground in furrows, a stake at each corner; green shoots while a crop grows
            for k in 0..5 {
                let fz = z - 1.0 + k as f32 * 0.5;
                bx(out, [x, 0.0, fz], [1.2, 0.06, 0.16], [0.30, 0.20, 0.12], 0.0);
                if active {
                    for j in 0..7 {
                        let fx = x - 1.05 + j as f32 * 0.35;
                        let h = 0.18 + 0.05 * (((j * 3 + k * 5) % 4) as f32);
                        bx(out, [fx, 0.06, fz], [0.025, h, 0.025], [0.42, 0.66, 0.24], 0.0);
                    }
                }
            }
            for (sx, sz) in [(-1.35, -1.25), (1.35, -1.25), (-1.35, 1.25), (1.35, 1.25)] {
                bx(out, [x + sx, 0.0, z + sz], [0.05, 0.45, 0.05], [0.42, 0.29, 0.16], 0.0);
            }
        }
        BuildingKind::Bloomery => {
            // a tapering clay shaft with a glowing mouth
            round(out, [x, 0.6, z], [0.55, 0.7, 0.55], [0.58, 0.40, 0.30], tint);
            round(out, [x, 1.35, z], [0.32, 0.3, 0.32], [0.55, 0.38, 0.28], tint);
            if active {
                bx(out, [x, 0.1, z + 0.5], [0.16, 0.18, 0.04], [1.0, 0.55, 0.15], 1.0);
            }
        }
        BuildingKind::Bellows => {
            bx(out, [x, 0.0, z], [0.4, 0.12, 0.25], [0.45, 0.31, 0.17], 0.0);
            round(out, [x, 0.25, z], [0.35, 0.12, 0.22], [0.55, 0.40, 0.26], tint);
            shapes::limb(out, [x + 0.35, 0.2, z], [x + 0.9, 0.15, z], 0.05, 0.03, [0.35, 0.33, 0.34], 4);
        }
        BuildingKind::TripHammer => {
            bx(out, [x, 0.0, z], [0.8, 0.5, 0.3], [0.48, 0.33, 0.18], 0.0);
            let lift = if active { ((tick as f32) * 0.2).sin().abs() * 0.3 } else { 0.0 };
            shapes::limb(out, [x - 0.7, 0.9, z], [x + 0.5, 0.75 + lift, z], 0.08, 0.06, [0.50, 0.35, 0.19], 5);
            bx(out, [x + 0.5, 0.5 + lift, z], [0.15, 0.15, 0.15], [0.35, 0.34, 0.36], 0.0);
            bx(out, [x + 0.5, 0.0, z], [0.2, 0.4, 0.2], [0.40, 0.40, 0.42], 0.0);
        }
        BuildingKind::IronAnvil => {
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.42, 0.12, 0.2], [0.30, 0.30, 0.34], 0.0);
            bx(out, [x + 0.45, 0.55, z], [0.12, 0.06, 0.08], [0.30, 0.30, 0.34], 0.0);
        }
        BuildingKind::BlastFurnace => {
            bx(out, [x, 0.0, z], [0.7, 1.2, 0.7], [0.66, 0.56, 0.46], 0.0);
            bx(out, [x, 2.4, z], [0.45, 0.8, 0.45], [0.62, 0.52, 0.42], 0.0);
            if active {
                bx(out, [x, 0.15, z + 0.71], [0.2, 0.2, 0.02], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::SteelAnvil => {
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.44, 0.12, 0.2], [0.62, 0.66, 0.72], 0.0);
        }
        BuildingKind::Windmill => {
            bx(out, [x, 0.0, z], [0.5, 1.6, 0.5], [0.52, 0.40, 0.26], 0.0);
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::TAU / 4.0 + tick as f32 * 0.02;
                shapes::limb(out, [x, 2.8, z + 0.55], [x + a.cos() * 1.8, 2.8 + a.sin() * 1.8, z + 0.55], 0.05, 0.05, [0.85, 0.82, 0.70], 4);
            }
        }
        BuildingKind::Conveyor => {
            bx(out, [x, 0.0, z], [1.2, 0.35, 0.3], [0.45, 0.31, 0.17], 0.0);
            bx(out, [x, 0.35, z], [1.2, 0.03, 0.26], [0.30, 0.26, 0.22], 0.0);
        }
        BuildingKind::SteamEngine => {
            // a brick firebox, a round boiler on it, a stack, a flywheel
            bx(out, [x, 0.0, z], [0.8, 0.4, 0.5], [0.66, 0.56, 0.46], 0.0);
            round(out, [x, 0.75, z], [0.8, 0.35, 0.35], [0.30, 0.30, 0.34], tint);
            bx(out, [x - 0.6, 1.0, z], [0.1, 0.9, 0.1], [0.25, 0.25, 0.28], 0.0);
            let a = tick as f32 * if active { 0.08 } else { 0.0 };
            for k in 0..6 {
                let t = a + k as f32 * std::f32::consts::TAU / 6.0;
                shapes::limb(out, [x + 0.9, 0.8, z + 0.5], [x + 0.9 + t.cos() * 0.6, 0.8 + t.sin() * 0.6, z + 0.5], 0.04, 0.04, [0.30, 0.30, 0.34], 4);
            }
            if active {
                bx(out, [x, 0.1, z + 0.51], [0.2, 0.15, 0.02], [1.0, 0.5, 0.15], 1.0);
            }
        }
        BuildingKind::Pump => {
            bx(out, [x, 0.0, z], [0.3, 0.8, 0.3], [0.30, 0.30, 0.34], 0.0);
            let lift = if active { ((tick as f32) * 0.1).sin() * 0.2 } else { 0.0 };
            shapes::limb(out, [x - 0.8, 1.2 + lift, z], [x + 0.8, 1.2 - lift, z], 0.06, 0.06, [0.45, 0.31, 0.17], 4);
        }
        BuildingKind::RollingMill => {
            bx(out, [x, 0.0, z], [0.8, 0.45, 0.4], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 0.6, z], [0.7, 0.14, 0.14], [0.55, 0.56, 0.60], tint);
            round(out, [x, 0.85, z], [0.7, 0.14, 0.14], [0.55, 0.56, 0.60], tint);
        }
        BuildingKind::Dynamo => {
            bx(out, [x, 0.0, z], [0.6, 0.15, 0.4], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 0.5, z], [0.4, 0.35, 0.35], [0.70, 0.42, 0.24], tint);
        }
        BuildingKind::Pole => {
            bx(out, [x, 0.0, z], [0.06, 1.8, 0.06], [0.42, 0.29, 0.16], 0.0);
            bx(out, [x, 3.5, z], [0.5, 0.04, 0.04], [0.42, 0.29, 0.16], 0.0);
        }
        BuildingKind::ElectricMotor => {
            round(out, [x, 0.35, z], [0.35, 0.3, 0.3], [0.30, 0.34, 0.40], tint);
            shapes::limb(out, [x + 0.35, 0.35, z], [x + 0.7, 0.35, z], 0.05, 0.05, [0.55, 0.56, 0.60], 4);
        }
        BuildingKind::ElectricLamp => {
            bx(out, [x, 0.0, z], [0.05, 1.2, 0.05], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 2.5, z], [0.18, 0.2, 0.18], [1.0, 0.95, 0.7], tint);
        }
        BuildingKind::Computer => {
            bx(out, [x, 0.0, z], [0.6, 0.45, 0.4], [0.80, 0.78, 0.72], 0.0);
            bx(out, [x, 0.9, z], [0.45, 0.35, 0.05], [0.20, 0.30, 0.25], 0.0);
            bx(out, [x, 0.95, z + 0.06], [0.38, 0.28, 0.01], [0.35, 0.9, 0.5], 0.8);
        }
        BuildingKind::Ruin => {
            // tumbled blocks, half sunk, moss on them
            for (dx, dz, h) in [(-0.5f32, -0.3f32, 0.4f32), (0.4, -0.4, 0.25), (0.2, 0.5, 0.55), (-0.4, 0.4, 0.2)] {
                round(out, [x + dx, h * 0.5, z + dz], [0.35, h, 0.3], [0.48, 0.48, 0.44], tint);
            }
            round(out, [x, 0.05, z], [0.9, 0.06, 0.9], [0.30, 0.42, 0.22], tint);
        }
        BuildingKind::Cellar => {
            round(out, [x, 0.2, z], [0.9, 0.35, 0.9], [0.32, 0.44, 0.24], tint);
            bx(out, [x, 0.0, z + 0.85], [0.35, 0.45, 0.06], [0.40, 0.28, 0.16], 0.0);
        }
        BuildingKind::MineShaft => {
            for (dx, dz) in [(-0.6f32, -0.6f32), (0.6, -0.6), (-0.6, 0.6), (0.6, 0.6)] {
                bx(out, [x + dx, 0.0, z + dz], [0.06, 1.2, 0.06], [0.42, 0.29, 0.16], 0.0);
            }
            bx(out, [x, 2.4, z], [0.7, 0.06, 0.7], [0.45, 0.31, 0.17], 0.0);
            round(out, [x, 0.02, z], [0.5, 0.02, 0.5], [0.08, 0.07, 0.07], tint);
        }
        BuildingKind::Anvil => {
            // a dressed block of stone on a stump
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.38, 0.12, 0.22], [0.48, 0.47, 0.50], 0.0);
        }
        BuildingKind::HandMill => {
            // two round stones, the upper one turning, on a plank stand
            bx(out, [x, 0.0, z], [0.6, 0.35, 0.6], [0.52, 0.36, 0.20], 0.0);
            round(out, [x, 0.42, z], [0.5, 0.1, 0.5], [0.55, 0.54, 0.52], tint);
            let a = tick as f32 * if active { 0.05 } else { 0.0 };
            round(out, [x, 0.58, z], [0.48, 0.09, 0.48], [0.62, 0.61, 0.58], tint);
            shapes::limb(out, [x + a.cos() * 0.35, 0.66, z + a.sin() * 0.35], [x + a.cos() * 0.35, 0.95, z + a.sin() * 0.35], 0.04, 0.04, [0.42, 0.29, 0.16], 4);
        }
        BuildingKind::AnimalGin => {
            // an upright post and a long sweep
            bx(out, [x, 0.0, z], [0.12, 1.1, 0.12], [0.45, 0.31, 0.17], 0.0);
            let a = tick as f32 * 0.01;
            shapes::limb(out, [x, 0.95, z], [x + a.cos() * 2.4, 0.9, z + a.sin() * 2.4], 0.06, 0.05, [0.50, 0.35, 0.19], 5);
        }
        BuildingKind::PitKiln => {
            // a shallow pit ringed with earth, a mound of straw and logs over the pots; smoke and a glow while firing
            round(out, [x, 0.05, z], [0.95, 0.12, 0.95], [0.30, 0.22, 0.14], tint);
            round(out, [x, 0.25, z], [0.7, 0.28, 0.7], [0.70, 0.62, 0.34], tint);
            for k in 0..5 {
                let a = k as f32 * std::f32::consts::TAU / 5.0;
                shapes::limb(out, [x + a.cos() * 0.75, 0.15, z + a.sin() * 0.75], [x + a.cos() * 0.1, 0.55, z + a.sin() * 0.1], 0.07, 0.06, [0.42, 0.29, 0.16], 5);
            }
            if active {
                bx(out, [x, 0.42, z], [0.22, 0.06, 0.22], [1.0, 0.45, 0.10], 1.0);
            }
        }
        BuildingKind::Pen => {
            // a ring of posts with two rails, open on the near side for the gate, a trough in the middle
            let r = 3.2f32;
            let n = 16;
            for k in 0..n {
                if k == 4 {
                    continue; // the gate
                }
                let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
                let (p0, p1) = ([x + r * a0.cos(), z + r * a0.sin()], [x + r * a1.cos(), z + r * a1.sin()]);
                bx(out, [p0[0], 0.0, p0[1]], [0.05, 0.55, 0.05], [0.42, 0.29, 0.16], 0.0);
                if k != 3 {
                    for h in [0.3f32, 0.75] {
                        shapes::limb(out, [p0[0], h, p0[1]], [p1[0], h, p1[1]], 0.035, 0.035, [0.50, 0.35, 0.19], 4);
                    }
                }
            }
            bx(out, [x, 0.0, z], [0.5, 0.18, 0.22], [0.40, 0.28, 0.16], 0.0);
            if active {
                bx(out, [x, 0.18, z], [0.42, 0.04, 0.16], [0.86, 0.72, 0.36], 0.0);
            }
        }
        BuildingKind::Loom => {
            // a warp-weighted loom: two posts, a beam across the top, threads hanging to clay weights
            for sx in [-0.7f32, 0.7] {
                bx(out, [x + sx, 0.0, z], [0.06, 1.0, 0.06], [0.42, 0.29, 0.16], 0.0);
            }
            bx(out, [x, 1.9, z], [0.82, 0.06, 0.07], [0.48, 0.33, 0.18], 0.0);
            for k in 0..9 {
                let tx = x - 0.56 + k as f32 * 0.14;
                bx(out, [tx, 0.45, z], [0.012, 0.72, 0.012], [0.82, 0.80, 0.66], 0.0);
                round(out, [tx, 0.4, z], [0.06, 0.07, 0.06], [0.62, 0.38, 0.26], tint);
            }
            // the woven part grows down from the beam while it works
            let woven = if active { 0.5 } else { 0.25 };
            bx(out, [x, 1.84 - 2.0 * woven, z + 0.01], [0.6, woven, 0.02], [0.78, 0.72, 0.52], 0.0);
        }
        BuildingKind::Furnace => {
            round(out, [x, 0.55, z], [0.7, 0.62, 0.7], [0.55, 0.53, 0.50], tint);
            round(out, [x, 1.0, z], [0.45, 0.35, 0.45], [0.5, 0.48, 0.46], tint);
            bx(out, [x, 0.0, z + 0.6], [0.25, 0.35, 0.04], [0.08, 0.07, 0.07], 0.0);
            if active {
                bx(out, [x, 1.0, z], [0.25, 0.10, 0.25], [1.0, 0.45, 0.10], 1.0);
                bx(out, [x, 0.0, z + 0.62], [0.2, 0.3, 0.03], [1.0, 0.5, 0.15], 1.0);
            }
        }
    }
}

/// A flat ribbon on the ground from `a` to `b`.
fn push_segment(out: &mut Vec<Vertex>, a: [f32; 2], b: [f32; 2], width: f32, y: f32, col: [f32; 3], glow: f32) {
    set_anchor(a[0], a[1]);
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-4 {
        return;
    }
    let (nx, nz) = (-dz / len * width / 2.0, dx / len * width / 2.0);
    let q = [[a[0] + nx, y, a[1] + nz], [a[0] - nx, y, a[1] - nz], [b[0] - nx, y, b[1] - nz], [b[0] + nx, y, b[1] + nz]];
    for i in [0, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0] {
        out.push(v(q[i], col, glow));
    }
}

/// The route an agent is walking: a line along its path to the target, with a pulsing flag at the end.
fn push_route(out: &mut Vec<Vertex>, world: &World, a: &alaz_sim::world::Agent, tick: u64) {
    use alaz_sim::world::Task;
    let target = match a.task {
        Task::Move { x, y } => Some((x, y)),
        Task::Gather { node, .. } => world.nodes.get(node.0 as usize).map(|n| n.center_mm()),
        Task::Drink { tx, ty } => Some((tx as i64 * 1000 + 500, ty as i64 * 1000 + 500)),
        Task::Tame { wild, .. } | Task::Hunt { wild, .. } => world.wild.iter().find(|w| w.id == wild).map(|w| (w.x, w.y)),
        _ => None,
    };
    let Some((tx, ty)) = target else { return };
    let m = |v: i64| v as f32 / 1000.0;
    let mut pts: Vec<[f32; 2]> = vec![[m(a.x), m(a.y)]];
    for &(px, py) in a.path.iter().rev() {
        pts.push([px as f32 + 0.5, py as f32 + 0.5]);
    }
    pts.push([m(tx), m(ty)]);
    let col = [1.0, 0.86, 0.25];
    for w in pts.windows(2) {
        push_segment(out, w[0], w[1], 0.14, 0.07, col, 0.7);
        // dashes between the nodes make the direction readable
        let (dx, dz) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let len = (dx * dx + dz * dz).sqrt();
        let n = (len / 1.2) as i32;
        let flow = (tick as f32 / 20.0).fract();
        for i in 0..n {
            let t = ((i as f32 + flow) * 1.2 / len).min(1.0);
            push_box(out, [w[0][0] + dx * t, 0.08, w[0][1] + dz * t], [0.1, 0.05, 0.1], [1.0, 0.98, 0.7], 1.0);
        }
    }
    let end = *pts.last().unwrap();
    // going to a resource: no flag, the resource itself glows (drawn by the caller)
    if matches!(a.task, Task::Gather { .. }) {
        return;
    }
    let pulse = 0.5 + 0.5 * (tick as f32 / 12.0).sin();
    push_box(out, [end[0], 0.0, end[1]], [0.04, 1.3, 0.04], [0.9, 0.85, 0.6], 0.5);
    push_box(out, [end[0] + 0.2, 1.0, end[1]], [0.22, 0.28, 0.03], [1.0, 0.3 + 0.4 * pulse, 0.2], 1.0);
    push_box(out, [end[0], 0.0, end[1]], [0.32 + 0.12 * pulse, 0.03, 0.32 + 0.12 * pulse], col, 1.0);
}

/// Where in a work cycle the blow lands (the tool meets the wood, the hands reach the mouth, ...).
const IMPACT: f32 = 0.62;

/// How far through its work an agent is, as whole cycles plus a fraction (blows land at fraction `IMPACT`).
/// Gathering is tied to the simulation's own progress so that each unit arrives exactly on a blow; a throw winds
/// up over the ticks it takes to land; a craft strike plays when the player strikes.
fn work_cycle(world: &World, a: &alaz_sim::world::Agent, work: Work, w: &mut Walk, alpha: f32, secs: f32, dt: f32) -> f32 {
    use alaz_sim::world::Task;
    // ALAZ_DEMO_U freezes every work motion at one moment of its cycle, for screenshots
    static FREEZE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(u) = *FREEZE.get_or_init(|| std::env::var("ALAZ_DEMO_U").ok().and_then(|v| v.parse().ok())) {
        return u;
    }
    let off = (a.id.0 % 7) as f32 * 0.13;
    match (work, a.task) {
        (Work::Chop | Work::Mine | Work::Dig | Work::Pick, Task::Gather { node, progress }) => {
            let Some(n) = world.nodes.get(node.0 as usize) else { return 0.0 };
            let need = alaz_sim::world::gather_need(n.kind, &a.inventory, world.perception_level()) as f32;
            // seconds one blow should take, so a long harvest is several quick blows rather than one slow one
            let beat = match work {
                Work::Chop => 0.7,
                Work::Mine => 0.8,
                Work::Dig => 0.85,
                _ => 0.55,
            };
            let blows = (need / alaz_sim::time::TICKS_PER_SECOND as f32 / beat).round().max(1.0);
            ((progress as f32 + alpha) / need).min(1.0) * blows + IMPACT
        }
        (Work::Mine, Task::Split { progress, .. }) => {
            // one blow for each piece that breaks off
            let need = alaz_sim::world::split_need(&a.inventory, world.perception_level()) as f32;
            ((progress as f32 + alpha) / need).min(1.0) + IMPACT
        }
        (Work::Pick, Task::Pick { progress, .. }) => ((progress as f32 + alpha) / alaz_sim::world::PICK_TICKS as f32).min(1.0) + IMPACT - 0.5,
        (Work::Throw, Task::Hunt { progress, .. }) => IMPACT * ((progress as f32 + alpha) / alaz_sim::world::HUNT_THROW_TICKS as f32).min(1.0),
        (Work::Hammer, Task::Craft { step, done, hits, .. }) => {
            let key = (step as u64) << 16 | (done as u64) << 8 | hits as u64 | 1 << 40;
            if key != w.craft {
                if w.craft != 0 {
                    w.strike = 0.0;
                }
                w.craft = key;
            }
            w.strike += dt;
            // a quick downstroke from the raised hand, the knock, and back up; at rest the hand waits raised
            let s = w.strike;
            if s < 0.07 {
                0.4 + (IMPACT - 0.4) * s / 0.07
            } else if s < 0.45 {
                IMPACT + (1.0 - IMPACT) * (s - 0.07) / 0.38
            } else {
                0.1 + 0.05 * (secs * 2.0).sin()
            }
        }
        (Work::Drink, _) => secs / 1.3 + off,
        (Work::Offer, _) => secs / 1.6 + off,
        _ => 0.0,
    }
}

/// Keyframed motion: `k` holds (time 0..1, values); values ease between neighbouring keys.
fn keys<const N: usize>(u: f32, k: &[(f32, [f32; N])]) -> [f32; N] {
    let i = k.iter().rposition(|(t, _)| *t <= u).unwrap_or(0).min(k.len() - 2);
    let (t0, a) = k[i];
    let (t1, b) = k[i + 1];
    let s = ((u - t0) / (t1 - t0).max(1e-4)).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    std::array::from_fn(|j| a[j] + (b[j] - a[j]) * s)
}

/// Whole-body pose for a moment of work. Hands and the tool are in the frame of the turned upper body
/// (forward, up, side); feet in the frame of the hips.
struct Act {
    crouch: f32,
    lean: f32,
    /// Upper body turned right of the hips (radians).
    twist: f32,
    /// Right hand, left hand (forward, height, side).
    hands: [[f32; 3]; 2],
    /// Which way the held tool points from the right hand.
    dir: [f32; 3],
    /// Head nudged forward and up, and turned (radians) relative to the shoulders.
    head: [f32; 3],
    /// Feet (forward, side), left then right, and whether that leg kneels.
    feet: [[f32; 2]; 2],
    kneel: [bool; 2],
}

/// The pose of each kind of work at fraction `u` of its cycle. Fast strikes, a held moment at the blow, and a
/// slower wind-up, so every action reads at a glance.
fn act(w: Work, u: f32) -> Act {
    // key layout: crouch, lean, twist, right hand xyz, left hand xyz, tool dir xyz, head f/up/yaw
    type K = [f32; 15];
    let pick = |k: K| -> Act {
        Act {
            crouch: k[0],
            lean: k[1],
            twist: k[2],
            hands: [[k[3], k[4], k[5]], [k[6], k[7], k[8]]],
            dir: [k[9], k[10], k[11]],
            head: [k[12], k[13], k[14]],
            feet: [[0.0, -0.11], [0.0, 0.11]],
            kneel: [false, false],
        }
    };
    match w {
        Work::Chop => {
            // a diagonal two-handed swing from over the right shoulder, the whole body turning into it
            let k: [(f32, K); 6] = [
                (0.0, [0.06, 0.1, 0.25, 0.25, 1.18, 0.16, 0.0, 0.0, 0.0, 0.25, 0.92, 0.2, 0.0, 0.0, -0.2]),
                (0.45, [0.02, -0.04, 0.85, -0.04, 1.62, 0.2, 0.0, 0.0, 0.0, -0.6, 0.72, 0.25, -0.02, 0.01, -0.65]),
                (0.54, [0.02, -0.06, 0.92, -0.06, 1.64, 0.2, 0.0, 0.0, 0.0, -0.72, 0.6, 0.3, -0.02, 0.01, -0.7]),
                (IMPACT, [0.14, 0.3, -0.12, 0.56, 1.04, -0.02, 0.0, 0.0, 0.0, 0.95, 0.02, -0.3, 0.04, -0.02, 0.08]),
                (0.74, [0.12, 0.26, -0.08, 0.52, 1.07, 0.0, 0.0, 0.0, 0.0, 0.95, 0.07, -0.26, 0.03, -0.01, 0.05]),
                (1.0, [0.06, 0.1, 0.25, 0.25, 1.18, 0.16, 0.0, 0.0, 0.0, 0.25, 0.92, 0.2, 0.0, 0.0, -0.2]),
            ];
            Act { feet: [[0.22, -0.2], [-0.14, 0.2]], ..pick(keys(u, &k)) }
        }
        Work::Mine => {
            // high over the head, back arched, then a slam down into the rock bending the knees
            let k: [(f32, K); 6] = [
                (0.0, [0.05, 0.12, 0.0, 0.32, 1.2, 0.04, 0.0, 0.0, 0.0, 0.45, 0.9, 0.0, 0.0, 0.0, 0.0]),
                (0.45, [0.0, -0.14, 0.12, -0.06, 1.95, 0.04, 0.0, 0.0, 0.0, -0.75, 0.66, 0.0, -0.04, 0.03, -0.1]),
                (0.53, [0.0, -0.16, 0.12, -0.09, 1.97, 0.04, 0.0, 0.0, 0.0, -0.86, 0.5, 0.0, -0.04, 0.03, -0.1]),
                (IMPACT, [0.24, 0.44, 0.0, 0.56, 0.84, 0.03, 0.0, 0.0, 0.0, 0.55, -0.83, 0.0, 0.06, -0.06, 0.0]),
                (0.76, [0.22, 0.42, 0.0, 0.53, 0.87, 0.03, 0.0, 0.0, 0.0, 0.6, -0.8, 0.0, 0.05, -0.05, 0.0]),
                (1.0, [0.05, 0.12, 0.0, 0.32, 1.2, 0.04, 0.0, 0.0, 0.0, 0.45, 0.9, 0.0, 0.0, 0.0, 0.0]),
            ];
            Act { feet: [[0.14, -0.19], [-0.06, 0.19]], ..pick(keys(u, &k)) }
        }
        Work::Dig => {
            // squatting: both hands scoop the ground, haul the clay up and toss it to the side
            let k: [(f32, K); 7] = [
                (0.0, [0.38, 0.3, 0.0, 0.3, 0.74, 0.12, 0.3, 0.74, -0.12, 0.0, 1.0, 0.0, 0.03, -0.03, 0.0]),
                (0.45, [0.42, 0.52, 0.0, 0.6, 0.16, 0.1, 0.6, 0.16, -0.1, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (IMPACT, [0.45, 0.56, -0.05, 0.56, 0.1, 0.08, 0.56, 0.1, -0.08, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (0.8, [0.36, 0.3, 0.5, 0.25, 0.86, 0.34, 0.25, 0.84, 0.18, 0.0, 1.0, 0.0, 0.02, 0.0, -0.3]),
                (0.87, [0.34, 0.24, 0.75, 0.12, 1.02, 0.62, 0.15, 0.96, 0.44, 0.0, 1.0, 0.0, 0.0, 0.0, -0.4]),
                (0.92, [0.35, 0.26, 0.6, 0.18, 0.95, 0.5, 0.2, 0.9, 0.32, 0.0, 1.0, 0.0, 0.0, 0.0, -0.3]),
                (1.0, [0.38, 0.3, 0.0, 0.3, 0.74, 0.12, 0.3, 0.74, -0.12, 0.0, 1.0, 0.0, 0.03, -0.03, 0.0]),
            ];
            Act { feet: [[0.1, -0.24], [0.0, 0.24]], ..pick(keys(u, &k)) }
        }
        Work::Pick => {
            // bent over the bush, hands taking turns: one plucks and pulls to the pouch while the other reaches in
            let k: [(f32, K); 5] = [
                (0.0, [0.12, 0.5, -0.15, 0.64, 0.62, 0.12, 0.14, 0.88, -0.26, 0.0, 1.0, 0.0, 0.05, -0.05, 0.1]),
                (0.3, [0.13, 0.52, -0.12, 0.66, 0.58, 0.1, 0.4, 0.75, -0.2, 0.0, 1.0, 0.0, 0.05, -0.05, 0.05]),
                (IMPACT, [0.12, 0.46, 0.18, 0.12, 0.9, 0.27, 0.64, 0.62, -0.12, 0.0, 1.0, 0.0, 0.05, -0.05, -0.1]),
                (0.8, [0.13, 0.52, 0.12, 0.4, 0.75, 0.2, 0.66, 0.58, -0.1, 0.0, 1.0, 0.0, 0.05, -0.05, -0.05]),
                (1.0, [0.12, 0.5, -0.15, 0.64, 0.62, 0.12, 0.14, 0.88, -0.26, 0.0, 1.0, 0.0, 0.05, -0.05, 0.1]),
            ];
            Act { feet: [[0.1, -0.15], [-0.06, 0.15]], ..pick(keys(u, &k)) }
        }
        Work::Drink => {
            // down on one knee: cup the water, lift it to the mouth and tip the head back
            let k: [(f32, K); 5] = [
                (0.0, [0.45, 0.4, 0.0, 0.6, 0.1, 0.06, 0.6, 0.1, -0.06, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
                (0.3, [0.46, 0.42, 0.0, 0.63, 0.08, 0.05, 0.63, 0.08, -0.05, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
                (IMPACT, [0.42, 0.26, 0.0, 0.56, 1.08, 0.05, 0.56, 1.08, -0.05, 0.0, 1.0, 0.0, -0.02, 0.02, 0.0]),
                (0.85, [0.4, 0.14, 0.0, 0.46, 1.16, 0.05, 0.46, 1.16, -0.05, 0.0, 1.0, 0.0, -0.07, 0.03, 0.0]),
                (1.0, [0.45, 0.4, 0.0, 0.6, 0.1, 0.06, 0.6, 0.1, -0.06, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
            ];
            Act { feet: [[0.3, -0.13], [-0.36, 0.12]], kneel: [false, true], ..pick(keys(u, &k)) }
        }
        Work::Hammer => {
            // kneeling over the work: the left hand holds the core low, the right knocks it with a hammerstone
            let k: [(f32, K); 5] = [
                (0.0, [0.42, 0.36, 0.05, 0.36, 0.86, 0.15, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (0.4, [0.41, 0.33, 0.08, 0.3, 0.98, 0.17, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (IMPACT, [0.45, 0.44, -0.04, 0.5, 0.5, 0.0, 0.5, 0.4, -0.06, 0.0, 1.0, 0.0, 0.08, -0.08, 0.0]),
                (0.72, [0.44, 0.42, -0.02, 0.48, 0.57, 0.03, 0.5, 0.41, -0.06, 0.0, 1.0, 0.0, 0.08, -0.08, 0.0]),
                (1.0, [0.42, 0.36, 0.05, 0.36, 0.86, 0.15, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
            ];
            Act { feet: [[0.3, -0.13], [-0.36, 0.12]], kneel: [false, true], ..pick(keys(u, &k)) }
        }
        Work::Throw => {
            // spear cocked back over the shoulder, free arm aimed at the prey, then a lunge and the release
            let k: [(f32, K); 4] = [
                (0.0, [0.04, 0.05, 0.35, 0.0, 1.55, 0.3, 0.45, 1.25, -0.2, 1.0, 0.08, 0.0, 0.0, 0.0, -0.35]),
                (0.5, [0.07, -0.12, 0.8, -0.32, 1.6, 0.32, 0.56, 1.42, -0.12, 1.0, 0.15, 0.0, -0.02, 0.0, -0.8]),
                (IMPACT, [0.1, 0.32, -0.35, 0.56, 1.4, 0.15, 0.0, 1.0, -0.3, 1.0, -0.06, 0.0, 0.04, -0.02, 0.35]),
                (1.0, [0.1, 0.3, -0.3, 0.54, 1.38, 0.15, 0.02, 1.0, -0.3, 1.0, -0.06, 0.0, 0.04, -0.02, 0.3]),
            ];
            Act { feet: [[0.32, -0.16], [-0.24, 0.16]], ..pick(keys(u, &k)) }
        }
        Work::Offer => {
            // crouched low and still, the hand held out with the berry, edging it closer and back
            let k: [(f32, K); 3] = [
                (0.0, [0.3, 0.3, 0.0, 0.5, 0.84, 0.12, 0.12, 0.74, -0.27, 0.0, 1.0, 0.0, 0.05, -0.03, 0.0]),
                (0.5, [0.32, 0.36, 0.0, 0.62, 0.9, 0.1, 0.12, 0.72, -0.27, 0.0, 1.0, 0.0, 0.07, -0.04, 0.0]),
                (1.0, [0.3, 0.3, 0.0, 0.5, 0.84, 0.12, 0.12, 0.74, -0.27, 0.0, 1.0, 0.0, 0.05, -0.03, 0.0]),
            ];
            Act { feet: [[0.18, -0.15], [-0.1, 0.15]], ..pick(keys(u, &k)) }
        }
        Work::None => pick([0.0; 15]),
    }
}

/// Chips, sparks and splashes thrown off by a blow, flying out from where it landed and falling back.
fn push_chips(out: &mut Vec<Vertex>, work: Work, target: (f32, f32), me: (f32, f32), cyc: f32, tool: Tool) {
    let u = cyc.fract();
    let age = (u - IMPACT) / 0.3;
    if !(0.0..1.0).contains(&age) {
        return;
    }
    // the face of the thing that was hit, toward the worker
    let (dx, dz) = (me.0 - target.0, me.1 - target.1);
    let d = dx.hypot(dz).max(1e-3);
    let (nx, nz) = (dx / d, dz / d);
    let (spot, y0, col, glow, n, speed) = match work {
        Work::Chop => (0.28, 1.0, [0.78, 0.6, 0.36], 0.0, 6, 1.6),
        Work::Mine => (0.45, 0.35, [0.62, 0.62, 0.64], 0.0, 6, 1.8),
        _ => return,
    };
    set_anchor(target.0, target.1);
    let seed = cyc.floor() as i32;
    let t = age * 0.45;
    for i in 0..n {
        let h = (alaz_sim::rng::hash2(0xC41F, seed, i) % 10_000) as f32 / 10_000.0;
        let a = (h - 0.5) * 2.4;
        let (s, c) = a.sin_cos();
        // fan out toward the worker and to the sides
        let (vx, vz) = ((nx * c - nz * s) * speed, (nz * c + nx * s) * speed);
        let vy = 1.2 + 1.6 * h;
        let p = [target.0 + nx * spot + vx * t, (y0 + vy * t - 4.9 * t * t).max(0.03), target.1 + nz * spot + vz * t];
        let r = 0.035 + 0.02 * (i % 3) as f32;
        push_box(out, p, [r, r * 0.8, r], col, glow);
    }
    // a pick of metal on stone strikes bright sparks
    if work == Work::Mine && tool == Tool::MetalPick && age < 0.4 {
        for i in 0..5 {
            let a = i as f32 * 1.26 + seed as f32;
            let k = age / 0.4 * 0.7;
            push_box(out, [target.0 + nx * spot + a.cos() * k, y0 + 0.1 + k * 0.6, target.1 + nz * spot + a.sin() * k], [0.025; 3], [1.0, 0.85, 0.4], 1.0);
        }
    }
}

/// A person built from tapered limbs and round parts: legs and arms bend, walking swings them,
/// and every kind of work gets its own motion.
fn push_human(out: &mut Vec<Vertex>, x: f32, z: f32, pose: Pose) {
    set_anchor(x, z);
    use shapes::{ellipsoid, limb, sphere};
    let (s, c) = pose.heading.sin_cos();
    let (fx, fz, rx, rz) = (s, c, c, -s);
    let id = pose.id as usize;
    let tones = [[0.88, 0.67, 0.50], [0.76, 0.53, 0.37], [0.58, 0.40, 0.28], [0.93, 0.75, 0.61]];
    let hairs = [[0.17, 0.11, 0.06], [0.33, 0.19, 0.08], [0.09, 0.07, 0.07], [0.52, 0.33, 0.14]];
    // the further from the ape, the straighter, taller and smoother: hunched and furry at stage 0
    let stoop = (5 - pose.stage.min(5)) as f32 / 5.0;
    let ape = stoop > 0.55;
    let fur = [0.33, 0.23, 0.15];
    let base_skin = tones[id % 4];
    let skin = [0, 1, 2].map(|i| base_skin[i] + (fur[i] - base_skin[i]) * stoop * 0.5);
    let skin_dk = [skin[0] * 0.86, skin[1] * 0.8, skin[2] * 0.78];
    let hair = hairs[(id / 4) % 4];
    let hide = [0.56, 0.37, 0.21];
    let hide_dk = [0.40, 0.26, 0.15];
    let fluff = [0.86, 0.80, 0.66];
    let fibre = [0.66, 0.70, 0.36];
    let fibre_dk = [0.47, 0.52, 0.24];
    let (wrap, cloak, boots) = (pose.outfit & 1 != 0, pose.outfit & 2 != 0, pose.outfit & 4 != 0);
    // build and height differ a little from person to person, so a tribe is not a row of copies
    let build = 0.94 + 0.04 * ((id * 7) % 4) as f32;
    let body_k = (0.86 + 0.14 * (1.0 - stoop)) * (0.97 + 0.02 * ((id * 5) % 3) as f32);
    let w = pose.work;
    let working = w != Work::None;
    let still = (1.0 - pose.amp * 4.0).clamp(0.0, 1.0) * if working { 0.0 } else { 1.0 };
    let swing = pose.phase.sin() * pose.amp;
    let breathe = (pose.t * 1.9 + id as f32).sin() * 0.008 * still;
    let bob = pose.phase.sin().abs() * 0.05 * pose.amp + breathe;
    let u = pose.cyc.fract();
    let act = act(w, u);
    // the moment right after a blow lands: the body gives a little
    let hit = if working && u >= IMPACT { (1.0 - (u - IMPACT) / 0.12).max(0.0) } else { 0.0 };
    let (crouch, lean) = if working { (act.crouch + 0.03 * hit, act.lean + stoop * 0.12) } else { (0.0, (pose.amp - 1.0).max(0.0) * 0.3 + stoop * 0.22) };
    let twist = act.twist;
    // on steep ground: climbing bends forward and grabs ahead, going down leans back and skids with arms out
    let moving = (pose.amp * 3.0).min(1.0) * if working { 0.0 } else { 1.0 };
    let ss = |a: f32, b: f32, v: f32| ((v - a) / (b - a)).clamp(0.0, 1.0);
    let climb = ss(0.22, 0.5, pose.slope) * moving;
    let slide = ss(0.22, 0.5, -pose.slope) * moving;
    let crouch = crouch + 0.13 * climb + 0.16 * slide;
    let lean = lean + 0.34 * climb - 0.22 * slide;
    let swing = swing * (1.0 - 0.85 * slide);
    // a blow: the body jerks back and doubles over for a moment
    let jolt = pose.hurt * pose.hurt;
    let crouch = crouch + 0.12 * jolt;
    let lean = lean - 0.22 * jolt;
    let start = out.len();
    // points are (forward, up, side) around the body, turned to the heading
    let at = |f: f32, up: f32, side: f32| [x + fx * f + rx * side * build, up * body_k, z + fz * f + rz * side * build];
    // a body part shaped by half-widths across, up and along the facing, leaning forward by `tilt`
    let part = |out: &mut Vec<Vertex>, c: [f32; 3], side: f32, up: f32, fwd: f32, tilt: f32, col: [f32; 3], nu: usize| {
        let (ts, tc) = tilt.sin_cos();
        let u = [fx * ts * up, tc * up * body_k, fz * ts * up];
        let f = [fx * tc * fwd, -ts * fwd * body_k, fz * tc * fwd];
        ellipsoid(out, c, [[rx * side * build, 0.0, rz * side * build], u, f], col, nu, 5);
    };
    // the upper body, turned by `twist` over the hips
    let (ts_, tc_) = (pose.heading + twist).sin_cos();
    let (ufx, ufz, urx, urz) = (ts_, tc_, tc_, -ts_);
    let ua = |f: f32, up: f32, side: f32| [x + ufx * f + urx * side * build, up * body_k, z + ufz * f + urz * side * build];
    let upart = |out: &mut Vec<Vertex>, c: [f32; 3], side: f32, up: f32, fwd: f32, tilt: f32, col: [f32; 3], nu: usize| {
        let (ts, tc) = tilt.sin_cos();
        let u = [ufx * ts * up, tc * up * body_k, ufz * ts * up];
        let f = [ufx * tc * fwd, -ts * fwd * body_k, ufz * tc * fwd];
        ellipsoid(out, c, [[urx * side * build, 0.0, urz * side * build], u, f], col, nu, 5);
    };
    let y = |h: f32| h + bob - crouch;
    // bending forward turns the whole upper body over the hips, so the spine keeps its length; `lean` is how far
    // ahead the chest goes
    let bend = lean.atan2(0.4).clamp(-0.6, 1.2);
    let (bs, bc) = bend.sin_cos();
    let tilt = bend;
    // a point of the upper body, given as it would be standing (`extra` ahead of the spine, height `h`), bent over
    let ub = |extra: f32, h: f32, side: f32| {
        let d = h - 0.9;
        ua(d * bs + extra * bc, y(0.9 + d * bc - extra * bs), side)
    };
    let pelvis = at(0.0, y(0.9), 0.0);
    let chest = ub(0.0, 1.3, 0.0);
    let neck = ub(0.03, 1.5, 0.0);

    // legs: thigh, shin and a foot that points where the body faces; stride when walking, planted when working
    for side in [-1.0f32, 1.0] {
        let ph = -side;
        // the foot's swing is the stride, fading in from a standstill; planted feet keep pace with the ground
        let off = pose.phase.sin() * pose.stride * pose.amp.min(1.0) * (1.0 - 0.85 * slide) * ph;
        let lift = ((pose.phase + if ph > 0.0 { 0.0 } else { std::f32::consts::PI }).cos()).max(0.0) * pose.amp * 0.14 * (1.0 + 1.4 * climb) * (1.0 - slide);
        let planted = if working { 0.1 * ph } else { 0.0 } + slide * 0.16 * ph;
        let hip = at(0.0, y(0.88), side * 0.1);
        let li = if side < 0.0 { 0 } else { 1 };
        let (ankle, knee, foot) = if working {
            // feet set for the job; the knee found from the two leg bones, bending forward (or down onto the ground)
            let [ff, fs] = act.feet[li];
            let kneel = act.kneel[li];
            let ankle = at(ff, if kneel { 0.07 } else { 0.09 }, fs);
            let seg = 0.42 * body_k;
            let d = [ankle[0] - hip[0], ankle[1] - hip[1], ankle[2] - hip[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
            let a = (seg * seg - len * len * 0.25).max(0.0).sqrt();
            let dn = [d[0] / len, d[1] / len, d[2] / len];
            let fdot = fx * dn[0] + fz * dn[2];
            let mut p = [fx - dn[0] * fdot, -dn[1] * fdot, fz - dn[2] * fdot];
            let pl = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-3);
            p = [p[0] / pl, p[1] / pl, p[2] / pl];
            let mut knee = [hip[0] + d[0] * 0.5 + p[0] * a, hip[1] + d[1] * 0.5 + p[1] * a, hip[2] + d[2] * 0.5 + p[2] * a];
            knee[1] = knee[1].max(0.07);
            let foot = if kneel { at(ff - 0.07, 0.045, fs) } else { at(ff + 0.06, 0.045, fs) };
            (ankle, knee, foot)
        } else {
            // in the air the knees come up on the rise; falling, the legs hang and kick
            let lift = if pose.air != 0.0 {
                let up = pose.air.max(0.0);
                0.08 + 0.2 * up + if pose.air < 0.0 { 0.06 * (pose.t * 13.0 + side * 1.7).sin() * (-pose.air) } else { 0.0 }
            } else {
                lift
            };
            let ankle = at(off + planted, lift + 0.09, side * 0.11);
            let bend = 0.07 + 0.18 * (crouch / 0.36) + lift * 0.9;
            let knee = at((off + planted) * 0.5 + bend, 0.47 + lift * 0.5 - crouch * 0.35, side * 0.11);
            (ankle, knee, at(off + planted + 0.06, lift + 0.045, side * 0.11))
        };
        limb(out, hip, knee, 0.09, 0.07, if ape { fur } else { skin_dk }, 6);
        limb(out, knee, ankle, 0.07, 0.05, if boots { hide } else { skin_dk }, 6);
        part(out, foot, 0.06, 0.045, 0.11, 0.0, if boots { hide_dk } else { skin_dk }, 6);
        if boots {
            // a fur cuff where the boot ends
            let cuff = [knee[0] * 0.45 + ankle[0] * 0.55, knee[1] * 0.45 + ankle[1] * 0.55, knee[2] * 0.45 + ankle[2] * 0.55];
            sphere(out, cuff, [0.078, 0.045, 0.078], fluff, 6, 3, 0.12, 1);
        }
    }

    // torso: hips, a waist and a chest wider than it is deep, rounded shoulders
    let torso_col = if ape { fur } else { skin };
    part(out, pelvis, 0.17, 0.12, 0.12, 0.0, if ape { fur } else { skin_dk }, 8);
    limb(out, pelvis, chest, 0.15, 0.17, torso_col, 8);
    upart(out, chest, 0.22, 0.17, 0.14, tilt, torso_col, 9);
    if ape {
        // shaggy shoulders and back
        sphere(out, ub(-0.06, 1.4, 0.0), [0.2, 0.12, 0.2], fur, 8, 4, 0.18, id as u32);
    }
    // a loincloth until the tribe learns to make clothes; then a fibre skirt and a band across the chest
    if wrap {
        limb(out, at(0.0, y(0.94), 0.0), at(0.0, y(0.55), 0.0), 0.18, 0.26, fibre, 9);
        limb(out, at(0.0, y(0.7), 0.0), at(0.0, y(0.54), 0.0), 0.24, 0.27, fibre_dk, 9);
        upart(out, ub(0.0, 1.32, 0.0), 0.225, 0.07, 0.145, tilt, fibre, 9);
    } else {
        limb(out, at(0.0, y(0.92), 0.0), at(0.03, y(0.66), 0.0), 0.165, 0.14, hide, 7);
    }
    if pose.stage >= 2 {
        // a belt with a pouch on the hip for what the hands do not hold
        limb(out, at(0.0, y(0.97), 0.0), at(0.0, y(0.91), 0.0), 0.175, 0.175, hide_dk, 9);
        part(out, at(-0.02, y(0.86), 0.19), 0.05, 0.065, 0.06, 0.0, hide, 6);
    }
    if cloak {
        // a hide mantle over the shoulders with a fur collar, hanging down the back
        upart(out, ub(-0.02, 1.43, 0.0), 0.28, 0.1, 0.19, tilt, hide, 9);
        limb(out, ub(-0.1, 1.42, 0.0), at(-0.16, y(0.7) + crouch * 0.4, 0.0), 0.21, 0.27, hide_dk, 8);
        sphere(out, ub(-0.02, 1.46, 0.0), [0.17, 0.05, 0.16], fluff, 8, 4, 0.12, 2);
    }

    // head: turns to look around while standing idle
    let look = ((pose.t * 0.45 + id as f32 * 1.3).sin() * 1.6).clamp(-1.0, 1.0) * 0.55 * still;
    let (ls, lc) = (pose.heading + look + twist + act.head[2]).sin_cos();
    let (hfx, hfz, hrx, hrz) = (ls, lc, lc, -ls);
    let head = ub(0.05 * bs.max(0.0) + act.head[0], 1.67 + act.head[1], 0.0);
    let hd = |f: f32, up: f32, side: f32| [head[0] + hfx * f + hrx * side, head[1] + up * body_k, head[2] + hfz * f + hrz * side];
    let hpart = |out: &mut Vec<Vertex>, c: [f32; 3], side: f32, up: f32, fwd: f32, col: [f32; 3]| {
        ellipsoid(out, c, [[hrx * side, 0.0, hrz * side], [0.0, up * body_k, 0.0], [hfx * fwd, 0.0, hfz * fwd]], col, 8, 5);
    };
    limb(out, chest, neck, 0.075, 0.065, skin, 6);
    hpart(out, head, 0.135, 0.15, 0.14, skin);
    // jaw and chin, ears
    hpart(out, hd(0.04 + 0.04 * stoop, -0.08, 0.0), 0.095, 0.06, 0.09 + 0.03 * stoop, skin);
    for side in [-1.0f32, 1.0] {
        hpart(out, hd(-0.01, 0.0, side * 0.135), 0.022, 0.04, 0.03, skin_dk);
    }
    if stoop > 0.35 {
        // an early skull: a heavy brow ridge
        hpart(out, hd(0.1, 0.055, 0.0), 0.13, 0.035, 0.05, skin_dk);
    }
    // eyes, brows and a nose
    for side in [-1.0f32, 1.0] {
        sphere(out, hd(0.128, 0.015, side * 0.05), [0.02; 3], [0.06, 0.05, 0.05], 4, 3, 0.0, 1);
        if stoop <= 0.35 {
            hpart(out, hd(0.122, 0.055, side * 0.05), 0.03, 0.012, 0.018, hair);
        }
    }
    hpart(out, hd(0.15, -0.025, 0.0), 0.026, 0.035, 0.03, [skin[0] * 0.95, skin[1] * 0.84, skin[2] * 0.78]);
    // hair: one of four cuts per person (the ape is always shaggy), and a beard on some
    let style = if ape { 3 } else { (id / 16 + id) % 4 };
    hpart(out, hd(-0.02, 0.055, 0.0), 0.148, 0.12, 0.15, hair);
    hpart(out, hd(0.075, 0.1, 0.0), 0.12, 0.045, 0.07, hair);
    match style {
        // long hair down the back
        1 => limb(out, hd(-0.08, 0.02, 0.0), [neck[0] - fx * 0.14, neck[1] - 0.16 * body_k, neck[2] - fz * 0.14], 0.12, 0.08, hair, 7),
        // a knot on top
        2 => sphere(out, hd(-0.07, 0.17, 0.0), [0.065, 0.06, 0.065], hair, 6, 4, 0.0, 3),
        // shaggy all round
        3 => sphere(out, hd(-0.04, 0.03, 0.0), [0.17, 0.14, 0.17], hair, 8, 5, 0.22, id as u32),
        // short, with a tuft down the neck
        _ => hpart(out, hd(-0.1, -0.05, 0.0), 0.09, 0.09, 0.06, hair),
    }
    if !ape && id % 3 == 1 {
        hpart(out, hd(0.09, -0.1, 0.0), 0.1, 0.07, 0.07, hair);
    }

    // arms: shoulder, elbow, hand; an ape's hang lower
    let reach = 0.12 * stoop;
    let arm = |out: &mut Vec<Vertex>, side: f32, hand: [f32; 3]| {
        let sh = ub(0.0, 1.42, side * 0.235);
        let mid = [(sh[0] + hand[0]) * 0.5, (sh[1] + hand[1]) * 0.5, (sh[2] + hand[2]) * 0.5];
        let elbow = [mid[0] + urx * side * 0.07 - ufx * 0.03, mid[1] - 0.06, mid[2] + urz * side * 0.07 - ufz * 0.03];
        let upper = if cloak { hide } else if ape { fur } else { skin };
        sphere(out, sh, [0.075; 3], upper, 6, 4, 0.0, 1);
        limb(out, sh, elbow, 0.065, 0.052, upper, 6);
        limb(out, elbow, hand, 0.052, 0.042, skin, 6);
        sphere(out, hand, [0.055; 3], skin, 6, 4, 0.0, 1);
    };
    let wood = [0.46, 0.32, 0.18];
    match w {
        Work::None => {
            for side in [-1.0f32, 1.0] {
                let off = swing * side * 0.3;
                let walk = at(off + lean * 0.6, 0.86 + bob + 0.05 * pose.amp - reach, side * 0.28);
                // climbing: hands reach up the slope in turn; sliding: arms out for balance
                let grab = (pose.phase + if side > 0.0 { 0.0 } else { std::f32::consts::PI }).sin();
                let up = at(0.42 + 0.12 * grab, 1.0 + 0.22 * grab - crouch, side * 0.22);
                let wide = at(0.05, 1.22 + 0.04 * (pose.t * 6.0 + side).sin() - crouch, side * 0.6);
                let mixp = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
                let hand = mixp(mixp(walk, up, climb), wide, slide);
                // a jump swings the arms up and forward; a fall throws them up and out, grabbing at the air
                let rise = at(0.3, 1.45, side * 0.24);
                let flail = at(0.08 + 0.1 * (pose.t * 11.0 + side).sin(), 1.75 + 0.1 * (pose.t * 14.0 + side * 2.0).sin(), side * 0.48);
                let hand = mixp(mixp(hand, rise, pose.air.max(0.0) * 0.8), flail, (-pose.air).max(0.0));
                arm(out, side, hand);
            }
            if slide > 0.3 {
                // dust kicked up behind the skidding feet
                for k in 0..4 {
                    let ph = (pose.t * 2.2 + k as f32 * 0.25).fract();
                    let side = if k % 2 == 0 { 0.12 } else { -0.12 };
                    let p = at(-0.15 - 0.55 * ph, 0.08 + 0.3 * ph, side * (1.0 + ph));
                    let r = 0.05 + 0.1 * ph;
                    sphere(out, p, [r, r * 0.8, r], [0.62, 0.56, 0.46], 5, 3, 0.2, k);
                }
            }
        }
        _ => {
            // a direction in the turned upper body's frame, in the world
            let lw = |v: [f32; 3]| {
                let w = [ufx * v[0] + urx * v[2], v[1], ufz * v[0] + urz * v[2]];
                let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt().max(1e-4);
                [w[0] / l, w[1] / l, w[2] / l]
            };
            let along = |p: [f32; 3], d: [f32; 3], k: f32| [p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k];
            let hpos = |h: [f32; 3]| ua(h[0], h[1], h[2]);
            let stone = [0.56, 0.56, 0.6];
            let [hr, hl] = act.hands;
            let right = hpos(hr);
            match w {
                Work::Chop | Work::Mine => {
                    // both hands on one handle, the left lower down it
                    let d = lw(act.dir);
                    let left = along(right, d, -0.17);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    if pose.tool == Tool::Hand {
                        sphere(out, along(right, d, 0.06), [0.08, 0.07, 0.08], stone, 7, 5, 0.15, 3);
                    } else {
                        let len = if w == Work::Mine { 0.62 } else { 0.68 };
                        let end = along(right, d, len);
                        limb(out, along(right, d, -0.26), end, 0.038, 0.032, wood, 5);
                        // the swing's plane: the head's edge leads the blow
                        let n = lw(if w == Work::Mine { [0.0, 0.0, 1.0] } else { [-0.3, 0.11, -0.95] });
                        let b = [n[1] * d[2] - n[2] * d[1], n[2] * d[0] - n[0] * d[2], n[0] * d[1] - n[1] * d[0]];
                        let head_col = if pose.tool == Tool::StoneAxe { [0.56, 0.56, 0.58] } else { [0.66, 0.72, 0.82] };
                        if w == Work::Mine && pose.tool == Tool::MetalPick {
                            limb(out, end, along(end, b, 0.2), 0.035, 0.008, head_col, 5);
                            limb(out, end, along(end, b, -0.16), 0.035, 0.01, head_col, 5);
                        } else {
                            // a wedge of stone or metal lashed across the handle's end
                            let c = along(along(end, d, -0.06), b, 0.06);
                            limb(out, along(c, b, -0.07), along(c, b, 0.13), 0.07, 0.025, head_col, 5);
                            limb(out, along(end, d, -0.1), along(end, d, -0.02), 0.04, 0.04, [0.5, 0.42, 0.28], 5);
                        }
                    }
                }
                Work::Throw => {
                    let d = lw(act.dir);
                    arm(out, 1.0, right);
                    arm(out, -1.0, hpos(hl));
                    let tip = along(right, d, 1.0);
                    limb(out, along(right, d, -0.6), tip, 0.025, 0.016, wood, 5);
                    limb(out, along(tip, d, -0.04), along(tip, d, 0.14), 0.04, 0.005, [0.62, 0.62, 0.66], 5);
                }
                Work::Hammer => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    sphere(out, [right[0], right[1] + 0.03, right[2]], [0.07, 0.065, 0.08], [0.62, 0.62, 0.66], 7, 5, 0.12, 3);
                    let core = [left[0], left[1] + 0.05, left[2]];
                    sphere(out, core, [0.1, 0.07, 0.09], [0.48, 0.48, 0.52], 7, 5, 0.14, 5);
                    if hit > 0.0 {
                        // sparks and flakes off the core
                        let k = 1.0 - hit;
                        for i in 0..5 {
                            let a = i as f32 * 1.26 + pose.cyc.floor() * 2.1;
                            let r = 0.08 + 0.4 * k;
                            let p = [core[0] + a.cos() * r, core[1] + 0.05 + 0.5 * k - 1.4 * k * k, core[2] + a.sin() * r];
                            let (c, g) = if i % 2 == 0 { ([1.0, 0.85, 0.4], 1.0) } else { ([0.7, 0.7, 0.74], 0.0) };
                            shapes::sphere(out, p, [0.022; 3], c, 4, 3, 0.0, i);
                            if g > 0.0 {
                                shapes::sphere(out, p, [0.028; 3], c, 4, 3, 0.0, i);
                            }
                        }
                    }
                }
                Work::Dig => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    let clay = [0.6, 0.42, 0.3];
                    let mid = [(right[0] + left[0]) * 0.5, (right[1] + left[1]) * 0.5 + 0.04, (right[2] + left[2]) * 0.5];
                    if (IMPACT..0.87).contains(&u) {
                        sphere(out, mid, [0.1, 0.07, 0.1], clay, 6, 4, 0.25, 7);
                    } else if u >= 0.87 {
                        // the toss: the clod flies off to the side and lands
                        let k = (u - 0.87) / 0.13;
                        let p = [mid[0] + urx * 0.9 * k, (mid[1] + 0.5 * k - 1.2 * k * k).max(0.05), mid[2] + urz * 0.9 * k];
                        sphere(out, p, [0.1, 0.07, 0.1], clay, 6, 4, 0.25, 7);
                    }
                    if hit > 0.0 {
                        let k = 1.0 - hit;
                        for i in 0..4 {
                            let a = i as f32 * 1.6 + pose.cyc.floor();
                            let p = [mid[0] + a.cos() * 0.3 * k, 0.05 + 0.3 * k - 0.4 * k * k, mid[2] + a.sin() * 0.3 * k];
                            sphere(out, p, [0.04; 3], clay, 4, 3, 0.2, i);
                        }
                    }
                }
                Work::Pick => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    // whatever was plucked rides to the pouch in the hand that pulled it
                    let leafy = [0.42, 0.62, 0.26];
                    if (0.3..IMPACT + 0.05).contains(&u) {
                        sphere(out, [right[0], right[1] + 0.05, right[2]], [0.06; 3], leafy, 5, 4, 0.2, 2);
                    }
                    if u >= 0.8 || u < 0.05 {
                        sphere(out, [left[0], left[1] + 0.05, left[2]], [0.06; 3], leafy, 5, 4, 0.2, 2);
                    }
                }
                Work::Drink => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    let water = [0.45, 0.66, 0.95];
                    let mid = [(right[0] + left[0]) * 0.5, (right[1] + left[1]) * 0.5, (right[2] + left[2]) * 0.5];
                    if (0.3..IMPACT).contains(&u) {
                        // water in the cupped hands, dripping on the way up
                        sphere(out, [mid[0], mid[1] + 0.04, mid[2]], [0.07, 0.03, 0.07], water, 6, 3, 0.0, 1);
                        for i in 0..2 {
                            let k = ((u - 0.3) * 6.0 + i as f32 * 0.5).fract();
                            shapes::sphere(out, [mid[0], mid[1] - 0.5 * k * k * 1.2, mid[2]], [0.022; 3], water, 4, 3, 0.0, i);
                        }
                    } else if u < 0.2 {
                        // the splash where the hands go in
                        for i in 0..4 {
                            let a = i as f32 * 1.57 + pose.cyc.floor();
                            let k = u / 0.2;
                            shapes::sphere(out, [mid[0] + a.cos() * 0.15 * k, 0.04 + 0.2 * k - 0.3 * k * k, mid[2] + a.sin() * 0.15 * k], [0.025; 3], water, 4, 3, 0.0, i);
                        }
                    }
                }
                Work::Offer => {
                    arm(out, 1.0, right);
                    arm(out, -1.0, hpos(hl));
                    sphere(out, along(right, lw([1.0, 0.6, 0.0]), 0.06), [0.05; 3], [0.8, 0.12, 0.2], 6, 4, 0.0, 1);
                }
                _ => {}
            }
        }
    }
    // ... and flashes red, then white at the very start
    if pose.hurt > 0.0 {
        let k = pose.hurt.powf(1.5);
        let flash = if pose.hurt > 0.85 { [1.0, 0.9, 0.85] } else { [1.0, 0.18, 0.12] };
        for v in &mut out[start..] {
            for i in 0..3 {
                v.color[i] += (flash[i] - v.color[i]) * k * 0.75;
            }
        }
        // a few drops flying off
        if pose.hurt > 0.4 {
            let u = 1.0 - pose.hurt;
            for i in 0..5u32 {
                let a = i as f32 * 1.26 + pose.id as f32;
                let (dx, dz) = (a.cos() * (0.25 + 1.4 * u), a.sin() * (0.25 + 1.4 * u));
                let y = 1.25 + 1.6 * u - 5.0 * u * u;
                shapes::sphere(out, [x + dx, y.max(0.05), z + dz], [0.035; 3], [0.62, 0.06, 0.06], 4, 3, 0.0, i);
            }
        }
    }
}

struct Quad {
    body: [f32; 3],
    head: [f32; 3],
    len: f32,
    leg_h: f32,
    thick: f32,
    head_r: f32,
    neck: f32,
    tail: bool,
    horns: bool,
}

/// A four-legged animal built from rounded parts; legs swing with the walk phase.
fn push_quad(out: &mut Vec<Vertex>, x: f32, z: f32, pose: Pose, q: Quad) {
    set_anchor(x, z);
    use shapes::{limb, sphere};
    let (s, c) = pose.heading.sin_cos();
    let (fx, fz, rx, rz) = (s, c, c, -s);
    let at = |f: f32, r: f32, h: f32| [x + fx * f + rx * r, h, z + fz * f + rz * r];
    let bob = pose.phase.sin().abs() * 0.04 * pose.amp + (pose.t * 1.6).sin() * 0.006 * (1.0 - pose.amp);
    let top = q.leg_h + q.thick + bob;
    let dark = [q.body[0] * 0.7, q.body[1] * 0.7, q.body[2] * 0.7];
    for (fi, f) in [q.len * 0.7, -q.len * 0.7].into_iter().enumerate() {
        for (si, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let ph = if (fi + si) % 2 == 0 { 1.0 } else { -1.0 };
            let off = pose.phase.sin() * pose.stride * pose.amp.min(1.0) * ph;
            let hip = at(f, side * q.thick * 0.6, q.leg_h + q.thick * 0.6 + bob);
            let foot = at(f + off, side * q.thick * 0.6, (off.abs() * 0.3).min(0.2));
            limb(out, hip, foot, q.thick * 0.28, q.thick * 0.18, dark, 6);
        }
    }
    limb(out, at(-q.len, 0.0, top - q.thick * 0.2), at(q.len, 0.0, top - q.thick * 0.1), q.thick * 0.95, q.thick * 0.85, q.body, 10);
    // standing animals put their heads down to graze now and then, and look around in between
    let still = (1.0 - pose.amp * 4.0).clamp(0.0, 1.0);
    let k = pose.id as f32 * 1.7;
    let graze = ((pose.t * 0.33 + k).sin() * 1.8 - 0.5).clamp(0.0, 1.0) * still;
    let look = (pose.t * 0.7 + k * 0.5).sin() * 0.25 * still * (1.0 - graze);
    let hy = top + q.neck * 0.5 + (q.head_r * 1.1 - top - q.neck * 0.5) * graze;
    let hf = q.len + q.neck * (0.5 + 0.25 * graze);
    let head_at = at(hf, look * q.neck, hy);
    limb(out, at(q.len * 0.8, 0.0, top), head_at, q.thick * 0.55, q.head_r * 0.9, q.body, 8);
    sphere(out, head_at, [q.head_r; 3], q.head, 10, 8, 0.0, 7);
    let snout = at(hf + q.head_r * 1.3, look * q.neck, hy - q.head_r * (0.4 + 0.5 * graze));
    sphere(out, snout, [q.head_r * 0.6; 3], q.head, 8, 6, 0.0, 8);
    if q.horns {
        for side in [-1.0f32, 1.0] {
            limb(out, at(hf, look * q.neck + side * q.head_r * 0.8, hy + q.head_r * 0.6), at(hf - q.head_r * 0.4 * graze, look * q.neck + side * q.head_r * 2.0, hy + q.head_r * 1.8), 0.05, 0.015, [0.9, 0.86, 0.75], 6);
        }
    }
    if q.tail {
        // the tail swishes, quicker while walking
        let wag = (pose.t * (3.0 + 4.0 * pose.amp) + k).sin() * (0.06 + 0.06 * pose.amp);
        limb(out, at(-q.len, 0.0, top), at(-q.len - 0.18 - q.head_r, wag, top - 0.1 - 0.08 * still), 0.05, 0.02, dark, 6);
    }
}

fn push_creature(out: &mut Vec<Vertex>, kind: AgentKind, x: f32, z: f32, marked: bool, pose: Pose) {
    set_anchor(x, z);
    use shapes::{limb, sphere};
    let height = match kind {
        AgentKind::Human => {
            push_human(out, x, z, pose);
            1.9
        }
        AgentKind::Dog => {
            push_quad(out, x, z, pose, Quad { body: [0.58, 0.42, 0.28], head: [0.62, 0.46, 0.32], len: 0.28, leg_h: 0.22, thick: 0.15, head_r: 0.11, neck: 0.18, tail: true, horns: false });
            0.6
        }
        AgentKind::Ox => {
            push_quad(out, x, z, pose, Quad { body: [0.30, 0.20, 0.14], head: [0.34, 0.23, 0.16], len: 0.55, leg_h: 0.45, thick: 0.40, head_r: 0.2, neck: 0.5, tail: true, horns: true });
            1.4
        }
        AgentKind::Horse => {
            push_quad(out, x, z, pose, Quad { body: [0.62, 0.40, 0.22], head: [0.58, 0.37, 0.20], len: 0.5, leg_h: 0.7, thick: 0.28, head_r: 0.15, neck: 0.62, tail: true, horns: false });
            1.7
        }
        AgentKind::Bot => {
            sphere(out, [x, 0.5, z], [0.3, 0.42, 0.3], [0.62, 0.66, 0.72], 12, 8, 0.0, 3);
            sphere(out, [x, 1.05, z], [0.22, 0.22, 0.22], [0.72, 0.76, 0.82], 12, 8, 0.0, 4);
            let (fx, fz) = (pose.heading.sin(), pose.heading.cos());
            sphere(out, [x + fx * 0.17, 1.08, z + fz * 0.17], [0.1, 0.045, 0.05], [0.3, 0.9, 1.0], 8, 4, 0.0, 5);
            limb(out, [x, 0.0, z], [x, 0.18, z], 0.2, 0.28, [0.4, 0.42, 0.46], 8);
            1.4
        }
    };
    if marked {
        push_box(out, [x, height + 0.12, z], [0.07, 0.07, 0.07], [0.3, 0.9, 0.9], 1.0);
    }
}

/// A short trail of paw prints behind an animal.
fn push_tracks(out: &mut Vec<Vertex>, id: u32, x: f32, z: f32, heading: (i64, i64), wolf: bool) {
    set_anchor(x, z);
    let (hx, hz) = (heading.0 as f32, heading.1 as f32);
    let len = (hx * hx + hz * hz).sqrt();
    // behind a walking animal; a resting one shows a fixed trail
    let (dx, dz) = if len > 100.0 {
        (-hx / len, -hz / len)
    } else {
        let a = (id.wrapping_mul(2654435761) >> 8) as f32 * 0.0001;
        (a.cos(), a.sin())
    };
    let col = if wolf { [0.20, 0.17, 0.15] } else { [0.30, 0.25, 0.19] };
    for i in 1..=6 {
        let d = 0.9 + i as f32 * 0.8;
        let side = if i % 2 == 0 { 0.18 } else { -0.18 };
        let (px, pz) = (x + dx * d - dz * side, z + dz * d + dx * side);
        push_box(out, [px, 0.015, pz], [0.07, 0.008, 0.09], col, 0.0);
    }
}

/// How a creature is moving this frame.
#[derive(Clone, Copy)]
struct Pose {
    heading: f32,
    phase: f32,
    amp: f32,
    /// Seconds, for work cycles.
    t: f32,
    work: Work,
    tool: Tool,
    /// Clothes worn (bit 1 wrap, 2 cloak, 4 boots) and the person's id, which varies skin and hair.
    outfit: u8,
    id: u32,
    /// How far the tribe has evolved, 0 (ape) to 5 (sage): posture, height and body hair follow.
    stage: u8,
    /// Rise of the ground along the heading (metres per metre): steep climbs and slides change the gait.
    slope: f32,
    /// Flinch after a blow, 1 to 0.
    hurt: f32,
    /// Foot swing in metres, matched to the walk cycle (see `step_walk`).
    stride: f32,
    /// Work motion: whole cycles done plus the fraction of the current one; each blow lands at `IMPACT`.
    cyc: f32,
    /// In the air: rising (up to 1) or falling (down to -1); 0 on the ground.
    air: f32,
}

impl Pose {
    fn idle(t: f32) -> Pose {
        Pose { heading: 0.0, phase: 0.0, amp: 0.0, t, work: Work::None, tool: Tool::Hand, outfit: 0, id: 0, stage: 5, slope: 0.0, hurt: 0.0, stride: 0.3, cyc: 0.0, air: 0.0 }
    }
}

/// What a creature is busy doing, drawn as a looping motion.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    None,
    Chop,
    Mine,
    Pick,
    Dig,
    Drink,
    Hammer,
    Throw,
    Offer,
}

/// What is held while working (the order matches the atlas: hand, stone axe, metal axe, metal pick, spear).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Hand,
    StoneAxe,
    MetalAxe,
    MetalPick,
    Spear,
}

/// Soft value noise on the tile grid, -1..1.
fn noise(world: &World, salt: u64, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |dx: i32, dy: i32| alaz_sim::rng::hash2(world.seed ^ salt, x0 as i32 + dx, y0 as i32 + dy) as f32 % 1000.0 / 500.0 - 1.0;
    let (a, b) = (h(0, 0) + (h(1, 0) - h(0, 0)) * sx, h(0, 1) + (h(1, 1) - h(0, 1)) * sx);
    a + (b - a) * sy
}

/// What this agent is doing right now (if it is close enough to the thing to actually be doing it).
fn work_of(world: &World, a: &alaz_sim::world::Agent) -> (Work, Tool, (f32, f32)) {
    use alaz_sim::world::Task;
    use alaz_sim::ItemKind as I;
    let has = |k| a.inventory.get(k) > 0;
    let m = |x: i64, y: i64| (x as f32 / 1000.0, y as f32 / 1000.0);
    let me = m(a.x, a.y);
    let near = |p: (f32, f32), r: f32| (p.0 - me.0).powi(2) + (p.1 - me.1).powi(2) <= r * r;
    match a.task {
        Task::Gather { node, .. } => {
            let Some(n) = world.nodes.get(node.0 as usize) else { return (Work::None, Tool::Hand, me) };
            let p = { let (x, y) = n.center_mm(); m(x, y) };
            if !near(p, 2.4) {
                return (Work::None, Tool::Hand, p);
            }
            match n.kind {
                NodeKind::Tree => {
                    let t = if has(I::IronAxe) || has(I::BronzeAxe) { Tool::MetalAxe } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Chop, t, p)
                }
                NodeKind::StoneOutcrop | NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop => {
                    let t = if has(I::IronPickaxe) || has(I::BronzePickaxe) { Tool::MetalPick } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Mine, t, p)
                }
                NodeKind::ClayDeposit => (Work::Dig, Tool::Hand, p),
                _ => (Work::Pick, Tool::Hand, p),
            }
        }
        Task::Split { body, .. } => match world.bodies.iter().find(|b| b.id == body) {
            Some(b) => {
                let p = { let (x, y) = b.cut_point(); m(x, y) };
                // bent over the trunk on the ground, the axe coming down on it
                if b.lying() && near(p, 2.4) {
                    let t = if has(I::IronAxe) || has(I::BronzeAxe) { Tool::MetalAxe } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Mine, t, p)
                } else {
                    (Work::None, Tool::Hand, p)
                }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Pick { body, .. } => match world.bodies.iter().find(|b| b.id == body) {
            Some(b) => {
                let p = m(b.x / alaz_sim::physics::Q, b.y / alaz_sim::physics::Q);
                if near(p, 1.2) { (Work::Pick, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Drink { tx, ty } => {
            let p = (tx as f32 + 0.5, ty as f32 + 0.5);
            if near(p, 2.4) { (Work::Drink, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
        }
        Task::Craft { .. } => (Work::Hammer, Tool::Hand, (me.0 + 0.0, me.1 + 0.0)),
        Task::Hunt { wild, .. } => match world.wild.iter().find(|w| w.id == wild) {
            Some(w) => {
                let p = m(w.x, w.y);
                if near(p, 7.0) { (Work::Throw, Tool::Spear, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Tame { wild, .. } => match world.wild.iter().find(|w| w.id == wild) {
            Some(w) => {
                let p = m(w.x, w.y);
                if near(p, 3.0) { (Work::Offer, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        _ => (Work::None, Tool::Hand, me),
    }
}


/// Felled trees and loose rocks. A tree is drawn exactly as it stood, turned over on its stump; once down it settles
/// along the slope, kicks up dust, and after a while sinks away into the ground. Rocks tumble as they roll.
fn push_bodies(out: &mut Vec<Vertex>, world: &World, alpha: f32, inside: &dyn Fn(f32, f32) -> bool) {
    use alaz_sim::physics::{BodyKind, HALF_PI_URAD, LOG_ROT_TICKS, Q};
    let q = Q as f32;
    for b in &world.bodies {
        let (bx, bz) = ((b.x as f32 + b.vx as f32 * alpha) / q / 1000.0, (b.y as f32 + b.vy as f32 * alpha) / q / 1000.0);
        if !inside(bx, bz) {
            continue;
        }
        match b.kind {
            BodyKind::Tree => {
                let node = alaz_sim::world::Node { kind: NodeKind::Tree, tx: b.tx, ty: b.ty, amount: 1 };
                let start = out.len();
                push_node(out, world, &node, world.perception_level());
                set_anchor(0.0, 0.0);
                foliage(false);
                let Some(v0) = out.get(start) else { continue };
                let (ax, az) = (v0.off[0], v0.off[1]);
                let (dx, dz) = (b.dx as f32 / 1000.0, b.dy as f32 / 1000.0);
                let len = b.size as f32 / 1000.0;
                // it comes to lie along the ground, not through it
                let g0 = ground_height(world.seed, ax, az);
                let tip_h = ground_height(world.seed, ax + dx * len * 0.8, az + dz * len * 0.8) - g0;
                let lie = std::f32::consts::FRAC_PI_2 - (tip_h / (len * 0.8)).atan();
                let raw = ((b.angle as f32 + b.spin as f32 * alpha) / HALF_PI_URAD as f32).min(1.0);
                let th = raw * lie;
                let (sn, cs) = th.sin_cos();
                // left to rot, it sinks into the earth over its last ten seconds
                let left = LOG_ROT_TICKS.saturating_sub(b.rest) as f32 / 600.0;
                let sink = (1.0 - left).clamp(0.0, 1.0) * 0.9;
                // broken up from the crown end: what has been cut away is gone (whole triangles past the cut)
                let cut = b.trunk_left_mm() as f32 / 1000.0;
                let broken = b.wood < b.full;
                let mut along_of = Vec::with_capacity(out.len() - start);
                for v in &mut out[start..] {
                    let (px, pz) = (v.pos[0] - ax, v.pos[2] - az);
                    let (along, up) = (px * dx + pz * dz, v.pos[1]);
                    along_of.push(up);
                    let (sx, sz) = (px - along * dx, pz - along * dz);
                    let (na, nu) = (along * cs + up * sn, up * cs - along * sn);
                    v.pos = [ax + sx + na * dx, nu - sink, az + sz + na * dz];
                }
                if broken {
                    // standing, "up" was along the trunk; keep the triangles below the cut
                    let mut keep = start;
                    for t in 0..(out.len() - start) / 3 {
                        let i = start + t * 3;
                        let far = along_of[t * 3].max(along_of[t * 3 + 1]).max(along_of[t * 3 + 2]);
                        if far <= cut + 0.05 {
                            out.copy_within(i..i + 3, keep);
                            keep += 3;
                        }
                    }
                    out.truncate(keep);
                    // the fresh cut face, pale wood
                    let r = 0.06 + len * 0.022;
                    set_anchor(ax, az);
                    shapes::sphere(out, [ax + dx * cut, r * 0.9 - sink, az + dz * cut], [r * 0.45, r * 0.95, r * 0.95], [0.86, 0.72, 0.48], 6, 4, 0.0, 5);
                }
                // dust where it hit, for a moment
                if b.angle >= HALF_PI_URAD && b.rest < 36 {
                    // small puffs thrown out along the trunk, rising, spreading and thinning out
                    let k = b.rest as f32 / 36.0;
                    set_anchor(ax, az);
                    for i in 0..8u32 {
                        let t = 0.2 + 0.8 * i as f32 / 7.0;
                        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                        let r = (0.14 + 0.1 * k) * (1.0 - k * k);
                        let spread = 0.25 + 0.6 * k;
                        let p = [ax + dx * len * t - dz * side * spread, 0.08 + 0.45 * k, az + dz * len * t + dx * side * spread];
                        shapes::sphere(out, p, [r, r * 0.7, r], [0.72, 0.68, 0.6], 5, 3, 0.3, i);
                    }
                }
            }
            BodyKind::Piece => {
                // a short split length of wood, pale where it broke, lying at its own angle and turning as it rolls
                let z_m = (b.z as f32 + b.vz as f32 * alpha) / q / 1000.0;
                let lift = z_m - ground_height(world.seed, bx, bz);
                let h = b.id.wrapping_mul(2_654_435_761);
                let a = (h % 628) as f32 / 100.0 + b.angle as f32 / 150.0;
                let (ox, oz) = (a.cos() * 0.2, a.sin() * 0.2);
                set_anchor(bx, bz);
                let y = 0.07 + lift.max(0.0);
                let item = alaz_sim::map::ItemKind::ALL.get(b.item as usize).copied().unwrap_or(alaz_sim::map::ItemKind::Wood);
                // afloat: it bobs on the swell
                let afloat = world.map.terrain(bx.floor() as i32, bz.floor() as i32) == Some(alaz_sim::map::Terrain::Water);
                let lift = if afloat { lift.max(0.0) + 0.04 + 0.05 * (world.tick as f32 / 25.0 + b.id as f32).sin() } else { lift };
                if item != alaz_sim::map::ItemKind::Wood {
                    // something put down: a small bundle in a colour of its own, with a pale halo on the ground
                    let col = piece_colour(item);
                    let y0 = lift.max(0.0);
                    use alaz_sim::map::ItemKind as I;
                    match item {
                        I::Branch | I::Plank | I::Spear => {
                            shapes::limb(out, [bx - ox * 2.4, y0 + 0.05, bz - oz * 2.4], [bx + ox * 2.4, y0 + 0.07, bz + oz * 2.4], 0.045, 0.03, col, 5);
                        }
                        I::Fiber | I::Herb => {
                            for k in 0..6u32 {
                                let t = (k as f32 - 2.5) * 0.04;
                                shapes::limb(out, [bx - ox * 2.0 + t, y0 + 0.04, bz - oz * 2.0 - t], [bx + ox * 2.0 + t, y0 + 0.05, bz + oz * 2.0 - t], 0.018, 0.01, col, 3);
                            }
                        }
                        I::Stone | I::Flint | I::SharpStone | I::CopperOre | I::TinOre | I::IronOre | I::Coal | I::Limestone => {
                            shapes::sphere(out, [bx, y0 + 0.1, bz], [0.17, 0.12, 0.15], col, 6, 4, 0.25, h);
                        }
                        I::Berries => {
                            for k in 0..5u32 {
                                let a2 = k as f32 * 1.26;
                                shapes::sphere(out, [bx + a2.cos() * 0.08, y0 + 0.06, bz + a2.sin() * 0.08], [0.06; 3], col, 5, 3, 0.0, k);
                            }
                        }
                        _ => {
                            // a tied bundle
                            shapes::sphere(out, [bx, y0 + 0.14, bz], [0.2, 0.15, 0.2], col, 7, 4, 0.12, h);
                            shapes::sphere(out, [bx, y0 + 0.3, bz], [0.06, 0.05, 0.06], [col[0] * 0.7, col[1] * 0.7, col[2] * 0.7], 5, 3, 0.0, 2);
                        }
                    }
                    continue;
                }
                let bark = [0.38 + 0.04 * ((h >> 9) % 3) as f32, 0.26, 0.15];
                shapes::limb(out, [bx - ox, y, bz - oz], [bx + ox, y, bz + oz], 0.07, 0.065, bark, 6);
                shapes::sphere(out, [bx + ox, y, bz + oz], [0.055; 3], [0.86, 0.72, 0.48], 5, 3, 0.0, 1);
                shapes::sphere(out, [bx - ox, y, bz - oz], [0.05; 3], [0.8, 0.66, 0.44], 5, 3, 0.0, 2);
                set_anchor(0.0, 0.0);
            }
            BodyKind::Rock => {
                let r = b.size as f32 / 1000.0;
                let z_m = (b.z as f32 + b.vz as f32 * alpha) / q / 1000.0;
                let lift = z_m - ground_height(world.seed, bx, bz);
                set_anchor(bx, bz);
                let start = out.len();
                let seed = (b.size as u32).wrapping_mul(2_654_435_761);
                let g = 0.5 + 0.06 * ((seed >> 7) % 4) as f32;
                shapes::rock(out, [bx, r, bz], [r * 1.1, r * 0.85, r], [g, g * 0.98, g * 0.95], seed);
                // tumbling: turned about two axes by how far it has rolled
                let roll = b.angle as f32 / 1000.0 / r.max(0.05);
                let (s1, c1) = (roll).sin_cos();
                let (s2, c2) = (roll * 0.63).sin_cos();
                for v in &mut out[start..] {
                    let (px, py, pz) = (v.pos[0] - bx, v.pos[1] - r, v.pos[2] - bz);
                    let (py, pz) = (py * c1 - pz * s1, py * s1 + pz * c1);
                    let (px, py) = (px * c2 - py * s2, px * s2 + py * c2);
                    v.pos = [bx + px, r + py + lift, bz + pz];
                }
                set_anchor(0.0, 0.0);
            }
        }
    }
}

/// The colour a dropped item lies on the ground in.
fn piece_colour(k: alaz_sim::map::ItemKind) -> [f32; 3] {
    use alaz_sim::map::ItemKind as I;
    match k {
        I::Stone | I::Flint | I::SharpStone => [0.55, 0.55, 0.55],
        I::Fiber | I::Cloth => [0.86, 0.78, 0.42],
        I::Branch | I::Plank => [0.55, 0.40, 0.24],
        I::Berries => [0.78, 0.12, 0.22],
        I::RawMeat | I::CookedMeat => [0.70, 0.30, 0.25],
        I::Clay | I::Pot | I::LargePot => [0.66, 0.44, 0.30],
        I::CopperOre | I::CopperIngot | I::CopperWire => [0.78, 0.46, 0.26],
        I::IronOre | I::Steel => [0.42, 0.42, 0.46],
        I::Coal | I::Charcoal | I::Coke => [0.12, 0.12, 0.12],
        _ => [0.80, 0.70, 0.52],
    }
}

thread_local! {
    /// This frame's wind, for models that move with it (smoke): (x, z) scaled by strength, and the strength 0..1.
    static WIND: std::cell::Cell<[f32; 3]> = const { std::cell::Cell::new([0.0, 0.0, 0.0]) };
}

/// The height of the sea's swell at (x, z) metres right now, worked out exactly as the shader draws it (the same six
/// wave trains, the same weights, in double precision), so a swimmer rides the waves that are on screen.
fn swell_height(world: &World, x: f32, z: f32, alpha: f32) -> f32 {
    const DIRS: [f64; 6] = [0.0, 63.0, 118.0, 182.0, 238.0, 301.0];
    const LENS: [f64; 6] = [14.0, 11.0, 8.5, 6.5, 5.3, 3.1];
    const HS: [f64; 6] = [0.17, 0.14, 0.11, 0.08, 0.05, 0.03];
    let (tx, ty) = (x.floor() as i32, z.floor() as i32);
    // how far out from land, in tiles (the shader's shore distance, up to 8)
    let mut depth = 8.0f64;
    'ring: for r in 1..=8i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) == r && world.map.terrain(tx + dx, ty + dy).is_some_and(|t| t != Terrain::Water) {
                    depth = r as f64 - 0.5;
                    break 'ring;
                }
            }
        }
    }
    let smooth = |a: f64, b: f64, v: f64| {
        let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (heading, strength) = alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64);
    let wnd = 0.1 + 1.8 * strength as f64 / 1000.0;
    let wa = heading as f64 / 1000.0;
    let amp = smooth(0.0, 2.5, depth) * (0.5 + 1.1 * wnd) * (1.0 + 1.2 * smooth(1.5, 8.0, depth));
    let t = (world.tick as f64 + alpha as f64) / 60.0;
    let mut h = 0.0;
    for i in 0..6 {
        let a = DIRS[i].to_radians();
        let k = std::f64::consts::TAU / LENS[i];
        let w = (9.8 * k).sqrt();
        let ph = k * (a.cos() * x as f64 + a.sin() * z as f64) - w * t + i as f64 * 1.7;
        let along = (a.cos() * wa.cos() + a.sin() * wa.sin()).max(0.0);
        h += HS[i] * amp * (0.12 + 0.88 * along * along) * 1.6 * ph.sin();
    }
    h as f32
}

/// Fish in deep water: small schools, placed by the seed, circling just under the surface and riding the swell;
/// now and then one leaps out, silver in the light. Only drawn, not simulated.
fn push_fish(out: &mut Vec<Vertex>, world: &World, view: (f32, f32, f32, f32), t: f32, alpha: f32) {
    use shapes::limb;
    let step = 5;
    let (x0, z0) = ((view.0 as i32).div_euclid(step) * step, (view.1 as i32).div_euclid(step) * step);
    let (x1, z1) = (view.2 as i32, view.3 as i32);
    let mut ty = z0;
    while ty <= z1 {
        let mut tx = x0;
        while tx <= x1 {
            let h = alaz_sim::rng::hash2(world.seed ^ 0xF15F, tx, ty) as u32;
            // the same schools the game knows of (a fish can be caught where one is drawn)
            if alaz_sim::map::fish_school(&world.map, tx, ty) {
                let (cx, cz) = (tx as f32 + 0.5 + (h >> 8 & 7) as f32 * 0.3, ty as f32 + 0.5 + (h >> 11 & 7) as f32 * 0.3);
                let n = 2 + (h >> 14) % 3;
                let dir = if h >> 17 & 1 == 0 { 1.0 } else { -1.0 };
                let speed = 0.25 + (h >> 18 & 7) as f32 * 0.04;
                let big = 1.6 + (h >> 21 & 3) as f32 * 0.25;
                for k in 0..n {
                    // each one on its own ring, a little behind the one in front
                    let r = 1.0 + 0.35 * k as f32;
                    let a = t * speed * dir + k as f32 * 0.55 + (h & 255) as f32;
                    let (x, z) = (cx + a.cos() * r, cz + a.sin() * r);
                    if alaz_sim::map::water_depth(&world.map, x.floor() as i32, z.floor() as i32) == 0 {
                        continue;
                    }
                    // heading along the ring
                    let (hx, hz) = (-a.sin() * dir, a.cos() * dir);
                    let len = 0.22 * big;
                    let surface = swell_height(world, x, z, alpha);
                    // a leap: every so often, for a moment, out of the water and back in an arc
                    let cycle = 9.0 + (h >> 23 & 7) as f32;
                    let ph = ((t + k as f32 * 3.1 + (h >> 4 & 31) as f32) % cycle) / 0.7;
                    let leaping = k == 0 && ph < 1.0;
                    let (y, col) = if leaping {
                        (surface + 0.55 * (ph * std::f32::consts::PI).sin(), [0.78, 0.82, 0.86])
                    } else {
                        (surface + 0.02, [0.10, 0.19, 0.25])
                    };
                    // a tail beat
                    let beat = (t * 9.0 + k as f32).sin() * 0.04;
                    let head = [x + hx * len * 0.5, y, z + hz * len * 0.5];
                    let tail = [x - hx * len * 0.5 - hz * beat, y + if leaping { -0.06 } else { 0.0 }, z - hz * len * 0.5 + hx * beat];
                    set_anchor(x, z);
                    limb(out, head, tail, 0.05 * big, 0.025 * big, col, 5);
                    let fin = [tail[0] - hx * 0.08 - hz * 0.05, tail[1], tail[2] - hz * 0.08 + hx * 0.05];
                    limb(out, tail, fin, 0.03 * big, 0.005, col, 4);
                }
            }
            tx += step;
        }
        ty += step;
    }
    set_anchor(0.0, 0.0);
}
