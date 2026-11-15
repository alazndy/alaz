//! Asset files: 3D models (.vox, .glb), pictures (.png) and their `.toml` side files, from `assets/` at the repository
//! root. A shipped binary carries the folder inside it (see build.rs); with `ALAZ_ASSETS=<folder>` the files are read
//! from disk instead, and with hot reload on (`ALAZ_HOTRELOAD=1`, the default in debug builds) a saved file is picked
//! up while the game runs. `ALAZ_NOASSETS=1` ignores every file (to compare against the painted-in-code look).
//!
//! Everything here is a lookup that may come back empty: the game draws the code-made version then. The store is
//! shared by the chunk-building threads, so models are parsed once and handed out as `Arc`s.

mod glb;
mod model;
mod vox;

pub use glb::{write_glb, GlbPart};
#[cfg(test)]
pub(crate) use glb::load as load_glb;
pub use model::{Model, ModelSet, Opts, Part, Place, Role, MV};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// A decoded picture, 8-bit RGBA, rows top to bottom.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

#[derive(Default)]
struct Files {
    bytes: HashMap<String, Arc<Vec<u8>>>,
    models: HashMap<String, Option<Arc<ModelSet>>>,
    images: HashMap<String, Option<Arc<Image>>>,
}

struct Store {
    /// Where the files are read from when `ALAZ_ASSETS` is set (None: the embedded copy).
    dir: Option<PathBuf>,
    files: RwLock<Files>,
    /// Bumped whenever the files changed on disk; what was built from them is out of date then.
    generation: AtomicU64,
    /// Any model file at all (so the common case, an empty folder, costs one atomic load per lookup).
    any_models: AtomicBool,
    any_pictures: AtomicBool,
    watch: Mutex<Watch>,
}

struct Watch {
    next: Instant,
    seen: Vec<(String, u64, Option<SystemTime>)>,
}

const POLL: Duration = Duration::from_millis(250);

fn any_picture(f: &Files) -> bool {
    f.bytes.keys().any(|k| (k.starts_with("icons/") || k.starts_with("ui/")) && k.ends_with(".png"))
}

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| {
        let off = std::env::var_os("ALAZ_NOASSETS").is_some_and(|v| v != "0");
        let dir = if off { None } else { std::env::var_os("ALAZ_ASSETS").filter(|v| !v.is_empty()).map(PathBuf::from) };
        let mut files = Files::default();
        let mut seen = Vec::new();
        match &dir {
            Some(d) => {
                seen = scan(d, &mut files);
                eprintln!("[assets] {} dosya okundu: {}", files.bytes.len(), d.display());
            }
            None if !off => {
                for (name, data) in EMBEDDED {
                    files.bytes.insert((*name).to_string(), Arc::new(data.to_vec()));
                }
            }
            None => {}
        }
        let any = files.bytes.keys().any(|k| k.starts_with("models/") && (k.ends_with(".vox") || k.ends_with(".glb")));
        let pics = any_picture(&files);
        Store { dir, files: RwLock::new(files), generation: AtomicU64::new(1), any_models: AtomicBool::new(any), any_pictures: AtomicBool::new(pics), watch: Mutex::new(Watch { next: Instant::now() + POLL, seen }) }
    })
}

