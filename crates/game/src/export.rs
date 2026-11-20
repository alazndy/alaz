//! `alaz --export-assets <folder>`: writes what the game draws in code out as editable files. Models and ground tiles
//! come from the render crate; the item icons are painted by the interface, so they are drawn here into a small
//! software canvas (no window, no GPU) and saved as PNG, 64 x 64 in `icons/` and 32 x 32 in `icons_32/`.

use alaz_render::egui::{self, epaint::Primitive, Color32, Pos2, Rect};
use alaz_render::assets;
use alaz_sim::ItemKind;
use std::path::Path;

/// Paints `k` with the interface's own icon code on a `size` x `size` canvas, at 4x and shrunk back by averaging, and
/// returns straight RGBA8.
pub fn render_icon(k: ItemKind, size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(SS as f32);
    let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(size as f32, size as f32));
    let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| {
        crate::icons::draw_item(ui.painter(), screen, k);
    });
    let prims = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
    // (no GPU to hand the font texture to)
    out.drop_without_applying_deltas();
    let dim = (size * SS) as usize;
    // premultiplied RGBA, 0..255 as floats
    let mut canvas = vec![[0.0f32; 4]; dim * dim];
    for prim in &prims {
        let Primitive::Mesh(mesh) = &prim.primitive else { continue };
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v: [_; 3] = std::array::from_fn(|i| mesh.vertices[tri[i] as usize]);
            let p: [[f32; 2]; 3] = std::array::from_fn(|i| [v[i].pos.x * SS as f32, v[i].pos.y * SS as f32]);
            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if area.abs() < 1e-9 {
                continue;
            }
            let (x0, x1) = (p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(dim - 1));
            let (y0, y1) = (p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(dim - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1][0] - cx) * (p[2][1] - cy) - (p[2][0] - cx) * (p[1][1] - cy)) / area;
                    let w1 = ((p[2][0] - cx) * (p[0][1] - cy) - (p[0][0] - cx) * (p[2][1] - cy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    // the vertex colours are premultiplied; the texture lookup is the font atlas's white texel
                    let c: [f32; 4] = std::array::from_fn(|k| {
                        let ch = |col: Color32| col.to_array()[k] as f32;
                        w0 * ch(v[0].color) + w1 * ch(v[1].color) + w2 * ch(v[2].color)
                    });
                    let d = &mut canvas[y * dim + x];
                    let inv = 1.0 - c[3] / 255.0;
                    for k in 0..4 {
                        d[k] = c[k] + d[k] * inv;
                    }
                }
            }
        }
    }
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let mut s = [0.0f32; 4];
            for sy in 0..SS as usize {
                for sx in 0..SS as usize {
                    let c = canvas[(y * SS as usize + sy) * dim + x * SS as usize + sx];
                    for k in 0..4 {
                        s[k] += c[k];
                    }
                }
            }
            let n = (SS * SS) as f32;
            let a = s[3] / n;
            if a < 0.5 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                // un-premultiply
                for k in 0..3 {
                    rgba.push((s[k] / n / a * 255.0).round().clamp(0.0, 255.0) as u8);
                }
                rgba.push(a.round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    rgba
}

/// Runs the export: models, tiles, icons. Prints what it wrote.
pub fn run(dir: &Path) {
    let mut written = match alaz_render::export_world_assets(dir) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("dışa aktarma başarısız: {e}");
            std::process::exit(1);
        }
    };
    assets::pause(true);
    for (sub, size) in [("icons", 64u32), ("icons_32", 32)] {
        if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
            eprintln!("{sub}: {e}");
            std::process::exit(1);
        }
        for k in ItemKind::ALL {
            let name = format!("{sub}/{}.png", assets::snake(&format!("{k:?}")));
            let png = assets::encode_png(size, size, &render_icon(k, size));
            if let Err(e) = std::fs::write(dir.join(&name), png) {
                eprintln!("{name}: {e}");
                std::process::exit(1);
            }
            written.push(name);
        }
    }
    assets::pause(false);
    println!("{} dosya yazıldı: {}", written.len(), dir.display());
    println!("  models/*.glb   : yapılar ve doğa nesneleri (düzenleyip assets/models/'e koy)");
    println!("  icons/*.png    : eşya ikonları 64x64 (assets/icons/'a koy); icons_32/ aynıları 32x32");
    println!("  tiles/*.png    : zemin dokuları (assets/tiles/'a koy)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_paints_something() {
        for k in ItemKind::ALL {
            let px = render_icon(k, 32);
            let covered = px.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
            assert!(covered > 32 * 32 / 50, "{k:?}: only {covered} of 1024 pixels painted");
        }
    }

    #[test]
    fn an_icon_survives_the_png_round_trip() {
        let px = render_icon(ItemKind::Stone, 64);
        let img = assets::decode_png(&assets::encode_png(64, 64, &px)).unwrap();
        assert_eq!((img.w, img.h), (64, 64));
        assert_eq!(img.rgba, px);
    }
}
