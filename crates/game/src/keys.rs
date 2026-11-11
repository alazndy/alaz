//! Rebindable keyboard controls. Each action has two slots (e.g. WASD and the arrow keys).
//! Saved as `keys.cfg` (`action=Key,Key`) next to the game's config.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    MoveUp,
    MoveDown,
    Sprint,
    MoveLeft,
    MoveRight,
    CamUp,
    CamDown,
    CamLeft,
    CamRight,
    RotLeft,
    RotRight,
    View,
    Primary,
    Follow,
    Stop,
    Bag,
    Craft,
    Build,
    Goals,
    Routes,
    Help,
    Save,
    Load,
    Mute,
    Grid,
    Menu,
}

impl Action {
    pub const ALL: [Action; 26] = [
        Action::MoveUp,
        Action::MoveDown,
        Action::Sprint,
        Action::MoveLeft,
        Action::MoveRight,
        Action::CamUp,
        Action::CamDown,
        Action::CamLeft,
        Action::CamRight,
        Action::RotLeft,
        Action::RotRight,
        Action::View,
        Action::Primary,
        Action::Follow,
        Action::Stop,
        Action::Bag,
        Action::Craft,
        Action::Build,
        Action::Goals,
        Action::Routes,
        Action::Help,
        Action::Save,
        Action::Load,
        Action::Mute,
        Action::Grid,
        Action::Menu,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::MoveUp => "Karakter ileri",
            Action::MoveDown => "Karakter geri",
            Action::Sprint => "Koş (basılı tut)",
            Action::MoveLeft => "Karakter sola",
            Action::MoveRight => "Karakter sağa",
            Action::CamUp => "Kamera yukarı",
            Action::CamDown => "Kamera aşağı",
            Action::CamLeft => "Kamera sola",
            Action::CamRight => "Kamera sağa",
            Action::RotLeft => "Kamerayı sola döndür",
            Action::RotRight => "Kamerayı sağa döndür",
            Action::View => "2D / 2.5D görünüm",
            Action::Primary => "Zıpla / vur (mini oyun)",
            Action::Follow => "Karakteri takip et",
            Action::Stop => "Dur",
            Action::Bag => "Çanta",
            Action::Craft => "Yap",
            Action::Build => "İnşa",
            Action::Goals => "Hedefler",
            Action::Routes => "Rotalar",
            Action::Help => "Yardım",
            Action::Save => "Kaydet",
            Action::Load => "Kayıt yükle",
            Action::Mute => "Sesi aç / kapat",
            Action::Grid => "Izgaraya oturt",
            Action::Menu => "Menü / iptal",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::Sprint => "sprint",
            Action::MoveLeft => "move_left",
            Action::MoveRight => "move_right",
            Action::CamUp => "cam_up",
            Action::CamDown => "cam_down",
            Action::CamLeft => "cam_left",
            Action::CamRight => "cam_right",
            Action::RotLeft => "rot_left",
            Action::RotRight => "rot_right",
            Action::View => "view",
            Action::Primary => "primary",
            Action::Follow => "follow",
            Action::Stop => "stop",
            Action::Bag => "bag",
            Action::Craft => "craft",
            Action::Build => "build",
            Action::Goals => "goals",
            Action::Routes => "routes",
            Action::Help => "help",
            Action::Save => "save",
            Action::Load => "load",
            Action::Mute => "mute",
            Action::Grid => "grid",
            Action::Menu => "menu",
        }
    }

    fn default_keys(self) -> [Option<KeyCode>; 2] {
        use KeyCode as K;
        let (a, b) = match self {
            Action::MoveUp => (K::KeyW, None),
            Action::MoveDown => (K::KeyS, None),
            Action::Sprint => (K::ShiftLeft, None),
            Action::MoveLeft => (K::KeyA, None),
            Action::MoveRight => (K::KeyD, None),
            Action::CamUp => (K::ArrowUp, None),
            Action::CamDown => (K::ArrowDown, None),
            Action::CamLeft => (K::ArrowLeft, None),
            Action::CamRight => (K::ArrowRight, None),
            Action::RotLeft => (K::KeyQ, None),
            Action::RotRight => (K::KeyE, None),
            Action::View => (K::Tab, None),
            Action::Primary => (K::Space, None),
            Action::Follow => (K::KeyF, None),
            Action::Stop => (K::KeyX, None),
            Action::Bag => (K::KeyI, None),
            Action::Craft => (K::KeyC, None),
            Action::Build => (K::KeyB, None),
            Action::Goals => (K::KeyH, None),
            Action::Routes => (K::KeyR, None),
            Action::Help => (K::F1, None),
            Action::Save => (K::F5, None),
            Action::Load => (K::F9, None),
            Action::Mute => (K::KeyM, None),
            Action::Grid => (K::KeyG, None),
            Action::Menu => (K::Escape, None),
        };
        [Some(a), b]
    }
}