/// Reads every asset file under `dir` into `files`; returns the (path, length, modified) list that tells a change.
fn scan(dir: &Path, files: &mut Files) -> Vec<(String, u64, Option<SystemTime>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut e: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        e.sort();
        for p in e {
            if p.is_dir() {
                walk(&p, root, out);
            } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "vox" | "glb" | "png" | "toml")) {
                out.push((p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
    let mut list = Vec::new();
    walk(dir, dir, &mut list);
    let mut seen = Vec::new();
    for (rel, p) in list {
        let meta = std::fs::metadata(&p).ok();
        seen.push((rel.clone(), meta.as_ref().map(|m| m.len()).unwrap_or(0), meta.and_then(|m| m.modified().ok())));
        if let Ok(b) = std::fs::read(&p) {
            files.bytes.insert(rel, Arc::new(b));
        }
    }
    seen
}

/// While set (by the exporter) every lookup comes back empty, so the code-made version is what gets written out.
static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn pause(on: bool) {
    PAUSED.store(on, Ordering::Relaxed);
}

/// Counts up whenever the files changed (never goes back); callers compare it with the number they last built from.
pub fn generation() -> u64 {
    store().generation.load(Ordering::Relaxed)
}

/// True when some model file exists (cheap).
pub fn has_models() -> bool {
    store().any_models.load(Ordering::Relaxed) && !PAUSED.load(Ordering::Relaxed)
}

/// True when some picture exists under `assets/icons/` or `assets/ui/` (cheap; the interface asks before it looks).
pub fn has_pictures() -> bool {
    let s = store();
    s.any_pictures.load(Ordering::Relaxed) && !PAUSED.load(Ordering::Relaxed)
}

/// Hot reload: call once a frame. Looks at the folder about four times a second and reloads what changed.
pub fn poll() {
    let s = store();
    let Some(dir) = &s.dir else { return };
    if !crate::hotreload::enabled() {
        return;
    }
    let Ok(mut w) = s.watch.lock() else { return };
    let now = Instant::now();
    if now < w.next {
        return;
    }
    w.next = now + POLL;
    let mut fresh = Files::default();
    let seen = scan(dir, &mut fresh);
    if seen == w.seen {
        return;
    }
    w.seen = seen;
    let any = fresh.bytes.keys().any(|k| k.starts_with("models/") && (k.ends_with(".vox") || k.ends_with(".glb")));
    let pics = any_picture(&fresh);
    if let Ok(mut f) = s.files.write() {
        *f = fresh;
    }
    s.any_models.store(any, Ordering::Relaxed);
    s.any_pictures.store(pics, Ordering::Relaxed);
    let g = s.generation.fetch_add(1, Ordering::Relaxed) + 1;
    eprintln!("[assets] değişiklik algılandı, yenilendi (#{g})");
}

/// The raw bytes of `assets/<path>` ("icons/stone.png").
pub fn bytes(path: &str) -> Option<Arc<Vec<u8>>> {
    store().files.read().ok()?.bytes.get(path).cloned()
}

/// Every file path under `assets/<dir>/` ("models"), sorted.
pub fn list(dir: &str) -> Vec<String> {
    let Ok(f) = store().files.read() else { return Vec::new() };
    let prefix = format!("{dir}/");
    let mut v: Vec<String> = f.bytes.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
    v.sort();
    v
}

/// A PNG picture, decoded once. `None` if there is no such file or it does not decode (the reason is printed once).
pub fn image(path: &str) -> Option<Arc<Image>> {
    let s = store();
    if PAUSED.load(Ordering::Relaxed) {
        return None;
    }
    {
        let f = s.files.read().ok()?;
        if let Some(i) = f.images.get(path) {
            return i.clone();
        }
        if !f.bytes.contains_key(path) {
            return None;
        }
    }
    let raw = bytes(path)?;
    let img = match decode_png(&raw) {
        Ok(i) => Some(Arc::new(i)),
        Err(e) => {
            eprintln!("[assets] {path}: {e}");
            None
        }
    };
    s.files.write().ok()?.images.insert(path.to_string(), img.clone());
    img
}

/// Decodes any PNG to 8-bit RGBA.
pub fn decode_png(raw: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(raw));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut rd = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; rd.output_buffer_size().ok_or("picture too large")?];
    let info = rd.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let n = (w * h) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..n * 4].to_vec(),
        png::ColorType::Rgb => buf[..n * 3].chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => buf[..n].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..n * 2].chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Indexed => return Err("indexed picture was not expanded".into()),
    };
    Ok(Image { w, h, rgba })
}

/// Encodes 8-bit RGBA as a PNG.
pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut wr) = enc.write_header() {
            let _ = wr.write_image_data(rgba);
        }
    }
    out
}

/// A model by name ("campfire", "tree_pine"): the file `models/<name>.vox|.glb`, or one of `models/<name>_1`,
/// `<name>_2` ... picked by `seed`. `None` when there is no such file (or it is broken).
pub fn model(name: &str, seed: u32) -> Option<Arc<Model>> {
    let s = store();
    if !s.any_models.load(Ordering::Relaxed) || PAUSED.load(Ordering::Relaxed) {
        return None;
    }
    {
        let f = s.files.read().ok()?;
        if let Some(set) = f.models.get(name) {
            return set.as_ref().map(|m| m.pick(seed));
        }
    }
    let set = load_set(name).map(Arc::new);
    let out = set.as_ref().map(|m| m.pick(seed));
    s.files.write().ok()?.models.insert(name.to_string(), set);
    out
}

