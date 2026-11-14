//! The tribe's language grows with its perception: at first things only have grunts, and a thing nobody
//! has handled has no word at all. Later the words become the ones we know.

use alaz_sim::{ItemKind, World};

/// Grunts for each item, one per `ItemKind` in declaration order.
const GRUNTS: [&str; ItemKind::COUNT] = [
    "Gro", "Ba", "Mimi", "Tik", "Ugat", "Kuma", "Zol", "Rak", "Rukh", "Mah", "Mahka", "Sut", "Ugak", "Ugrak", "Tok", "Ota", "Sarga", "Kupa", "Tin", "Bruk", "Brakat", "Bruga", "Dal", "Lif", "Kis", "Yigi", "Deri", "Bag", "Sarun", "Ayak", "Mesh", "Tahta", "Çak", "Tane", "Buk", "Pud", "Nam", "Kaz", "Öğü", "Çamçam", "Kup", "Kupsu", "Dok", "Örtek", "Tor", "Dön", "Gıcır", "Yarık", "Tak", "Kes", "Eşe", "Mer", "Ket", "Çor", "Bok", "Kupkup", "Kupaa", "Pot", "Pothot", "Erik", "Kalb", "Kalba", "Kalk", "Kalka", "Kalbi", "Kalbii", "Kalç", "Kalça", "Kızık", "Kızba", "Kızka", "Kızbı", "Kızbal", "Kızkaz", "Kızbıç", "Bozbruk", "Brukhot", "Brukbal", "Brukkaz", "Tahtak", "Dişdiş", "Süngük", "Kireç", "Akkil", "Tuğham", "Tuğla", "Pikpik", "Sertçe", "Çeliş", "Karkar", "Kokok", "Dökdök", "Kumkum", "Şeffa", "Telte", "Çekçek", "Işık", "Silsil", "Anahtar", "Kartkart", "Karasu", "Dinkaz", "Bal", "Balhot", "Çubip", "Bashi", "Kalkan", "Tolga", "Önlu",
];

/// The word the tribe has for an item right now.
pub fn item_word(w: &World, k: ItemKind) -> String {
    if !w.has_seen(k) {
        return "???".to_string();
    }
    if w.perception_level() <= 1 {
        let g = GRUNTS[k as usize];
        return format!("{g}!");
    }
    crate::hud::item_name(k).to_string()
}

/// Tab names: blunt words at first, the usual ones once the tribe has more words.
pub fn tab_word(w: &World, modern: &'static str) -> &'static str {
    if w.perception_level() > 1 {
        return modern;
    }
    match modern {
        "Çanta" => "Torba",
        "Yap" => "Yap",
        "İnşa" => "Kur",
        "Hedefler" => "Dilek",
        "Rotalar" => "Yol",
        other => other,
    }
}

/// Early on, advice comes in short plain sentences: only the first sentence of a hint is understood.
pub fn hint_text(w: &World, text: &str) -> String {
    if w.perception_level() > 1 {
        return text.to_string();
    }
    match text.find(". ") {
        Some(i) => text[..=i].to_string(),
        None => text.to_string(),
    }
}

/// Day counter word: a newcomer only knows 'sun'.
pub fn day_word(w: &World) -> &'static str {
    if w.perception_level() > 1 { "Gün" } else { "Güneş" }
}

/// Names of the body's needs: blunt feelings first, the usual words later.
pub fn stat_word(w: &World, modern: &'static str) -> &'static str {
    if w.perception_level() > 1 {
        return modern;
    }
    match modern {
        "Sağlık" => "Can",
        "Tokluk" => "Karın",
        "Su" => "Susuz",
        "Sıcaklık" => "Ateş",
        "Kanıyorsun" => "Kan!",
        "Hastasın" => "Kötü!",
        other => other,
    }
}

/// How clearly the tribe sees the world, named per stage.
pub fn perception_word(level: u8) -> &'static str {
    ["Hayvan gözü", "Şaşkın", "Meraklı", "Bilen", "Usta", "Aydın"][level.min(5) as usize]
}