/// Keys that can be bound, with the names used in the config file and on screen.
const NAMES: &[(KeyCode, &str)] = {
    use KeyCode::*;
    &[
        (KeyA, "A"), (KeyB, "B"), (KeyC, "C"), (KeyD, "D"), (KeyE, "E"), (KeyF, "F"), (KeyG, "G"), (KeyH, "H"),
        (KeyI, "I"), (KeyJ, "J"), (KeyK, "K"), (KeyL, "L"), (KeyM, "M"), (KeyN, "N"), (KeyO, "O"), (KeyP, "P"),
        (KeyQ, "Q"), (KeyR, "R"), (KeyS, "S"), (KeyT, "T"), (KeyU, "U"), (KeyV, "V"), (KeyW, "W"), (KeyX, "X"),
        (KeyY, "Y"), (KeyZ, "Z"),
        (Digit0, "0"), (Digit1, "1"), (Digit2, "2"), (Digit3, "3"), (Digit4, "4"),
        (Digit5, "5"), (Digit6, "6"), (Digit7, "7"), (Digit8, "8"), (Digit9, "9"),
        (ArrowUp, "Yukarı ok"), (ArrowDown, "Aşağı ok"), (ArrowLeft, "Sol ok"), (ArrowRight, "Sağ ok"),
        (Space, "Boşluk"), (Tab, "Tab"), (Escape, "Esc"), (Enter, "Enter"), (Backspace, "Geri sil"),
        (Home, "Home"), (End, "End"), (PageUp, "PgUp"), (PageDown, "PgDn"),
        (Comma, ","), (Period, "."), (Minus, "Eksi"), (Equal, "="), (Backquote, "`"),
        (ShiftLeft, "Sol Shift"), (ControlLeft, "Sol Ctrl"), (AltLeft, "Sol Alt"),
        (F1, "F1"), (F2, "F2"), (F3, "F3"), (F4, "F4"), (F5, "F5"), (F6, "F6"),
        (F7, "F7"), (F8, "F8"), (F9, "F9"), (F10, "F10"), (F11, "F11"), (F12, "F12"),
    ]
};

pub fn key_name(k: KeyCode) -> &'static str {
    NAMES.iter().find(|(c, _)| *c == k).map_or("?", |(_, n)| n)
}

pub fn bindable(k: KeyCode) -> bool {
    NAMES.iter().any(|(c, _)| *c == k)
}

fn key_from_name(n: &str) -> Option<KeyCode> {
    NAMES.iter().find(|(_, name)| *name == n).map(|(c, _)| *c)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keybinds {
    map: HashMap<Action, [Option<KeyCode>; 2]>,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self { map: Action::ALL.iter().map(|&a| (a, a.default_keys())).collect() }
    }
}

impl Keybinds {
    pub fn slots(&self, a: Action) -> [Option<KeyCode>; 2] {
        self.map.get(&a).copied().unwrap_or([None, None])
    }

