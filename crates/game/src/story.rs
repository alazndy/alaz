//! The story and the first steps: a short telling when a world begins, a page when the tribe enters a new age,
//! and a checklist of first things to do that ticks itself off from what has actually happened in the world.

use crate::keys::{Action, Keybinds};
use alaz_render::egui::{self, Align2, Color32, RichText, Stroke};
use alaz_sim::{BuildingKind, ItemKind, Milestone, World};

/// What the story shows right now, and what it has already shown.
#[derive(Default)]
pub struct Story {
    /// The opening, page by page, while it is being read.
    intro: Option<usize>,
    /// Ages whose page has been shown (milestone bits), so each is told once.
    told: u64,
    /// A page being shown (an age reached).
    page: Option<(&'static str, &'static str)>,
    /// The first-steps list, folded away by the player.
    folded: bool,
}

const INTRO: [(&str, &str); 4] = [
    (
        "Çok eskiden",
        "Dünya yeniydi, ama sen ondan da yeniydin. Ne ateşi biliyordun ne taşı yontmayı. Ağaçlardan indin, ellerine baktın: bir şey tutabiliyorlardı.",
    ),
    (
        "Kabile",
        "Bir avuç akraban var. Aç, susuz, üşüyen bedenler. Kurtlar ormanın kıyısında dolaşıyor. Hayatta kalırsanız, öğrendiğiniz her şey çocuklarınıza geçecek.",
    ),
    (
        "Yol",
        "Taştan bakıra, bakırdan demire, demirden buhara, buhardan elektriğe... Bir gün belki bir makine, kendi kurallarını yazacak kadar akıllı olacak. Ama önce: bir taş, bir dal, bir kıvılcım.",
    ),
    (
        "İlk adım",
        "Sağ üstteki liste ilk adımları gösteriyor; yaptıkça kendiliğinden işaretlenir. Ortadaki sarı şerit sıradaki hedefi söyler. Takılırsan Yardım tuşuna bas.",
    ),
];

/// The page told when an age is reached.
fn age_page(m: Milestone) -> Option<(&'static str, &'static str)> {
    Some(match m {
        Milestone::NeolithicAge => ("Cilalı Taş", "Toprağı ekmeyi, kili pişirmeyi, hayvanı ağıla kapatmayı öğrendiniz. Artık göçmek zorunda değilsiniz: bir köy doğuyor."),
        Milestone::CopperAge => ("Bakır", "Yeşil taş ateşte eridi ve parladı. Taştan sert değil, ama dökülebilir, dövülebilir. Dünyada ilk kez bir şeyi istediğiniz biçime soktunuz."),
        Milestone::BronzeAge => ("Tunç", "Bakıra biraz kalay katınca sertleşti. Uzak yerlerden kalay getirmek gerekiyor: yollar, takas, komşular. Dünya büyüyor."),
        Milestone::IronAge => ("Demir", "Kızıl toprak körükle beslenen ateşte süngerleşti, çekiç onu demire çevirdi. Demir her yerde var: artık az kişinin elinde değil."),
        Milestone::SteelAge => ("Çelik ve Çark", "Su ve rüzgâr sizin için çalışıyor. Çelik kırılmıyor, çarklar dönüyor, bantlar taşıyor. Elleriniz serbest kaldı; akıllarınız da."),
        Milestone::SteamAge => ("Buhar", "Kömür ve su: kazan öttü, piston itti. Artık bir dereye ya da rüzgâra bağlı değilsiniz. Madenler derinleşiyor, cam pencereler ışık alıyor."),
        Milestone::ElectricAge => ("Elektrik", "Dönen bakır tel görünmez bir güç verdi: tellerden geçip uzakta lamba yaktı. Gece artık o kadar karanlık değil."),
        Milestone::ElectronicsAge => ("Elektronik", "Kumdan bir anahtar yaptınız: hiç hareket etmeden açılıp kapanan. Binlercesi bir kartta birleşince düşünmeye benzer bir şey yapıyor."),
        Milestone::ComputerAge => ("Döngü", "Makine kendi kuralını yazdı. Bu dünya size öğreteceğini öğretti. Bilgisayardan yeni bir dünya başlatabilirsiniz: bu dünyanın izleri orada harabe olarak bekleyecek."),
        _ => return None,
    })
}

/// One first step: what to do, how, and whether it is done.
struct Step {
    title: &'static str,
    how: String,
    done: bool,
}

