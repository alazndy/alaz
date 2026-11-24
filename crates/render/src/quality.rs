//! Graphics quality the player can choose (and `ALAZ_PIXELS`, `ALAZ_SHADOWS`, `ALAZ_DECOR` force for measuring).
//! The pixel size reaches the camera, the painted ground tiles and the ground shader, so it is a process-wide setting
//! that `Renderer::set_quality` changes and rebuilds the GPU side for; everything else reads it from here.
use std::sync::atomic::{AtomicBool, Ordering};

/// How fine the world's texels are.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PixelDetail {
    /// 27 texels to the metre (the picture is 5/3 as fine as the painted art).
    #[default]
    Fine,
    /// 21 texels to the metre (4/3): fewer pixels to shade, for slow GPUs.
    Coarse,
}

/// Resolution of the sun's shadow maps, or none.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ShadowQuality {
    /// Dynamic 2048 squared, static 3072 squared.
    #[default]
    High,
    /// Half the resolution on a side (texels twice as large, same box).
    Low,
    /// No shadow pass at all.
    Off,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Quality {
    pub pixels: PixelDetail,
    pub shadows: ShadowQuality,
    /// Grass tufts, flowers and pebbles in the distance.
    pub decor: bool,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { pixels: PixelDetail::Fine, shadows: ShadowQuality::High, decor: true }
    }
}

impl PixelDetail {
    pub fn key(self) -> &'static str {
        match self {
            PixelDetail::Fine => "fine",
            PixelDetail::Coarse => "coarse",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fine" => Some(PixelDetail::Fine),
            "coarse" => Some(PixelDetail::Coarse),
            _ => None,
        }
    }
}

impl ShadowQuality {
    pub fn key(self) -> &'static str {
        match self {
            ShadowQuality::High => "high",
            ShadowQuality::Low => "low",
            ShadowQuality::Off => "off",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "high" => Some(ShadowQuality::High),
            "low" => Some(ShadowQuality::Low),
            "off" => Some(ShadowQuality::Off),
            _ => None,
        }
    }
    /// Texels on a side of the dynamic and of the static shadow map (a stub for Off, which never draws them).
    pub(crate) fn sizes(self) -> (u32, u32) {
        match self {
            ShadowQuality::High => (2048, 3072),
            ShadowQuality::Low | ShadowQuality::Off => (1024, 1536),
        }
    }
    /// Sizes of the textures actually allocated.
    pub(crate) fn alloc(self) -> (u32, u32) {
        if self == ShadowQuality::Off {
            (4, 4)
        } else {
            self.sizes()
        }
    }
}

impl Quality {
    /// The environment overrides (a screenshot or a measurement must not depend on what the player saved).
    pub fn with_env(mut self) -> Self {
        if let Some(p) = std::env::var("ALAZ_PIXELS").ok().and_then(|v| PixelDetail::parse(&v)) {
            self.pixels = p;
        }
        if let Some(s) = std::env::var("ALAZ_SHADOWS").ok().and_then(|v| ShadowQuality::parse(&v)) {
            self.shadows = s;
        }
        if let Ok(v) = std::env::var("ALAZ_DECOR") {
            self.decor = !matches!(v.trim(), "0" | "off" | "false" | "no");
        }
        self
    }
}

static COARSE: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_pixels(p: PixelDetail) {
    COARSE.store(p == PixelDetail::Coarse, Ordering::Relaxed);
}

pub fn pixels() -> PixelDetail {
    if COARSE.load(Ordering::Relaxed) {
        PixelDetail::Coarse
    } else {
        PixelDetail::Fine
    }
}

/// Ground texels to the metre (and the side of a ground tile in the atlas).
pub fn ground_texels() -> i32 {
    match pixels() {
        PixelDetail::Fine => 27,
        PixelDetail::Coarse => 21,
    }
}

/// How much finer the picture is than the painted art's 16 texels per metre.
pub fn res() -> f32 {
    match pixels() {
        PixelDetail::Fine => 5.0 / 3.0,
        PixelDetail::Coarse => 4.0 / 3.0,
    }
}

/// How many times finer than the small picture the scene is drawn before it is averaged down to it (2 = 4 samples
/// to a texel). The texel grid does not move with the camera's turning, so a single sample per texel flickers along
/// every edge as the camera turns; averaging a few samples settles it. `ALAZ_SUPERSAMPLE=1` turns it off, 3 is the
/// most.
pub fn supersample() -> u32 {
    static SS: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *SS.get_or_init(|| std::env::var("ALAZ_SUPERSAMPLE").ok().and_then(|v| v.trim().parse::<u32>().ok()).map_or(2, |n| n.clamp(1, 3)))
}

/// The outline and pixel shaders with the supersampling factor put in front as a constant.
pub(crate) fn ss_source(body: &str) -> String {
    format!("const SS: i32 = {};\n{body}", supersample())
}

/// Numbers the scene shader needs, as constants put in front of its source (so the compiler folds them as it did
/// when they were typed in).
pub(crate) fn shader_header(q: Quality) -> String {
    let gt = match q.pixels {
        PixelDetail::Fine => 27,
        PixelDetail::Coarse => 21,
    };
    format!(
        "const GT: i32 = {gt}; const GTF: f32 = {gt}.0; const RING_TEXELS: i32 = {}; const SHADOWS: bool = {}; const SSF: f32 = {}.0;\n",
        4 * crate::renderer::CHUNK_TILES * gt,
        q.shadows != ShadowQuality::Off,
        supersample()
    )
}

/// The scene shader's full source for this quality.
pub(crate) fn scene_source(body: &str, q: Quality) -> String {
    format!("{}{body}", shader_header(q))
}
