//! The tribe's language grows with its perception: at first things only have grunts, and a thing nobody
//! has handled has no word at all. Later the words become the ones we know.

use alaz_sim::{ItemKind, World};

/// Grunts for each item, one per `ItemKind` in declaration order.
const GRUNTS: [&str; ItemKind::COUNT] = [
    "Gro", "Ba", "Mimi", "Tik", "Ugat", "Kuma", "Zol", "Rak", "Rukh", "Mah", "Mahka", "Sut", "Ugak", "Ugrak", "Tok", "Ota", "Sarga", "Kupa", "Tin", "Bruk", "Brakat", "Bruga", "Dal", "Lif", "Kis", "Yigi", "Deri", "Bag", "Sarun", "Ayak", "Mesh", "Tahta", "Çak", "Tane", "Buk", "Pud", "Nam", "Kaz", "Öğü", "Çamçam", "Kup", "Kupsu", "Dok", "Örtek", "Tor", "Dön", "Gıcır", "Yarık", "Tak", "Kes", "Eşe", "Mer", "Ket", "Çor", "Bok", "Kupkup", "Kupaa", "Pot", "Pothot", "Erik", "Kalb", "Kalba", "Kalk", "Kalka", "Kalbi", "Kalbii", "Kalç", "Kalça", "Kızık", "Kızba", "Kızka", "Kızbı", "Kızbal", "Kızkaz", "Kızbıç", "Bozbruk", "Brukhot", "Brukbal", "Brukkaz", "Tahtak", "Dişdiş", "Süngük", "Kireç", "Akkil", "Tuğham", "Tuğla", "Pikpik", "Sertçe", "Çeliş", "Karkar", "Kokok", "Dökdök", "Kumkum", "Şeffa", "Telte", "Çekçek", "Işık", "Silsil", "Anahtar", "Kartkart", "Karasu", "Dinkaz",
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
