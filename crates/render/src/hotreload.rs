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

/// The files being watched.
pub struct HotReload {
    files: Vec<Watched>,
    next_check: Instant,
}

pub(crate) fn enabled() -> bool {
    match std::env::var("ALAZ_HOTRELOAD") {
        Ok(v) => v != "0" && !v.is_empty(),
        Err(_) => cfg!(debug_assertions),
    }
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl HotReload {
    pub fn new() -> Option<HotReload> {
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
        })
    }

    /// The scene shader as it stands on disk (a change of quality rebuilds from it, not from the embedded copy).
    pub fn scene_source(&self) -> Option<String> {
        let f = self.files.iter().find(|f| f.which == Which::Scene)?;
        std::fs::read_to_string(&f.path).ok()
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

    /// Called once a frame: when asset files changed on disk (see `assets::poll`) the ground tiles are painted again and
    /// every built chunk is dropped, so models and pictures show their new version.
    pub(crate) fn assets_poll(&mut self) {
        crate::assets::poll();
        let g = crate::assets::generation();
        if g == self.assets_gen {
            return;
        }
        self.assets_gen = g;
        self.repaint_atlas();
        self.refresh_groups();
        self.shadow_static.valid = false;
    }

    fn rebuild(&mut self, which: Which, name: &str, src: &str) -> Result<(), String> {
        let device = &self.device;
        let format = self.config.format;
        let l = &self.layouts;
        let module = |label, src: &str| device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) });
        match which {
            Which::Scene => {
                let q = self.quality;
                let set = guarded(device, || pipelines::SceneSet::build(device, &module(name, &crate::quality::scene_source(src, q)), l, format))?;
                // (the static map was drawn with the old shader, and so was what the bake holds)
                self.apply_scene(set);
            }
            Which::Outline => {
                self.outline.pipeline = guarded(device, || pipelines::fullscreen(device, "outline", &module(name, src), &l.outline, format))?;
            }
            Which::Pixel => {
                self.pixel.pipeline = guarded(device, || pipelines::fullscreen(device, "pixel", &module(name, src), &l.pixel, format))?;
            }
        }
        Ok(())
    }
}
