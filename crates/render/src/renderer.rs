use crate::bake::GroundBake;
use crate::mesh::*;
use crate::prof::GpuProf;
use crate::sprites::{self, Art};
use crate::{hotreload, pipelines, Camera, DEPTH_FORMAT, NIGHT_TINT, SHADOW_FORMAT, SHADOW_RES, SHADOW_STATIC_RES};
use alaz_sim::time::daylight_permille;
use alaz_sim::{rules, AgentKind, BuildingKind, Terrain, World};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::event::WindowEvent;
use winit::window::Window;

mod frame;

pub(crate) const CHUNK_TILES: i32 = 32;

pub(crate) const MAX_CACHED_CHUNKS: usize = 600;

/// Milliseconds per frame spent building chunk meshes (at least one chunk is always built).
pub(crate) const BUILD_BUDGET_MS: f64 = 2.5;

pub(crate) struct Chunk {
    pub(crate) terrain: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes: Option<(wgpu::Buffer, u32)>,
    /// Grass, flowers, pebbles: too small to show in the sun's shadow map, so they are kept apart and not cast.
    pub(crate) decor: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes_version: (u64, u64),
    /// What had been seen when the nodes were built (unknown things look plain).
    pub(crate) seen: u128,
    pub(crate) level: u8,
    pub(crate) used: u64,
    /// Which build of the meshes this is (unique per build): the static shadow map is redrawn when it differs.
    pub(crate) build: u64,
}

/// The sun's shadow of what stands still (chunk models), kept between frames. It is drawn into a box 1.5 times as
/// wide as the dynamic one, with the same texel size, and redrawn only when the sun has stepped, the box size
/// changed, the camera nears the edge of the margin, or a chunk's models changed.
#[derive(Default)]
pub(crate) struct StaticShadow {
    /// The map holds a finished drawing.
    pub(crate) valid: bool,
    /// Light-space view-projection it was drawn with, and the sun direction it was drawn along.
    pub(crate) vp: glam::Mat4,
    pub(crate) dir: [f32; 3],
    /// Half the dynamic box's width in metres (so the texel is `radius / 1024`), the box centre in whole texels
    /// of the sun's view, and the centre's depth (the dynamic map shares the depth range).
    pub(crate) radius: f32,
    pub(crate) tex: [i64; 2],
    pub(crate) z: f32,
    /// The chunks in it: key -> (build, had models).
    pub(crate) drawn: HashMap<(i32, i32), (u64, bool)>,
    pub(crate) last_draw: u64,
    /// `ALAZ_SHADOW_EVERY=1` (a check): redraw every frame, as before the static map existed.
    pub(crate) every_frame: bool,
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
pub(crate) fn visible_chunks(cam: &Camera, aspect: f32, world: &World) -> (i32, i32, i32, i32, (f32, f32, f32, f32)) {
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
pub(crate) struct PixelPass {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) uniform: wgpu::Buffer,
    pub(crate) group: wgpu::BindGroup,
    /// 0 = plain, 1 = film grade.
    pub(crate) cinema: f32,
}

/// Draws ink outlines from the scene's depth, at the small picture's resolution.
pub(crate) struct OutlinePass {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) group: wgpu::BindGroup,
}