/// The goal banner's lead word.
pub fn goal_word(w: &World) -> &'static str {
    if w.perception_level() > 1 { "Hedef" } else { "Dilek" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_shorten_for_a_newcomer_and_stay_whole_later() {
        let mut w = World::new(1);
        w.forget_everything();
        assert_eq!(hint_text(&w, "Bir. İki. Üç."), "Bir.");
        assert_eq!(tab_word(&w, "İnşa"), "Kur");
        w.learn_everything();
        assert_eq!(hint_text(&w, "Bir. İki."), "Bir. İki.");
        assert_eq!(tab_word(&w, "İnşa"), "İnşa");
    }

    #[test]
    fn grunts_are_distinct_and_cover_every_item() {
        for (i, a) in GRUNTS.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &GRUNTS[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(GRUNTS.len(), ItemKind::ALL.len());
    }
}

/// What a thing looks like to eyes that cannot yet tell it apart: below the level given, it passes for the
/// first thing named. Stones are stones until the eye learns flint and then the ores; grass is grass until seed
/// and flax stand out; clay is clay until fire clay does.
const LOOKALIKES: [(ItemKind, ItemKind, u8); 9] = [
    (ItemKind::Flint, ItemKind::Stone, 1),
    (ItemKind::CopperOre, ItemKind::Stone, 2),
    (ItemKind::TinOre, ItemKind::Stone, 2),
    (ItemKind::IronOre, ItemKind::Stone, 2),
    (ItemKind::Limestone, ItemKind::Stone, 2),
    (ItemKind::Coal, ItemKind::Stone, 2),
    (ItemKind::Seed, ItemKind::Fiber, 2),
    (ItemKind::Flax, ItemKind::Fiber, 2),
    (ItemKind::FireClay, ItemKind::Clay, 3),
];

/// The thing `k` passes for at the world's perception now (itself once the eye can tell).
pub fn looks_like(w: &World, k: ItemKind) -> ItemKind {
    let lvl = w.perception_level();
    LOOKALIKES.iter().find(|(real, _, need)| *real == k && lvl < *need).map_or(k, |(_, seen, _)| *seen)
}

/// Things that still pass for something else now, and the level at which they will stand out.
pub fn blurred_at(level: u8) -> Vec<(ItemKind, ItemKind)> {
    LOOKALIKES.iter().filter(|(_, _, need)| *need == level).map(|(r, s, _)| (*r, *s)).collect()
}

/// Everything in the bag that passes for `seen` (its real kinds and how many of each).
pub fn hidden_in(w: &World, inv: &alaz_sim::world::Inventory, seen: ItemKind) -> Vec<(ItemKind, u16)> {
    ItemKind::ALL.iter().copied().filter(|k| looks_like(w, *k) == seen && inv.get(*k) > 0).map(|k| (k, inv.get(k))).collect()
}

#[cfg(test)]
mod lookalike_tests {
    use super::*;

    #[test]
    fn lookalikes_come_apart_as_the_eye_sharpens() {
        let mut w = World::new(5);
        assert_eq!(w.perception_level(), 0);
        assert_eq!(looks_like(&w, ItemKind::Flint), ItemKind::Stone, "a fresh eye sees flint as stone");
        assert_eq!(looks_like(&w, ItemKind::CopperOre), ItemKind::Stone);
        assert_eq!(looks_like(&w, ItemKind::Wood), ItemKind::Wood);
        // handle enough things and the eye learns
        for &k in ItemKind::ALL.iter() {
            w.mark_seen(k);
            if w.perception_level() >= 2 {
                break;
            }
        }
        assert!(w.perception_level() >= 2);
        assert_eq!(looks_like(&w, ItemKind::Flint), ItemKind::Flint);
        assert_eq!(looks_like(&w, ItemKind::CopperOre), ItemKind::CopperOre);
        assert_eq!(looks_like(&w, ItemKind::FireClay), ItemKind::Clay, "fire clay stands out later still");
    }
}