fn steps(w: &World, me: &alaz_sim::world::Agent, binds: &Keybinds) -> Vec<Step> {
    let k = |a: Action| binds.short(a);
    let (sx, sy) = w.spawn_mm;
    let moved = (me.x - sx).pow(2) + (me.y - sy).pow(2) > 6_000 * 6_000;
    let built = |kind: BuildingKind| w.buildings.iter().any(|b| b.kind == kind);
    vec![
        Step { title: "Yürü, etrafa bak", how: format!("{}{}{}{} ile yürü, fareyle bak. Koşmak: {}.", k(Action::MoveUp), k(Action::MoveLeft), k(Action::MoveDown), k(Action::MoveRight), k(Action::Sprint)), done: moved },
        // the same order as the goal line at the top and as the recipes open up
        Step { title: "İki taş topla", how: "Yerdeki taşa sağ tıkla. İmleç üstündeyken adı yazar.".into(), done: w.has_seen(ItemKind::Stone) },
        Step { title: "Keskin taş yap", how: format!("Yap sekmesini aç ({}), iki taşı ellerine al ve Dene: yontma oyunu başlar.", k(Action::Craft)), done: w.has_seen(ItemKind::SharpStone) },
        Step { title: "Dal ve lif topla", how: "Kuru dallara ve sarı lif otlarına sağ tıkla.".into(), done: w.has_seen(ItemKind::Branch) && w.has_seen(ItemKind::Fiber) },
        Step { title: "Taş balta yap", how: "Keskin taş, bir dal ve iki lifi ellerinde dene.".into(), done: w.has_seen(ItemKind::StoneAxe) },
        Step { title: "Odun kes", how: "Baltayla bir ağaca sağ tıkla; devrilince gövdesine sağ tıkla. Üç odun lazım.".into(), done: w.has_seen(ItemKind::Wood) },
        Step { title: "Çakmaktaşı bul", how: "İki taş, dal ve liften çekiç yap; çekiçle bir kaya damarına sağ tıkla.".into(), done: w.has_seen(ItemKind::Flint) },
        Step { title: "Ateş yak", how: format!("İnşa sekmesinden ({}) kamp ateşi kur. Ateş ısıtır ve kurtları uzak tutar.", k(Action::Build)), done: built(BuildingKind::Campfire) },
        Step { title: "Ye ve iç", how: "Yemiş topla; alttaki çubuktan Ye ve Su iç düğmelerini kullan.".into(), done: w.has_seen(ItemKind::Berries) },
        Step { title: "Barınak kur", how: "Gece soğuk olur: ateşin yanına bir barınak kur.".into(), done: built(BuildingKind::Shelter) },
    ]
}

impl Story {
    /// A new world: start the telling from the top. Ages already reached (a loaded world) are not told again.
    pub fn begin(&mut self, w: &World) {
        self.intro = Some(0);
        self.page = None;
        self.folded = false;
        self.told = w.milestone_bits();
    }

    /// A world was loaded: no opening, and only ages reached from now on are told.
    pub fn resume(&mut self, w: &World) {
        self.intro = None;
        self.page = None;
        self.told = w.milestone_bits();
    }

    /// `panel_open`: a building or worker panel holds the top right, where the first-steps list would go.
    pub fn draw(&mut self, ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent, binds: &Keybinds, panel_open: bool) {
        // an age reached since last time: tell it once
        for m in Milestone::ALL {
            if w.milestone_done(m) && self.told & m.bit() == 0 {
                self.told |= m.bit();
                if let Some(p) = age_page(m) {
                    self.page = Some(p);
                }
            }
        }
        if let Some(i) = self.intro {
            let (title, body) = INTRO[i];
            let last = i + 1 == INTRO.len();
            if let Some(next) = card(ctx, "story_intro", title, body, if last { "Başla" } else { "İleri" }, Some("Geç")) {
                self.intro = if next && !last { Some(i + 1) } else { None };
            }
            return;
        }
        if let Some((title, body)) = self.page {
            if card(ctx, "story_age", &format!("Yeni bir çağ: {title}"), body, "Devam", None).is_some() {
                self.page = None;
            }
        }
        if !panel_open {
            self.checklist(ctx, w, me, binds);
        }
    }

