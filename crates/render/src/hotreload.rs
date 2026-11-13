//! Shader hot reload. With ALAZ_HOTRELOAD=1 (on by default in debug builds, ALAZ_HOTRELOAD=0 turns it off) the .wgsl
//! files are watched on disk; when one changes its module and every pipeline built from it are rebuilt. A shader that
//! fails to compile leaves the old pipelines in place and prints the error. Shipped builds keep the embedded sources.
use crate::{pipelines, Renderer};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub const SHADER: &str = include_str!("shader.wgsl");
pub const OUTLINE: &str = include_str!("outline.wgsl");
pub const PIXEL: &str = include_str!("pixel.wgsl");

const POLL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, PartialEq)]
enum Which {
    Scene,
    Outline,
    Pixel,
}

struct Watched {
    which: Which,
    name: &'static str,
    path: PathBuf,
    seen: Option<SystemTime>,
}

/// The unchanging pipeline layouts plus the files being watched.
pub struct HotReload {
    files: Vec<Watched>,
    next_check: Instant,
    pub shadow_layout: wgpu::PipelineLayout,
    pub copy_layout: wgpu::PipelineLayout,
    pub scene_layout: wgpu::PipelineLayout,
    pub bake_layout: wgpu::PipelineLayout,
    pub outline_layout: wgpu::PipelineLayout,
    pub pixel_layout: wgpu::PipelineLayout,
}

fn enabled() -> bool {
    match std::env::var("ALAZ_HOTRELOAD") {
        Ok(v) => v != "0" && !v.is_empty(),
        Err(_) => cfg!(debug_assertions),
    }
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl HotReload {
    pub fn new(
        shadow_layout: wgpu::PipelineLayout,
        copy_layout: wgpu::PipelineLayout,
        scene_layout: wgpu::PipelineLayout,
        bake_layout: wgpu::PipelineLayout,
        outline_layout: wgpu::PipelineLayout,
        pixel_layout: wgpu::PipelineLayout,
    ) -> Option<HotReload> {
        if !enabled() {
            return None;
        }
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let watch = |which, name: &'static str| {
            let path = dir.join(name);
            Watched { which, seen: mtime(&path), name, path }
        };
        eprintln!("[hotreload] shader dosyaları izleniyor: {}", dir.display());
        Some(HotReload {
            files: vec![watch(Which::Scene, "shader.wgsl"), watch(Which::Outline, "outline.wgsl"), watch(Which::Pixel, "pixel.wgsl")],
            next_check: Instant::now() + POLL,
            shadow_layout,
            copy_layout,
            scene_layout,
            bake_layout,
            outline_layout,
            pixel_layout,
        })
    }
}

/// Compiles `src` and builds everything from it inside a validation scope; Err carries the first error message.
fn guarded<T>(device: &wgpu::Device, build: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let out = build();
    match pollster::block_on(scope.pop()) {
        Some(wgpu::Error::Validation { source, description }) => {
            // the compiler's message (with line and column) sits down the source chain
            let mut msg = description;
            let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(&*source);
            while let Some(e) = cur {
                msg = format!("{msg}\n{e}");
                cur = e.source();
            }
            Err(msg)
        }
        Some(e) => Err(e.to_string()),
        None => Ok(out),
    }
}

impl Renderer {
    /// Called once a frame; looks at the files about four times a second and rebuilds what changed.
    pub(crate) fn hot_reload_poll(&mut self) {
        let Some(hot) = self.hot.as_mut() else { return };
        let now = Instant::now();
        if now < hot.next_check {
            return;
        }
        hot.next_check = now + POLL;
        let mut changed = Vec::new();
        for f in &mut hot.files {
            let m = mtime(&f.path);
            if m.is_some() && m != f.seen {
                f.seen = m;
                changed.push((f.which, f.name, f.path.clone()));
            }
        }
        for (which, name, path) in changed {
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[hotreload] {name}: okunamadı: {e}");
                    continue;
                }
            };
            match self.rebuild(which, name, &src) {
                Ok(()) => eprintln!("[hotreload] {name} yenilendi"),
                Err(e) => eprintln!("[hotreload] {name}: {e}"),
            }
        }
    }

    fn rebuild(&mut self, which: Which, name: &str, src: &str) -> Result<(), String> {
        let device = &self.device;
        let format = self.config.format;
        let hot = self.hot.as_ref().expect("hot reload on");
        let module = |label| device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) });
        match which {
            Which::Scene => {
                let (shadow, shadow_static, shadow_copy, tri, ground, ground_live, bake, mark, hidden) = guarded(device, || {
                    let m = module(name);
                    (
                        pipelines::shadow(device, &m, &hot.shadow_layout, "vs_shadow"),
                        pipelines::shadow(device, &m, &hot.shadow_layout, "vs_shadow_static"),
                        pipelines::shadow_copy(device, &m, &hot.copy_layout),
                        pipelines::scene(device, &m, &hot.scene_layout, format),
                        pipelines::ground(device, &m, &hot.scene_layout, format, "fs_ground"),
                        pipelines::ground(device, &m, &hot.scene_layout, format, "fs_ground_live"),
                        pipelines::bake(device, &m, &hot.bake_layout),
                        pipelines::mark(device, &hot.scene_layout, &m, format, "vs", "fs_mark", wgpu::CompareFunction::Always),
                        pipelines::mark(device, &hot.scene_layout, &m, format, "vs_hidden", "fs_hidden", wgpu::CompareFunction::Greater),
                    )
                })?;
                self.shadow_pipeline = shadow;
                self.shadow_static_pipeline = shadow_static;
                self.shadow_copy_pipeline = shadow_copy;
                // (the static map was drawn with the old shader)
                self.shadow_static.valid = false;
                self.tri_pipeline = tri;
                self.ground_pipeline = ground;
                self.ground_live_pipeline = ground_live;
                self.bake_pipeline = bake;
                self.bake.invalidate();
                // (the static colour is shader code, so what was baked is out of date)
                self.mark_pipeline = mark;
                self.hidden_pipeline = hidden;
            }
            Which::Outline => {
                self.outline.pipeline = guarded(device, || pipelines::fullscreen(device, "outline", &module(name), &hot.outline_layout, format))?;
            }
            Which::Pixel => {
                self.pixel.pipeline = guarded(device, || pipelines::fullscreen(device, "pixel", &module(name), &hot.pixel_layout, format))?;
            }
        }
        Ok(())
    }
}