    /// Short text for the keys of an action, e.g. `W / Yukarı ok`.
    pub fn label(&self, a: Action) -> String {
        let s = self.slots(a);
        let parts: Vec<&str> = s.iter().flatten().map(|k| key_name(*k)).collect();
        if parts.is_empty() { "-".into() } else { parts.join(" / ") }
    }

    /// First key only, for crowded buttons.
    pub fn short(&self, a: Action) -> String {
        self.slots(a).iter().flatten().next().map_or("-".into(), |k| key_name(*k).to_string())
    }

    /// Every action this key triggers.
    pub fn actions_for(&self, code: KeyCode) -> Vec<Action> {
        Action::ALL.iter().copied().filter(|a| self.slots(*a).contains(&Some(code))).collect()
    }

    pub fn held(&self, down: &HashSet<KeyCode>, a: Action) -> bool {
        self.slots(a).iter().flatten().any(|k| down.contains(k))
    }

    /// Bind `code` to slot `slot` of `a`. The key is taken away from any other action first,
    /// except the camera and the menu pair that may share arrows by choice of the player.
    pub fn set(&mut self, a: Action, slot: usize, code: Option<KeyCode>) {
        if let Some(code) = code {
            for other in Action::ALL {
                if other == a {
                    continue;
                }
                let e = self.map.entry(other).or_insert([None, None]);
                for s in e.iter_mut() {
                    if *s == Some(code) {
                        *s = None;
                    }
                }
            }
        }
        self.map.entry(a).or_insert([None, None])[slot.min(1)] = code;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for a in Action::ALL {
            let names: Vec<&str> = self.slots(a).iter().map(|k| k.map_or("yok", key_name)).collect();
            out.push_str(&format!("{}={}\n", a.id(), names.join(",")));
        }
        out
    }

    pub fn from_text(text: &str) -> Self {
        let mut k = Self::default();
        for line in text.lines() {
            let Some((id, rest)) = line.split_once('=') else { continue };
            let Some(a) = Action::ALL.iter().copied().find(|a| a.id() == id.trim()) else { continue };
            let mut slots = [None, None];
            for (i, n) in rest.split(',').take(2).enumerate() {
                slots[i] = key_from_name(n.trim());
            }
            k.map.insert(a, slots);
        }
        k
    }

    fn path() -> PathBuf {
        if let Some(d) = std::env::var_os("ALAZ_SAVE_DIR") {
            return PathBuf::from(d).join("keys.cfg");
        }
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| {
            std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".config")
        });
        base.join("alaz").join("keys.cfg")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).map(|t| Self::from_text(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, self.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip_keeps_custom_binds() {
        let mut k = Keybinds::default();
        k.set(Action::Bag, 0, Some(KeyCode::KeyT));
        k.set(Action::CamUp, 1, None);
        let back = Keybinds::from_text(&k.to_text());
        assert_eq!(back, k);
        assert_eq!(back.slots(Action::Bag)[0], Some(KeyCode::KeyT));
    }

    #[test]
    fn taking_a_key_frees_it_elsewhere() {
        let mut k = Keybinds::default();
        k.set(Action::Bag, 0, Some(KeyCode::KeyW));
        assert_eq!(k.slots(Action::MoveUp)[0], None, "W moved from walking to bag");
        assert_eq!(k.actions_for(KeyCode::KeyW), vec![Action::Bag]);
    }

    #[test]
    fn defaults_have_no_duplicates_and_are_nameable() {
        let k = Keybinds::default();
        let mut seen = HashSet::new();
        for a in Action::ALL {
            for s in k.slots(a).iter().flatten() {
                assert!(bindable(*s), "{a:?}");
                assert!(seen.insert(*s), "duplicate default {s:?}");
            }
        }
    }

    #[test]
    fn garbage_config_falls_back_to_defaults() {
        let k = Keybinds::from_text("nonsense\nbag=Zzz,??\n\n=\n");
        assert_eq!(k.slots(Action::Craft), Action::Craft.default_keys());
        assert_eq!(k.slots(Action::Bag), [None, None]);
    }
}
