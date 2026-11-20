//! Pictures from `assets/` for the interface: item icons (`icons/<item>.png`), panel and button frames, the logo
//! (`ui/*.png`). Every lookup may come back empty; the callers then paint what they always painted.
//! Textures are made once per file (nearest filtering: pixels stay pixels) and made again when the file changed.

use alaz_render::assets;
use alaz_render::egui::{self, Color32, Pos2, Rect, Shape, TextureHandle, TextureOptions};
use alaz_sim::ItemKind;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CACHE: RefCell<HashMap<String, (u64, Option<TextureHandle>)>> = RefCell::new(HashMap::new());
}

/// Any pictures at all (so painting code can skip the lookups when the folder is empty).
pub fn any() -> bool {
    assets::has_pictures()
}

/// The texture of `assets/<path>`, with its size in pixels.
pub fn texture(ctx: &egui::Context, path: &str) -> Option<(TextureHandle, [usize; 2])> {
    let current = assets::generation();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((g, t)) = c.get(path)
            && *g == current {
                return t.clone().map(|t| {
                    let s = t.size();
                    (t, s)
                });
            }
        let tex = assets::image(path).map(|img| {
            let size = [img.w as usize, img.h as usize];
            ctx.load_texture(format!("asset:{path}"), egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba), TextureOptions::NEAREST)
        });
        c.insert(path.to_string(), (current, tex.clone()));
        tex.map(|t| {
            let s = t.size();
            (t, s)
        })
    })
}

/// `icons/<item>.png` for an item, if the file is there.
pub fn item_icon(ctx: &egui::Context, k: ItemKind) -> Option<TextureHandle> {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| ItemKind::ALL.iter().map(|k| assets::snake(&format!("{k:?}"))).collect());
    let id = ids.get(k as usize)?;
    texture(ctx, &format!("icons/{id}.png")).map(|(t, _)| t)
}

/// A frame picture cut in nine: the corners keep their size (one picture pixel = `px` points), the edges and the
/// middle stretch. The border is a third of the picture's shorter side.
pub fn nine_slice(tex: &TextureHandle, size: [usize; 2], rect: Rect, px: f32, tint: Color32) -> Vec<Shape> {
    let (w, h) = (size[0] as f32, size[1] as f32);
    let b = (size[0].min(size[1]) / 3).max(1) as f32;
    let (bx, by) = ((b * px).min(rect.width() / 2.0), (b * px).min(rect.height() / 2.0));
    let xs = [rect.left(), rect.left() + bx, rect.right() - bx, rect.right()];
    let ys = [rect.top(), rect.top() + by, rect.bottom() - by, rect.bottom()];
    let us = [0.0, b / w, 1.0 - b / w, 1.0];
    let vs = [0.0, b / h, 1.0 - b / h, 1.0];
    let mut out = Vec::with_capacity(9);
    for j in 0..3 {
        for i in 0..3 {
            let r = Rect::from_min_max(Pos2::new(xs[i], ys[j]), Pos2::new(xs[i + 1], ys[j + 1]));
            let uv = Rect::from_min_max(Pos2::new(us[i], vs[j]), Pos2::new(us[i + 1], vs[j + 1]));
            if r.width() > 0.0 && r.height() > 0.0 {
                out.push(Shape::image(tex.id(), r, uv, tint));
            }
        }
    }
    out
}

/// `ui/<name>_<era>.png` for the era the interface is in, else `ui/<name>.png`.
pub fn era_texture(ctx: &egui::Context, name: &str, era: u8) -> Option<(TextureHandle, [usize; 2])> {
    texture(ctx, &format!("ui/{name}_{era}.png")).or_else(|| texture(ctx, &format!("ui/{name}.png")))
}
