//! Hand-drawn style vector icons for items, painted with egui shapes (no image files).

use alaz_render::egui::{epaint::PathShape, Color32, Painter, Pos2, Rect, Shape, Stroke};
use alaz_sim::ItemKind;

fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Draw `k` inside `rect`. Coordinates below are in a unit square (0..1, y down).
pub fn draw_item(p: &Painter, rect: Rect, k: ItemKind) {
    let s = rect.width().min(rect.height());
    let o = rect.center() - alaz_render::egui::vec2(s / 2.0, s / 2.0);
    let pt = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let poly = |pts: &[(f32, f32)], fill: Color32, stroke: Color32| {
        p.add(Shape::Path(PathShape::convex_polygon(pts.iter().map(|&(x, y)| pt(x, y)).collect(), fill, Stroke::new(s * 0.04, stroke))));
    };
    let line = |a: (f32, f32), b: (f32, f32), w: f32, col: Color32| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], Stroke::new(w * s, col));
    };
    let dot = |x: f32, y: f32, r: f32, col: Color32| {
        p.circle_filled(pt(x, y), r * s, col);
    };
    let ring = |x: f32, y: f32, r: f32, col: Color32, w: f32| {
        p.circle_stroke(pt(x, y), r * s, Stroke::new(w * s, col));
    };
    let wood = c(122, 86, 52);
    let wood_dark = c(80, 56, 34);
    let rock = |fill: Color32| {
        poly(&[(0.14, 0.62), (0.26, 0.34), (0.52, 0.22), (0.78, 0.36), (0.88, 0.62), (0.68, 0.8), (0.3, 0.78)], fill, c(60, 60, 64));
        line((0.3, 0.4), (0.5, 0.3), 0.03, Color32::from_white_alpha(70));
    };
    // a handle with a head on top, for the tools
    let tool = |head: &dyn Fn(), handle: Color32| {
        line((0.28, 0.9), (0.66, 0.26), 0.08, handle);
        head();
    };
    match k {
        ItemKind::Stone => rock(c(140, 140, 146)),
        ItemKind::SharpStone => {
            poly(&[(0.2, 0.76), (0.5, 0.14), (0.8, 0.7), (0.5, 0.84)], c(176, 176, 186), c(70, 70, 76));
            line((0.5, 0.2), (0.5, 0.74), 0.025, Color32::from_white_alpha(110));
        }
        ItemKind::Wood => {
            poly(&[(0.12, 0.34), (0.82, 0.34), (0.82, 0.74), (0.12, 0.74)], wood, wood_dark);
            p.circle_filled(pt(0.82, 0.54), 0.2 * s, c(190, 150, 100));
            ring(0.82, 0.54, 0.2, wood_dark, 0.03);
            ring(0.82, 0.54, 0.1, wood_dark, 0.02);
            line((0.2, 0.46), (0.7, 0.46), 0.02, wood_dark);
        }
        ItemKind::Hide => {
            poly(&[(0.12, 0.3), (0.4, 0.22), (0.6, 0.3), (0.88, 0.2), (0.8, 0.5), (0.9, 0.78), (0.6, 0.7), (0.4, 0.8), (0.14, 0.74), (0.22, 0.5)], c(150, 104, 66), c(86, 56, 34));
            line((0.3, 0.45), (0.7, 0.55), 0.02, Color32::from_white_alpha(60));
        }
        ItemKind::FiberWrap => {
            poly(&[(0.16, 0.26), (0.84, 0.26), (0.76, 0.8), (0.24, 0.8)], c(176, 190, 100), c(100, 112, 50));
            for k in 0..5 {
                let x = 0.26 + k as f32 * 0.12;
                line((x, 0.3), (x - 0.02, 0.76), 0.02, c(120, 136, 60));
            }
            line((0.16, 0.32), (0.84, 0.32), 0.05, c(120, 90, 50));
        }
        ItemKind::HideCloak => {
            poly(&[(0.3, 0.16), (0.7, 0.16), (0.9, 0.84), (0.1, 0.84)], c(150, 104, 66), c(86, 56, 34));
            line((0.3, 0.2), (0.7, 0.2), 0.07, c(110, 76, 46));
            dot(0.5, 0.24, 0.05, c(220, 200, 150));
        }
        ItemKind::HideBoots => {
            for dx in [0.0f32, 0.3] {
                poly(&[(0.14 + dx, 0.2), (0.36 + dx, 0.2), (0.38 + dx, 0.62), (0.52 + dx, 0.8), (0.14 + dx, 0.8)], c(150, 104, 66), c(86, 56, 34));
            }
        }
        ItemKind::Splint => {
            line((0.2, 0.8), (0.8, 0.2), 0.1, c(150, 108, 64));
            line((0.12, 0.62), (0.62, 0.12), 0.1, c(130, 92, 54));
            line((0.3, 0.44), (0.56, 0.7), 0.05, c(220, 214, 170));
        }
        ItemKind::Flint => {
            // a dark, glassy nodule with a pale chalky rind and a spark
            poly(&[(0.16, 0.6), (0.3, 0.3), (0.56, 0.2), (0.8, 0.36), (0.84, 0.62), (0.6, 0.82), (0.3, 0.8)], c(214, 206, 186), c(120, 112, 96));
            poly(&[(0.28, 0.58), (0.38, 0.38), (0.56, 0.32), (0.72, 0.42), (0.72, 0.6), (0.56, 0.72), (0.36, 0.7)], c(58, 62, 74), c(30, 32, 40));
            line((0.42, 0.42), (0.56, 0.38), 0.03, Color32::from_white_alpha(120));
            dot(0.84, 0.18, 0.06, c(255, 214, 90));
        }
        ItemKind::Seed => {
            for (x, y) in [(0.3, 0.42), (0.52, 0.32), (0.68, 0.5), (0.42, 0.64), (0.6, 0.72), (0.28, 0.76)] {
                poly(&[(x - 0.06, y), (x, y - 0.1), (x + 0.06, y), (x, y + 0.1)], c(196, 160, 90), c(120, 92, 46));
            }
        }
        ItemKind::Grain => {
            for dx in [-0.16f32, 0.0, 0.16] {
                line((0.5 + dx * 0.4, 0.92), (0.5 + dx, 0.36), 0.03, c(170, 140, 60));
                for k in 0..4 {
                    let y = 0.14 + k as f32 * 0.07;
                    dot(0.5 + dx - 0.03, y + 0.06, 0.04, c(226, 190, 92));
                    dot(0.5 + dx + 0.03, y + 0.09, 0.04, c(226, 190, 92));
                }
            }
        }
        ItemKind::Flour => {
            poly(&[(0.2, 0.36), (0.8, 0.36), (0.86, 0.84), (0.14, 0.84)], c(170, 130, 86), c(100, 72, 44));
            poly(&[(0.24, 0.4), (0.5, 0.18), (0.76, 0.4)], c(244, 238, 222), c(200, 190, 170));
            line((0.2, 0.46), (0.8, 0.46), 0.03, c(120, 88, 52));
        }
        ItemKind::Bread => {
            poly(&[(0.1, 0.66), (0.2, 0.44), (0.42, 0.34), (0.62, 0.34), (0.82, 0.44), (0.9, 0.66), (0.5, 0.76)], c(196, 132, 60), c(120, 74, 30));
            for x in [0.34, 0.5, 0.66] {
                line((x - 0.05, 0.44), (x + 0.05, 0.56), 0.035, c(240, 200, 130));
            }
        }
        ItemKind::Hoe => {
            line((0.24, 0.92), (0.62, 0.22), 0.08, wood);
            poly(&[(0.56, 0.24), (0.86, 0.3), (0.82, 0.44), (0.58, 0.34)], c(176, 176, 186), c(70, 70, 76));
            line((0.56, 0.24), (0.66, 0.32), 0.04, c(176, 196, 96));
        }
        ItemKind::Quern => {
            poly(&[(0.08, 0.72), (0.16, 0.56), (0.84, 0.56), (0.92, 0.72), (0.84, 0.84), (0.16, 0.84)], c(130, 128, 122), c(70, 68, 64));
            poly(&[(0.3, 0.56), (0.36, 0.42), (0.64, 0.42), (0.7, 0.56)], c(160, 156, 150), c(80, 78, 74));
            dot(0.24, 0.6, 0.03, c(236, 230, 214));
            dot(0.76, 0.62, 0.025, c(236, 230, 214));
        }
        ItemKind::RawLargePot | ItemKind::LargePot => {
            let (body, edge) = if k == ItemKind::RawLargePot { (c(150, 120, 96), c(96, 76, 60)) } else { (c(186, 104, 62), c(116, 60, 32)) };
            poly(&[(0.36, 0.08), (0.64, 0.08), (0.62, 0.16), (0.86, 0.34), (0.9, 0.62), (0.72, 0.92), (0.28, 0.92), (0.1, 0.62), (0.14, 0.34), (0.38, 0.16)], body, edge);
            for y in [0.36, 0.5, 0.64, 0.78] {
                line((0.18, y), (0.82, y), 0.02, Color32::from_black_alpha(50));
            }
        }
        ItemKind::RawPot | ItemKind::Pot | ItemKind::WaterPot => {
            // a round-bellied pot: grey-brown and damp before firing, warm terracotta after
            let (body, edge) = if k == ItemKind::RawPot { (c(150, 120, 96), c(96, 76, 60)) } else { (c(196, 110, 66), c(120, 62, 34)) };
            poly(&[(0.34, 0.2), (0.66, 0.2), (0.62, 0.3), (0.8, 0.46), (0.82, 0.66), (0.66, 0.86), (0.34, 0.86), (0.18, 0.66), (0.2, 0.46), (0.38, 0.3)], body, edge);
            for y in [0.44, 0.58, 0.72] {
                line((0.24, y), (0.76, y), 0.02, Color32::from_black_alpha(if k == ItemKind::RawPot { 70 } else { 40 }));
            }
            if k == ItemKind::WaterPot {
                poly(&[(0.38, 0.2), (0.62, 0.2), (0.6, 0.26), (0.4, 0.26)], c(80, 150, 230), c(50, 100, 180));
                dot(0.5, 0.08, 0.05, c(110, 180, 245));
            }
        }
        ItemKind::Cloth => {
            poly(&[(0.16, 0.24), (0.84, 0.24), (0.84, 0.78), (0.16, 0.78)], c(214, 200, 160), c(130, 116, 84));
            for k in 0..6 {
                let y = 0.3 + k as f32 * 0.08;
                line((0.18, y), (0.82, y), 0.015, c(170, 154, 116));
                let x = 0.24 + k as f32 * 0.11;
                line((x, 0.26), (x, 0.76), 0.015, c(186, 170, 130));
            }
            poly(&[(0.84, 0.66), (0.92, 0.82), (0.74, 0.86), (0.7, 0.78)], c(214, 200, 160), c(130, 116, 84));
        }
        ItemKind::Tunic => {
            poly(&[(0.3, 0.16), (0.7, 0.16), (0.9, 0.34), (0.78, 0.44), (0.7, 0.38), (0.72, 0.88), (0.28, 0.88), (0.3, 0.38), (0.22, 0.44), (0.1, 0.34)], c(190, 160, 110), c(110, 86, 52));
            line((0.42, 0.18), (0.5, 0.3), 0.025, c(110, 86, 52));
            line((0.58, 0.18), (0.5, 0.3), 0.025, c(110, 86, 52));
            line((0.3, 0.6), (0.7, 0.6), 0.03, c(150, 70, 50));
        }
        ItemKind::Bag => {
            poly(&[(0.3, 0.34), (0.7, 0.34), (0.84, 0.62), (0.76, 0.86), (0.24, 0.86), (0.16, 0.62)], c(198, 176, 128), c(116, 96, 60));
            line((0.32, 0.36), (0.68, 0.36), 0.04, c(120, 90, 50));
            line((0.36, 0.34), (0.5, 0.12), 0.025, c(120, 90, 50));
            line((0.64, 0.34), (0.5, 0.12), 0.025, c(120, 90, 50));
            line((0.26, 0.6), (0.74, 0.6), 0.02, c(160, 136, 92));
        }
        ItemKind::Wheel => {
            ring(0.5, 0.5, 0.34, wood, 0.1);
            for (dx, dy) in [(0.0, -0.3), (0.0, 0.3), (-0.3, 0.0), (0.3, 0.0)] {
                line((0.5, 0.5), (0.5 + dx, 0.5 + dy), 0.05, wood_dark);
            }
            dot(0.5, 0.5, 0.09, c(196, 140, 70));
        }
        ItemKind::Cart => {
            poly(&[(0.12, 0.3), (0.78, 0.3), (0.74, 0.56), (0.16, 0.56)], wood, wood_dark);
            line((0.78, 0.4), (0.96, 0.32), 0.04, wood_dark);
            ring(0.34, 0.66, 0.16, wood_dark, 0.06);
            dot(0.34, 0.66, 0.04, c(196, 140, 70));
            ring(0.6, 0.66, 0.16, wood_dark, 0.06);
            dot(0.6, 0.66, 0.04, c(196, 140, 70));
        }
        ItemKind::Plow => {
            line((0.12, 0.3), (0.62, 0.62), 0.07, wood);
            line((0.5, 0.56), (0.42, 0.86), 0.06, wood_dark);
            poly(&[(0.36, 0.84), (0.6, 0.78), (0.56, 0.92)], c(196, 140, 70), c(120, 80, 40));
            line((0.62, 0.62), (0.86, 0.36), 0.05, wood_dark);
        }
        ItemKind::Hammer => {
            line((0.3, 0.92), (0.56, 0.36), 0.08, wood);
            poly(&[(0.36, 0.22), (0.62, 0.14), (0.8, 0.3), (0.74, 0.5), (0.48, 0.52), (0.34, 0.38)], c(140, 140, 146), c(60, 60, 64));
            line((0.46, 0.4), (0.62, 0.44), 0.04, c(176, 196, 96));
        }
        ItemKind::Knife => {
            line((0.2, 0.86), (0.44, 0.6), 0.09, wood);
            poly(&[(0.4, 0.62), (0.84, 0.16), (0.9, 0.22), (0.5, 0.7)], c(176, 176, 186), c(70, 70, 76));
            line((0.5, 0.58), (0.82, 0.22), 0.02, Color32::from_white_alpha(120));
        }
        ItemKind::Shovel => {
            line((0.5, 0.06), (0.5, 0.6), 0.08, wood);
            poly(&[(0.32, 0.58), (0.68, 0.58), (0.72, 0.84), (0.5, 0.96), (0.28, 0.84)], c(140, 140, 146), c(60, 60, 64));
            line((0.4, 0.62), (0.6, 0.62), 0.04, c(176, 196, 96));
        }
        ItemKind::Lentil => {
            for (x, y) in [(0.3, 0.4), (0.5, 0.32), (0.7, 0.42), (0.38, 0.6), (0.6, 0.62), (0.5, 0.78)] {
                dot(x, y, 0.09, c(196, 120, 60));
                dot(x - 0.03, y - 0.03, 0.025, Color32::from_white_alpha(90));
            }
        }
        ItemKind::Flax => {
            for dx in [-0.12f32, 0.0, 0.12] {
                line((0.5 + dx * 0.5, 0.92), (0.5 + dx, 0.26), 0.025, c(120, 150, 70));
                dot(0.5 + dx, 0.22, 0.05, c(110, 140, 220));
            }
        }
        ItemKind::Soup => {
            poly(&[(0.14, 0.46), (0.86, 0.46), (0.74, 0.82), (0.26, 0.82)], c(196, 110, 66), c(120, 62, 34));
            poly(&[(0.18, 0.46), (0.82, 0.46), (0.78, 0.54), (0.22, 0.54)], c(210, 140, 60), c(160, 100, 40));
            line((0.4, 0.38), (0.36, 0.2), 0.025, Color32::from_white_alpha(110));
            line((0.58, 0.38), (0.62, 0.18), 0.025, Color32::from_white_alpha(110));
        }
        ItemKind::Manure => {
            poly(&[(0.16, 0.74), (0.3, 0.5), (0.5, 0.42), (0.72, 0.5), (0.86, 0.74), (0.5, 0.82)], c(92, 66, 40), c(56, 40, 24));
            dot(0.42, 0.56, 0.04, c(120, 90, 56));
        }
        ItemKind::RawCrucible | ItemKind::Crucible | ItemKind::MoltenCopper => {
            let (body, edge) = if k == ItemKind::RawCrucible { (c(150, 120, 96), c(96, 76, 60)) } else { (c(170, 150, 130), c(90, 80, 70)) };
            poly(&[(0.22, 0.3), (0.78, 0.3), (0.7, 0.86), (0.3, 0.86)], body, edge);
            if k == ItemKind::MoltenCopper {
                poly(&[(0.26, 0.34), (0.74, 0.34), (0.72, 0.44), (0.28, 0.44)], c(255, 150, 50), c(230, 90, 30));
            }
        }
        ItemKind::RawMoldAxe | ItemKind::MoldAxe | ItemKind::RawMoldPick | ItemKind::MoldPick | ItemKind::RawMoldKnife | ItemKind::MoldKnife | ItemKind::RawMoldIngot | ItemKind::MoldIngot => {
            let raw = matches!(k, ItemKind::RawMoldAxe | ItemKind::RawMoldPick | ItemKind::RawMoldKnife | ItemKind::RawMoldIngot);
            let (body, edge) = if raw { (c(150, 120, 96), c(96, 76, 60)) } else { (c(186, 104, 62), c(116, 60, 32)) };
            poly(&[(0.1, 0.3), (0.9, 0.3), (0.9, 0.78), (0.1, 0.78)], body, edge);
            let hole = c(60, 40, 28);
            match k {
                ItemKind::RawMoldAxe | ItemKind::MoldAxe => poly(&[(0.3, 0.42), (0.62, 0.38), (0.72, 0.54), (0.62, 0.7), (0.3, 0.64)], hole, hole),
                ItemKind::RawMoldPick | ItemKind::MoldPick => poly(&[(0.2, 0.5), (0.5, 0.4), (0.8, 0.5), (0.5, 0.58)], hole, hole),
                ItemKind::RawMoldKnife | ItemKind::MoldKnife => poly(&[(0.2, 0.56), (0.78, 0.44), (0.8, 0.5), (0.24, 0.62)], hole, hole),
                _ => poly(&[(0.28, 0.44), (0.72, 0.44), (0.66, 0.66), (0.34, 0.66)], hole, hole),
            }
        }
        ItemKind::BadBronze => {
            poly(&[(0.2, 0.62), (0.32, 0.4), (0.8, 0.4), (0.88, 0.62)], c(140, 110, 70), c(80, 60, 40));
            line((0.4, 0.42), (0.52, 0.6), 0.03, c(60, 40, 30));
            line((0.6, 0.42), (0.56, 0.6), 0.03, c(60, 40, 30));
        }
        ItemKind::MoltenBronze => {
            poly(&[(0.22, 0.3), (0.78, 0.3), (0.7, 0.86), (0.3, 0.86)], c(170, 150, 130), c(90, 80, 70));
            poly(&[(0.26, 0.34), (0.74, 0.34), (0.72, 0.44), (0.28, 0.44)], c(255, 200, 80), c(230, 140, 30));
        }
        ItemKind::BronzeAxeHead => poly(&[(0.3, 0.3), (0.72, 0.18), (0.82, 0.6), (0.36, 0.56)], c(200, 160, 80), c(120, 90, 40)),
        ItemKind::BronzePickHead => poly(&[(0.1, 0.5), (0.5, 0.36), (0.9, 0.5), (0.5, 0.48)], c(200, 160, 80), c(120, 90, 40)),
        ItemKind::SpongeIron => {
            poly(&[(0.16, 0.6), (0.3, 0.34), (0.56, 0.26), (0.82, 0.4), (0.86, 0.66), (0.5, 0.8)], c(90, 80, 76), c(50, 44, 40));
            for (x, y) in [(0.36, 0.48), (0.58, 0.42), (0.66, 0.6), (0.44, 0.64)] {
                dot(x, y, 0.035, c(40, 34, 30));
            }
        }
        ItemKind::CopperWire => {
            ring(0.5, 0.5, 0.28, c(204, 120, 70), 0.05);
            ring(0.5, 0.5, 0.18, c(204, 120, 70), 0.05);
        }
        ItemKind::Magnet => {
            line((0.3, 0.2), (0.3, 0.7), 0.12, c(200, 60, 50));
            line((0.7, 0.2), (0.7, 0.7), 0.12, c(170, 180, 196));
            line((0.3, 0.75), (0.7, 0.75), 0.12, c(120, 120, 130));
        }
        ItemKind::LightBulb => {
            dot(0.5, 0.4, 0.24, Color32::from_rgba_unmultiplied(250, 240, 180, 200));
            poly(&[(0.4, 0.62), (0.6, 0.62), (0.58, 0.82), (0.42, 0.82)], c(150, 150, 160), c(90, 90, 100));
        }
        ItemKind::Silicon => rock(c(110, 120, 140)),
        ItemKind::Oil => {
            poly(&[(0.5, 0.12), (0.76, 0.56), (0.66, 0.84), (0.34, 0.84), (0.24, 0.56)], c(30, 28, 26), c(10, 10, 10));
            line((0.42, 0.5), (0.46, 0.66), 0.03, Color32::from_white_alpha(70));
        }
        ItemKind::RawFish | ItemKind::CookedFish => {
            let (body, belly) = if matches!(k, ItemKind::RawFish) { (c(110, 140, 160), c(200, 214, 222)) } else { (c(176, 110, 60), c(222, 170, 110)) };
            poly(&[(0.12, 0.5), (0.3, 0.34), (0.62, 0.32), (0.78, 0.46), (0.62, 0.64), (0.3, 0.66)], body, c(40, 40, 46));
            poly(&[(0.3, 0.52), (0.62, 0.5), (0.62, 0.6), (0.3, 0.62)], belly, belly);
            poly(&[(0.76, 0.48), (0.92, 0.32), (0.92, 0.66)], body, c(40, 40, 46));
            dot(0.24, 0.47, 0.03, c(20, 20, 24));
        }
        ItemKind::FishingRod => {
            line((0.15, 0.9), (0.82, 0.12), 0.05, wood);
            line((0.82, 0.12), (0.84, 0.62), 0.015, c(220, 220, 210));
            line((0.84, 0.62), (0.78, 0.7), 0.025, c(150, 150, 160));
        }
        ItemKind::ProspectorPick => {
            line((0.3, 0.92), (0.6, 0.32), 0.07, wood);
            poly(&[(0.4, 0.3), (0.62, 0.24), (0.86, 0.36), (0.62, 0.32)], c(200, 160, 80), c(120, 90, 40));
        }
        ItemKind::Transistor => {
            poly(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.6), (0.3, 0.6)], c(40, 40, 46), c(20, 20, 24));
            for x in [0.38, 0.5, 0.62] {
                line((x, 0.6), (x, 0.85), 0.03, c(190, 190, 200));
            }
        }
        ItemKind::CircuitBoard => {
            poly(&[(0.12, 0.24), (0.88, 0.24), (0.88, 0.76), (0.12, 0.76)], c(40, 110, 60), c(20, 70, 40));
            line((0.2, 0.4), (0.6, 0.4), 0.025, c(220, 180, 90));
            line((0.6, 0.4), (0.6, 0.62), 0.025, c(220, 180, 90));
            dot(0.72, 0.5, 0.06, c(30, 30, 34));
        }
        ItemKind::Limestone => rock(c(214, 208, 190)),
        ItemKind::Coal => rock(c(30, 30, 34)),
        ItemKind::Coke => rock(c(70, 70, 76)),
        ItemKind::CastIronPart => {
            ring(0.5, 0.5, 0.3, c(70, 70, 76), 0.1);
            dot(0.5, 0.5, 0.08, c(70, 70, 76));
        }
        ItemKind::Sand => {
            poly(&[(0.1, 0.74), (0.3, 0.5), (0.5, 0.42), (0.7, 0.5), (0.9, 0.74)], c(222, 200, 140), c(170, 150, 100));
        }
        ItemKind::Glass => {
            poly(&[(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)], Color32::from_rgba_unmultiplied(160, 210, 230, 120), c(120, 170, 190));
            line((0.3, 0.3), (0.45, 0.6), 0.03, Color32::from_white_alpha(160));
        }
        ItemKind::FireClay => {
            poly(&[(0.1, 0.7), (0.2, 0.46), (0.5, 0.36), (0.8, 0.46), (0.9, 0.7), (0.5, 0.82)], c(224, 210, 186), c(150, 136, 110));
        }
        ItemKind::RawFireBrick | ItemKind::FireBrick => {
            let fill = if k == ItemKind::RawFireBrick { c(200, 190, 170) } else { c(214, 180, 140) };
            poly(&[(0.14, 0.38), (0.86, 0.38), (0.86, 0.68), (0.14, 0.68)], fill, c(120, 100, 80));
        }
        ItemKind::PigIron | ItemKind::HighCarbonSteel | ItemKind::Steel => {
            let fill = match k { ItemKind::PigIron => c(80, 78, 80), ItemKind::HighCarbonSteel => c(110, 112, 120), _ => c(170, 180, 196) };
            poly(&[(0.2, 0.62), (0.32, 0.4), (0.8, 0.4), (0.88, 0.62)], fill, c(50, 50, 56));
            line((0.34, 0.44), (0.76, 0.44), 0.03, Color32::from_white_alpha(90));
        }
        ItemKind::Plank => {
            poly(&[(0.1, 0.42), (0.9, 0.36), (0.92, 0.56), (0.12, 0.62)], c(190, 150, 100), wood_dark);
            line((0.2, 0.5), (0.8, 0.45), 0.015, wood_dark);
        }
        ItemKind::Saw => {
            line((0.12, 0.7), (0.3, 0.6), 0.09, wood);
            poly(&[(0.28, 0.5), (0.9, 0.36), (0.92, 0.48), (0.3, 0.66)], c(200, 160, 80), c(120, 90, 40));
            for k in 0..6 {
                let x = 0.36 + k as f32 * 0.09;
                line((x, 0.64 - k as f32 * 0.025), (x + 0.03, 0.7 - k as f32 * 0.025), 0.02, c(120, 90, 40));
            }
        }
        ItemKind::CopperIngot => {
            poly(&[(0.2, 0.62), (0.32, 0.4), (0.8, 0.4), (0.88, 0.62)], c(204, 120, 70), c(130, 70, 40));
            line((0.34, 0.44), (0.76, 0.44), 0.03, Color32::from_white_alpha(100));
        }
        ItemKind::CopperAxeHead | ItemKind::CopperPickHead | ItemKind::CopperKnifeHead | ItemKind::CopperAxe | ItemKind::CopperPickaxe | ItemKind::CopperKnife => {
            let handle = matches!(k, ItemKind::CopperAxe | ItemKind::CopperPickaxe | ItemKind::CopperKnife);
            if handle {
                line((0.3, 0.92), (0.62, 0.3), 0.08, wood);
            }
            let cu = c(204, 120, 70);
            let cu_edge = c(130, 70, 40);
            match k {
                ItemKind::CopperAxeHead | ItemKind::CopperAxe => poly(&[(0.5, 0.2), (0.82, 0.12), (0.88, 0.4), (0.56, 0.38)], cu, cu_edge),
                ItemKind::CopperPickHead | ItemKind::CopperPickaxe => poly(&[(0.28, 0.3), (0.6, 0.2), (0.92, 0.32), (0.6, 0.3)], cu, cu_edge),
                _ => poly(&[(0.5, 0.4), (0.86, 0.12), (0.9, 0.18), (0.56, 0.46)], cu, cu_edge),
            }
        }
        ItemKind::Torch => {
            line((0.5, 0.9), (0.5, 0.36), 0.1, c(120, 84, 46));
            poly(&[(0.5, 0.04), (0.74, 0.34), (0.5, 0.42), (0.26, 0.34)], c(255, 190, 70), c(220, 110, 40));
        }
        ItemKind::LogPile => {
            for (row, n) in [(0.0f32, 3), (1.0, 2)] {
                for j in 0..n {
                    let x = 0.5 + (j as f32 - (n as f32 - 1.0) / 2.0) * 0.26;
                    let y = 0.7 - row * 0.22;
                    poly(&[(x - 0.13, y - 0.1), (x + 0.13, y - 0.1), (x + 0.13, y + 0.1), (x - 0.13, y + 0.1)], wood, wood_dark);
                    dot(x + 0.13, y, 0.07, c(190, 150, 100));
                }
            }
        }
        ItemKind::Branch => {
            line((0.14, 0.82), (0.84, 0.2), 0.07, wood);
            line((0.5, 0.52), (0.78, 0.7), 0.045, wood);
            line((0.34, 0.66), (0.22, 0.4), 0.04, wood);
        }
        ItemKind::Fiber => {
            for (i, dx) in [-0.14f32, -0.05, 0.05, 0.14].iter().enumerate() {
                let sway = 0.06 * (i as f32 - 1.5);
                p.add(Shape::line(
                    (0..=8).map(|t| { let v = t as f32 / 8.0; pt(0.5 + dx + sway * v * v, 0.9 - v * 0.8) }).collect(),
                    Stroke::new(0.035 * s, c(170, 190, 90)),
                ));
            }
            line((0.34, 0.62), (0.66, 0.62), 0.04, c(120, 90, 50));
        }
        ItemKind::Berries => {
            line((0.5, 0.2), (0.5, 0.4), 0.03, c(60, 110, 50));
            poly(&[(0.5, 0.26), (0.74, 0.18), (0.66, 0.38)], c(80, 150, 60), c(40, 90, 40));
            for (x, y) in [(0.34, 0.6), (0.62, 0.58), (0.48, 0.76)] {
                dot(x, y, 0.15, c(196, 36, 66));
                dot(x - 0.05, y - 0.05, 0.04, Color32::from_white_alpha(140));
            }
        }
        ItemKind::Clay => {
            poly(&[(0.1, 0.7), (0.2, 0.46), (0.5, 0.36), (0.8, 0.46), (0.9, 0.7), (0.5, 0.82)], c(172, 106, 72), c(110, 66, 44));
            line((0.3, 0.56), (0.7, 0.56), 0.025, Color32::from_black_alpha(60));
        }
        ItemKind::Charcoal => {
            poly(&[(0.16, 0.7), (0.28, 0.4), (0.5, 0.5), (0.52, 0.76)], c(46, 46, 52), c(20, 20, 24));
            poly(&[(0.5, 0.76), (0.58, 0.34), (0.82, 0.44), (0.86, 0.74)], c(60, 60, 66), c(20, 20, 24));
        }
        ItemKind::IronOre => {
            rock(c(120, 100, 94));
            dot(0.36, 0.55, 0.06, c(190, 100, 50));
            dot(0.6, 0.5, 0.05, c(190, 100, 50));
        }
        ItemKind::CopperOre => {
            rock(c(118, 118, 120));
            dot(0.36, 0.55, 0.06, c(60, 190, 150));
            dot(0.62, 0.48, 0.05, c(60, 190, 150));
        }
        ItemKind::TinOre => {
            rock(c(126, 128, 134));
            dot(0.36, 0.55, 0.06, c(210, 214, 226));
            dot(0.62, 0.48, 0.05, c(210, 214, 226));
        }
        ItemKind::IronBloom => {
            poly(&[(0.12, 0.66), (0.24, 0.36), (0.6, 0.28), (0.86, 0.5), (0.8, 0.76), (0.34, 0.8)], c(104, 108, 126), c(50, 52, 64));
            line((0.3, 0.48), (0.6, 0.4), 0.03, Color32::from_white_alpha(80));
        }
        ItemKind::Bronze => {
            poly(&[(0.12, 0.72), (0.28, 0.36), (0.72, 0.36), (0.88, 0.72)], c(196, 146, 62), c(110, 76, 30));
            line((0.3, 0.46), (0.7, 0.46), 0.03, Color32::from_white_alpha(100));
        }
        ItemKind::RawMeat => {
            poly(&[(0.14, 0.6), (0.3, 0.3), (0.66, 0.26), (0.84, 0.52), (0.64, 0.78), (0.3, 0.76)], c(206, 92, 98), c(120, 40, 46));
            line((0.6, 0.3), (0.86, 0.14), 0.05, c(236, 230, 214));
            dot(0.88, 0.12, 0.06, c(236, 230, 214));
        }
        ItemKind::CookedMeat => {
            poly(&[(0.14, 0.6), (0.3, 0.3), (0.66, 0.26), (0.84, 0.52), (0.64, 0.78), (0.3, 0.76)], c(160, 96, 50), c(90, 50, 24));
            line((0.6, 0.3), (0.86, 0.14), 0.05, c(236, 230, 214));
            dot(0.88, 0.12, 0.06, c(236, 230, 214));
            line((0.3, 0.5), (0.6, 0.44), 0.025, c(110, 62, 30));
        }
        ItemKind::StoneAxe => tool(&|| { poly(&[(0.5, 0.2), (0.86, 0.2), (0.82, 0.5), (0.54, 0.46)], c(150, 150, 158), c(70, 70, 78)); }, wood),
        ItemKind::IronAxe => tool(&|| { poly(&[(0.5, 0.2), (0.88, 0.16), (0.84, 0.52), (0.54, 0.46)], c(150, 160, 186), c(60, 64, 80)); }, wood),
        ItemKind::BronzeAxe => tool(&|| { poly(&[(0.5, 0.2), (0.88, 0.16), (0.84, 0.52), (0.54, 0.46)], c(204, 150, 66), c(110, 76, 30)); }, wood),
        ItemKind::IronPickaxe => tool(&|| { p.add(Shape::line((0..=8).map(|t| { let v = t as f32 / 8.0; pt(0.34 + v * 0.62, 0.2 + 0.34 * (v - 0.5).powi(2) * 4.0 * 0.5) }).collect(), Stroke::new(0.07 * s, c(150, 160, 186)))); }, wood),
        ItemKind::BronzePickaxe => tool(&|| { p.add(Shape::line((0..=8).map(|t| { let v = t as f32 / 8.0; pt(0.34 + v * 0.62, 0.2 + 0.34 * (v - 0.5).powi(2) * 4.0 * 0.5) }).collect(), Stroke::new(0.07 * s, c(204, 150, 66)))); }, wood),
        ItemKind::Spear => {
            line((0.2, 0.9), (0.72, 0.2), 0.05, wood);
            poly(&[(0.72, 0.1), (0.64, 0.3), (0.84, 0.26)], c(176, 176, 186), c(70, 70, 76));
        }
        ItemKind::Gear => {
            let pts: Vec<(f32, f32)> = (0..16).map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.0;
                let r = if i % 2 == 0 { 0.4 } else { 0.3 };
                (0.5 + a.cos() * r, 0.5 + a.sin() * r)
            }).collect();
            poly(&pts, c(150, 154, 172), c(60, 62, 76));
            dot(0.5, 0.5, 0.11, c(34, 30, 28));
        }
        ItemKind::Herb => {
            line((0.5, 0.9), (0.5, 0.4), 0.035, c(70, 120, 60));
            poly(&[(0.5, 0.56), (0.18, 0.4), (0.34, 0.68)], c(96, 168, 80), c(44, 92, 44));
            poly(&[(0.5, 0.46), (0.82, 0.3), (0.66, 0.58)], c(110, 182, 90), c(44, 92, 44));
            poly(&[(0.5, 0.4), (0.4, 0.12), (0.62, 0.16)], c(120, 190, 100), c(44, 92, 44));
        }
        ItemKind::Bandage => {
            poly(&[(0.18, 0.4), (0.82, 0.4), (0.82, 0.7), (0.18, 0.7)], c(232, 226, 210), c(150, 140, 120));
            for x in [0.34, 0.5, 0.66] {
                line((x, 0.4), (x, 0.7), 0.025, c(190, 180, 160));
            }
            line((0.5, 0.46), (0.5, 0.64), 0.04, c(200, 60, 60));
        }
        ItemKind::Clamp => {
            line((0.2, 0.84), (0.8, 0.2), 0.07, wood);
            line((0.8, 0.84), (0.2, 0.2), 0.07, wood);
            line((0.4, 0.5), (0.6, 0.5), 0.07, c(176, 196, 96));
            line((0.42, 0.44), (0.58, 0.56), 0.025, c(120, 90, 50));
        }
    }
}

/// A slot background with the icon inside; unknown items are a dark "?".
pub fn draw_slot(p: &Painter, rect: Rect, k: ItemKind, known: bool) {
    p.rect_filled(rect, 8.0, c(36, 29, 23));
    p.rect_stroke(rect, 8.0, Stroke::new(1.0, c(96, 78, 54)), alaz_render::egui::StrokeKind::Inside);
    let inner = rect.shrink(rect.width() * 0.14);
    if known {
        draw_item(p, inner, k);
    } else {
        p.text(rect.center(), alaz_render::egui::Align2::CENTER_CENTER, "?", alaz_render::egui::FontId::proportional(rect.width() * 0.6), c(110, 92, 66));
    }
}