/// Every variant of a model, in file order (for the exporter's round trip and the viewer).
pub fn model_set(name: &str) -> Option<Arc<ModelSet>> {
    model(name, 0)?;
    store().files.read().ok()?.models.get(name)?.clone()
}

fn load_set(name: &str) -> Option<ModelSet> {
    let f = store().files.read().ok()?;
    let mut stems: Vec<&str> = Vec::new();
    for k in f.bytes.keys() {
        let Some(rest) = k.strip_prefix("models/") else { continue };
        let Some(stem) = rest.strip_suffix(".vox").or_else(|| rest.strip_suffix(".glb")) else { continue };
        let is_variant = stem == name || stem.strip_prefix(name).and_then(|r| r.strip_prefix('_')).is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        if is_variant {
            stems.push(stem);
        }
    }
    if stems.is_empty() {
        return None;
    }
    // (numbered ones in numeric order: _2 before _10)
    stems.sort_by_key(|s| (s.len(), *s));
    stems.dedup();
    let mut variants = Vec::new();
    for stem in stems {
        let side = f.bytes.get(&format!("models/{stem}.toml")).or_else(|| f.bytes.get(&format!("models/{name}.toml")));
        let opts = Opts::parse(side.map(|b| String::from_utf8_lossy(b).into_owned()).as_deref());
        let (path, data) = match (f.bytes.get(&format!("models/{stem}.glb")), f.bytes.get(&format!("models/{stem}.vox"))) {
            (Some(d), _) => (format!("models/{stem}.glb"), d.clone()),
            (None, Some(d)) => (format!("models/{stem}.vox"), d.clone()),
            _ => continue,
        };
        let r = if path.ends_with(".glb") { glb::load(&data, &opts) } else { vox::load(&data, &opts) };
        match r {
            Ok(m) => variants.push(Arc::new(m)),
            Err(e) => eprintln!("[assets] {path}: {e}"),
        }
    }
    (!variants.is_empty()).then_some(ModelSet { variants })
}

/// `CharcoalKiln` -> `charcoal_kiln`.
pub fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The file name of a building's model.
pub fn building_name(kind: alaz_sim::BuildingKind) -> &'static str {
    static NAMES: OnceLock<HashMap<alaz_sim::BuildingKind, &'static str>> = OnceLock::new();
    NAMES.get_or_init(|| alaz_sim::BuildingKind::ALL.iter().map(|&k| (k, &*Box::leak(snake(&format!("{k:?}")).into_boxed_str()))).collect()).get(&kind).copied().unwrap_or("building")
}

/// The file name of a scenery node's model.
pub fn node_name(kind: alaz_sim::NodeKind) -> &'static str {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    let names = NAMES.get_or_init(|| NODE_KINDS.iter().map(|&k| &*Box::leak(snake(&format!("{k:?}")).into_boxed_str())).collect());
    NODE_KINDS.iter().position(|&k| k == kind).map(|i| names[i]).unwrap_or("node")
}

pub const NODE_KINDS: [alaz_sim::NodeKind; 11] = {
    use alaz_sim::NodeKind::*;
    [StoneOutcrop, Tree, BerryBush, ClayDeposit, IronOutcrop, HerbPatch, CopperOutcrop, TinOutcrop, Deadwood, FiberPlant, CaveMouth]
};

/// The preview screen (`ALAZ_DEMO_VIEW=<name>`): one model standing alone, shown at work and at rest by turns.
pub struct View {
    pub name: String,
    pub at: Option<[f32; 2]>,
}

impl View {
    pub fn new(name: String) -> View {
        eprintln!("[view] {name}: model dosyası ya da koddaki yapı; dosya kaydedilince yenilenir (ALAZ_ASSETS + ALAZ_HOTRELOAD=1)");
        View { name, at: None }
    }

    /// The model into `out`: the file if there is one, else the code-made building of that name.
    pub(crate) fn push(&self, out: &mut crate::mesh::Mesh, tick: u64) {
        let Some([x, z]) = self.at else { return };
        // three seconds at rest, three at work
        let active = (tick / 180) % 2 == 1;
        if let Some(m) = model(&self.name, 0) {
            m.place(out, &Place::at(x, z).working(active, tick));
            return;
        }
        if let Some(kind) = alaz_sim::BuildingKind::ALL.iter().find(|&&k| building_name(k) == self.name) {
            crate::mesh::push_building(out, *kind, x, z, active, tick, None);
        }
    }
}