    fn checklist(&mut self, ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent, binds: &Keybinds) {
        let list = steps(w, me, binds);
        let done = list.iter().filter(|s| s.done).count();
        if done == list.len() || w.craft_view(me.id).is_some() {
            return;
        }
        egui::Area::new(egui::Id::new("first_steps")).anchor(Align2::RIGHT_TOP, [-12.0, 120.0]).show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(30, 24, 18, 225))
                .stroke(Stroke::new(1.2, Color32::from_rgb(150, 120, 70)))
                .inner_margin(10)
                .corner_radius(0)
                .show(ui, |ui| {
                    ui.set_width(250.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("İlk adımlar  {done}/{}", list.len())).strong().color(Color32::from_rgb(244, 218, 130)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(if self.folded { "Göster" } else { "Gizle" }).clicked() {
                                self.folded = !self.folded;
                            }
                        });
                    });
                    if self.folded {
                        return;
                    }
                    let current = list.iter().position(|s| !s.done);
                    for (i, s) in list.iter().enumerate() {
                        let (mark, col) = if s.done { ("[x]", Color32::from_rgb(130, 200, 120)) } else if Some(i) == current { (">", Color32::from_rgb(250, 225, 150)) } else { ("-", Color32::from_gray(150)) };
                        ui.label(RichText::new(format!("{mark} {}", s.title)).color(col));
                        if Some(i) == current {
                            ui.label(RichText::new(&s.how).small().color(Color32::from_gray(215)));
                        }
                    }
                });
        });
    }
}

/// A page in the middle of the screen. `Some(true)` when the main button was pressed, `Some(false)` for the
/// secondary one, `None` while it is still being read.
fn card(ctx: &egui::Context, id: &str, title: &str, body: &str, go: &str, skip: Option<&str>) -> Option<bool> {
    let mut out = None;
    egui::Area::new(egui::Id::new(id)).anchor(Align2::CENTER_CENTER, [0.0, -40.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(28, 21, 15, 245))
            .stroke(Stroke::new(2.0, Color32::from_rgb(190, 150, 70)))
            .inner_margin(22)
            .corner_radius(0)
            .shadow(egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(150) })
            .show(ui, |ui| {
                ui.set_width(440.0_f32.min(ctx.content_rect().width() - 60.0));
                ui.label(RichText::new(title).size(33.0).strong().color(Color32::from_rgb(244, 210, 120)));
                ui.add_space(8.0);
                ui.label(RichText::new(body).size(22.0).color(Color32::from_rgb(236, 226, 206)));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(go).size(22.0)).clicked() {
                        out = Some(true);
                    }
                    if let Some(s) = skip {
                        if ui.button(RichText::new(s).size(22.0)).clicked() {
                            out = Some(false);
                        }
                    }
                });
            });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    struct S {
        w: World,
        story: Story,
        binds: Keybinds,
    }

    fn harness(w: World, begin: bool) -> egui_kittest::Harness<'static, S> {
        let mut story = Story::default();
        if begin { story.begin(&w) } else { story.resume(&w) }
        let state = S { w, story, binds: Keybinds::default() };
        egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 760.0)).build_ui_state(
            |ui, s: &mut S| {
                let me = s.w.agents[0].clone();
                s.story.draw(ui.ctx(), &s.w, &me, &s.binds, false);
            },
            state,
        )
    }

    fn world() -> World {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w
    }

    #[test]
    fn a_new_world_tells_its_story_then_shows_the_first_steps() {
        let mut h = harness(world(), true);
        h.run();
        assert!(h.query_by_label_contains("Çok eskiden").is_some());
        for _ in 0..3 {
            h.get_by_label("İleri").click();
            h.run();
        }
        h.get_by_label("Başla").click();
        h.run();
        assert!(h.query_by_label_contains("Çok eskiden").is_none());
        assert!(h.query_by_label_contains("İlk adımlar 0/10").is_some() || h.query_by_label_contains("İlk adımlar  0/10").is_some());
        assert!(h.query_by_label_contains("Yürü, etrafa bak").is_some());
    }

    #[test]
    fn a_loaded_world_skips_the_opening_and_tells_a_new_age_once() {
        let mut h = harness(world(), false);
        h.run();
        assert!(h.query_by_label_contains("Çok eskiden").is_none(), "no opening on load");
        h.state_mut().w.debug_reach(Milestone::CopperAge);
        h.run();
        assert!(h.query_by_label_contains("Yeni bir çağ: Bakır").is_some());
        h.get_by_label("Devam").click();
        h.run();
        h.run();
        assert!(h.query_by_label_contains("Yeni bir çağ").is_none(), "told once");
    }
}
