//! Render pipeline builders. Each takes the shader module and the (unchanging) layouts, so the same code builds the
//! pipelines at start-up and again when a shader is hot-reloaded.
use crate::mesh::Vertex;
use crate::{DEPTH_FORMAT, SHADOW_FORMAT};

const VERTEX_ATTRS: [wgpu::VertexAttribute; 6] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32, 3 => Float32x2, 4 => Float32x2, 5 => Float32];

fn vertex_buffer() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &VERTEX_ATTRS }
}

/// The sun's depth pass (shader.wgsl).
pub fn shadow(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, vs: &str) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[Some(vertex_buffer())] },
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_shadow"), compilation_options: Default::default(), targets: &[] }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SHADOW_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Starts the dynamic shadow map from a window of the static one: a full-screen triangle that writes depth only.
pub fn shadow_copy(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow copy"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_copy"), compilation_options: Default::default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs_copy"), compilation_options: Default::default(), targets: &[] }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SHADOW_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// The main scene pipeline (shader.wgsl).
pub fn scene(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[Some(vertex_buffer())] },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
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
    })
}

/// The ground (shader.wgsl): `fragment` is "fs_ground" (reads the bake) or "fs_ground_live" (works the colour out).
/// Its vertex stage hands over only the position; the models' `scene` pipeline serves everything else.
pub fn ground(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat, fragment: &str) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_ground"), compilation_options: Default::default(), buffers: &[Some(vertex_buffer())] },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
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
    })
}

/// The ground bake (shader.wgsl): a block of ground texels into the two bake textures.
pub fn bake(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ground bake"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_bake"), compilation_options: Default::default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_bake"),
            compilation_options: Default::default(),
            targets: &[Some(crate::bake::COL_FORMAT.into()), Some(crate::bake::NRM_FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// The player drawn into the picture's alpha only (shader.wgsl).
pub fn mark(device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat, vs: &str, fs: &str, compare: wgpu::CompareFunction) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fs),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[Some(vertex_buffer())] },
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

/// A full-screen pass without vertex buffers or depth (outline.wgsl, pixel.wgsl).
pub fn fullscreen(device: &wgpu::Device, label: &str, module: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(format.into())] }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// The bind group layouts and pipeline layouts the pipelines are built against. They do not change while the game runs
/// (the bake's follows its ring, which a change of pixel detail replaces).
pub struct Layouts {
    /// Globals, atlas, heights: what the shadow passes bind.
    pub shadow_bgl: wgpu::BindGroupLayout,
    /// Everything the main pass reads.
    pub main_bgl: wgpu::BindGroupLayout,
    /// Globals and the static shadow map: the copy that starts the dynamic one.
    pub copy_bgl: wgpu::BindGroupLayout,
    pub shadow: wgpu::PipelineLayout,
    pub copy: wgpu::PipelineLayout,
    pub scene: wgpu::PipelineLayout,
    pub bake: wgpu::PipelineLayout,
    pub outline: wgpu::PipelineLayout,
    pub pixel: wgpu::PipelineLayout,
}

/// Every pipeline built from shader.wgsl.
pub struct SceneSet {
    pub shadow: wgpu::RenderPipeline,
    pub shadow_static: wgpu::RenderPipeline,
    pub shadow_copy: wgpu::RenderPipeline,
    pub tri: wgpu::RenderPipeline,
    pub ground: wgpu::RenderPipeline,
    pub ground_live: wgpu::RenderPipeline,
    pub bake: wgpu::RenderPipeline,
    pub mark: wgpu::RenderPipeline,
    pub hidden: wgpu::RenderPipeline,
}

impl SceneSet {
    pub fn build(device: &wgpu::Device, m: &wgpu::ShaderModule, l: &Layouts, format: wgpu::TextureFormat) -> SceneSet {
        SceneSet {
            shadow: shadow(device, m, &l.shadow, "vs_shadow"),
            shadow_static: shadow(device, m, &l.shadow, "vs_shadow_static"),
            shadow_copy: shadow_copy(device, m, &l.copy),
            tri: scene(device, m, &l.scene, format),
            ground: ground(device, m, &l.scene, format, "fs_ground"),
            ground_live: ground(device, m, &l.scene, format, "fs_ground_live"),
            bake: bake(device, m, &l.bake),
            mark: mark(device, &l.scene, m, format, "vs", "fs_mark", wgpu::CompareFunction::Always),
            hidden: mark(device, &l.scene, m, format, "vs_hidden", "fs_hidden", wgpu::CompareFunction::Greater),
        }
    }
}