pub struct Renderer {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) depth: wgpu::TextureView,
    pub(crate) scene: wgpu::TextureView,
    pub(crate) inked: wgpu::TextureView,
    pub(crate) outline: OutlinePass,
    pub(crate) pixel: PixelPass,
    /// Models and decals (and, with the mark pipelines, the player's outline).
    pub(crate) tri_pipeline: wgpu::RenderPipeline,
    /// The ground of a chunk whose bake is done, and of one whose is not (it works its colour out as it draws).
    pub(crate) ground_pipeline: wgpu::RenderPipeline,
    pub(crate) ground_live_pipeline: wgpu::RenderPipeline,
    pub(crate) bake_pipeline: wgpu::RenderPipeline,
    /// The ground's time-independent colour, drawn ahead of time (bake.rs).
    pub(crate) bake: GroundBake,
    /// Shader files watched for changes (ALAZ_HOTRELOAD).
    pub(crate) hot: Option<hotreload::HotReload>,
    /// The player's body marked in the picture's alpha (all of it, then the hidden part), for the outline pass to
    /// draw a bright line round whatever of it stands behind something.
    pub(crate) mark_pipeline: wgpu::RenderPipeline,
    pub(crate) hidden_pipeline: wgpu::RenderPipeline,
    pub(crate) chunks: HashMap<(i32, i32), Chunk>,
    pub(crate) frame_no: u64,
    pub(crate) stats: RenderStats,
    pub(crate) globals: wgpu::Buffer,
    pub(crate) height_tex: wgpu::Texture,
    /// Per tile of the height window: its ground picture (atlas texel corner), ground kind and shade, so the shader
    /// can draw a neighbour's ground across a ragged boundary instead of along tile edges.
    pub(crate) tile_tex: wgpu::Texture,
    /// (seed, tile corner of the window's first texel) of what the height texture holds now
    pub(crate) height_at: Option<(u64, i32, i32)>,
    pub(crate) weather_kind: f32,
    pub(crate) weather_amount: f32,
    pub(crate) wind: f32,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_bind_group: wgpu::BindGroup,
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_static_view: wgpu::TextureView,
    pub(crate) shadow_static_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_copy_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_copy_group: wgpu::BindGroup,
    pub(crate) shadow_static: StaticShadow,
    pub(crate) chunk_gen: u64,
    pub(crate) art: Art,
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_state: egui_winit::State,
    pub(crate) egui_renderer: egui_wgpu::Renderer,
    /// Per-creature walking animation state, keyed by agent id.
    pub(crate) anim: HashMap<u32, Walk>,
    /// The same for wild animals, keyed by their wild id.
    pub(crate) wild_anim: HashMap<u32, Walk>,
    pub(crate) last_frame: std::time::Instant,
    pub(crate) tick_seen: u64,
    pub(crate) tick_at: std::time::Instant,
    pub(crate) tick_gap: f32,
    /// Tile of the resource node under the pointer: drawn with a bright rim so it is clear what can be taken.
    pub hover_node: Option<(i32, i32)>,
    /// How far the game is between the last simulated tick and the next (0..1), when the caller knows it
    /// exactly; otherwise it is guessed from the time between ticks.
    pub sim_alpha: Option<f32>,
    /// The sun direction the shadows are cast along: it follows the sun in small steps, not every frame, so
    /// shadow edges hold still instead of shimmering as the sun moves.
    pub(crate) shadow_dir: [f32; 3],
    /// Where the cloud field has drifted to (it moves with the wind, so it is summed frame by frame).
    pub(crate) cloud_off: [f32; 2],
    pub(crate) prof: Option<GpuProf>,
    /// `ALAZ_SHOT`: where to write the picture, and how many frames to wait first
    pub(crate) shot: Option<(String, u32)>,
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
        // ALAZ_GPUPROF=1 times every pass on the GPU (when the adapter can) and prints it every 5 s
        let gpu_prof = std::env::var_os("ALAZ_GPUPROF").is_some() && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let features = if gpu_prof { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: features, ..Default::default() })).expect("request device");
        let prof = gpu_prof.then(|| GpuProf::new(&device, &queue));

        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface unsupported");
        // ALAZ_NOVSYNC lifts the 60 fps cap so performance tests measure real frame cost
        config.present_mode =
            if std::env::var_os("ALAZ_NOVSYNC").is_some() { wgpu::PresentMode::AutoNoVsync } else { wgpu::PresentMode::AutoVsync };
        // ALAZ_SHOT copies the finished frame out of the swapchain
        if std::env::var_os("ALAZ_SHOT").is_some() && surface.get_capabilities(&adapter).usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
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
            source: wgpu::ShaderSource::Wgsl(hotreload::SHADER.into()),
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
        let bake = GroundBake::new(&device, &globals, &atlas_view, &height_view, &tile_view);
        // sun shadows: depth seen from the sun, over the part of the world around the camera. The static map (what
        // stands still) is drawn rarely, over a box 1.5 times as wide with the same texel size; every frame its window
        // for the current box is copied into the dynamic map, and what moves is drawn over that.
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow"),
            size: wgpu::Extent3d { width: SHADOW_RES, height: SHADOW_RES, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_static_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow, static"),
            size: wgpu::Extent3d { width: SHADOW_STATIC_RES, height: SHADOW_STATIC_RES, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_static_view = shadow_static_tex.create_view(&Default::default());
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
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
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
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&bake.col_view) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&bake.nrm_view) },
            ],
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadow"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let shadow_pipeline = pipelines::shadow(&device, &shader, &shadow_layout, "vs_shadow");
        let shadow_static_pipeline = pipelines::shadow(&device, &shader, &shadow_layout, "vs_shadow_static");
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let shadow_copy_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow copy"),
            layout: &copy_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&shadow_static_view) },
            ],
        });
        let copy_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadow copy"), bind_group_layouts: &[Some(&copy_bgl)], immediate_size: 0 });
        let shadow_copy_pipeline = pipelines::shadow_copy(&device, &shader, &copy_layout);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl_main)], immediate_size: 0 });
        let tri_pipeline = pipelines::scene(&device, &shader, &layout, config.format);
        let ground_pipeline = pipelines::ground(&device, &shader, &layout, config.format, "fs_ground");
        let ground_live_pipeline = pipelines::ground(&device, &shader, &layout, config.format, "fs_ground_live");
        let bake_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ground bake"), bind_group_layouts: &[Some(&bake.layout)], immediate_size: 0 });
        let bake_pipeline = pipelines::bake(&device, &shader, &bake_layout);
        // the player drawn again into the picture's alpha only: where its body is, and where something hides it
        let mark_pipeline = pipelines::mark(&device, &layout, &shader, config.format, "vs", "fs_mark", wgpu::CompareFunction::Always);
        let hidden_pipeline = pipelines::mark(&device, &layout, &shader, config.format, "vs_hidden", "fs_hidden", wgpu::CompareFunction::Greater);
        let (w, h) = (config.width, config.height);
        let depth = Self::make_depth(&device, w, h);
        let scene = Self::make_color(&device, config.format, w, h);
        let inked = Self::make_color(&device, config.format, w, h);
        let (outline, outline_pl) = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("outline"),
                source: wgpu::ShaderSource::Wgsl(hotreload::OUTLINE.into()),
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
            let pipeline = pipelines::fullscreen(&device, "outline", &module, &pl, config.format);
            let group = Self::make_outline_group(&device, &layout, &scene, &depth);
            (OutlinePass { pipeline, layout, group }, pl)
        };
        let (pixel, pixel_pl) = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pixel"),
                source: wgpu::ShaderSource::Wgsl(hotreload::PIXEL.into()),
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
            let pipeline = pipelines::fullscreen(&device, "pixel", &module, &pl, config.format);
            let group = Self::make_pixel_group(&device, &layout, &inked, &uniform);
            let cinema = if std::env::var_os("ALAZ_NOCINEMA").is_some() { 0.0 } else { 1.0 };
            (PixelPass { pipeline, layout, uniform, group, cinema }, pl)
        };

        let hot = hotreload::HotReload::new(shadow_layout, copy_layout, layout, bake_layout, outline_pl, pixel_pl);
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
            ground_pipeline,
            ground_live_pipeline,
            bake_pipeline,
            bake,
            hot,
            mark_pipeline,
            hidden_pipeline,
            chunks: HashMap::new(),
            anim: HashMap::new(),
            wild_anim: HashMap::new(),
            sim_alpha: None,
            shadow_dir: [0.0, 1.0, 0.0],
            cloud_off: [0.0, 0.0],
            prof,
            shot: std::env::var("ALAZ_SHOT").ok().map(|p| (p, std::env::var("ALAZ_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(240))),
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
            shadow_static_view,
            shadow_static_pipeline,
            shadow_copy_pipeline,
            shadow_copy_group,
            shadow_static: StaticShadow { every_frame: std::env::var_os("ALAZ_SHADOW_EVERY").is_some(), ..Default::default() },
            chunk_gen: 0,
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

    /// Dev aid (`ALAZ_SHOT=file.ppm`, `ALAZ_SHOT_FRAMES=n`): writes the finished frame (weather and interface
    /// included) to a file and quits, so a screenshot never depends on which window the desktop has in front.
    fn write_shot(&self, tex: &wgpu::Texture, path: &str, w: u32, h: u32) {
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(data) = slice.get_mapped_range() else { return };
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in 0..h {
            for x in 0..w {
                let i = (y * row + x * 4) as usize;
                let (r, g, b) = if bgra { (data[i + 2], data[i + 1], data[i]) } else { (data[i], data[i + 1], data[i + 2]) };
                out.extend_from_slice(&[r, g, b]);
            }
        }
        let _ = std::fs::write(path, out);
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
}

