use alaz_render::egui::{self, Align2, Color32, RichText, Stroke};
use alaz_sim::rules::{rules, ItemCount};
use alaz_sim::time::{minute_of_day, TICKS_PER_SECOND};
use alaz_sim::world::{FIRE_FUEL_MAX, MAX_STAT};
use alaz_sim::{
    AgentId, AgentKind, BuildingKind, CommandKind, Discovery, Event, ItemKind, Milestone, RecipeId, Reject, RouteStep,
    Terrain,
    World,
};

use crate::keys::{Action, Keybinds};

pub const HINT_SECONDS: f32 = 12.0;
const MAX_TILE_SEARCH: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    Building(usize),
    Agent(usize),
}

/// Panels that open above the action bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Bag,
    Craft,
    Build,
    Goals,
    Routes,
    Help,
}

/// Read-only UI state handed to the HUD each frame.
pub struct UiCtx<'a> {
    pub hints: &'a [(String, f32)],
    pub build_mode: Option<BuildingKind>,
    pub grid_snap: bool,
    pub selection: Option<Selection>,
    pub recording: bool,
    pub recorded: &'a [RouteStep],
    pub me: AgentId,
    /// Co-op status line (empty when playing solo).
    pub net_status: &'a str,
    /// Result of the last strike and how long it stays highlighted.
    pub last_strike: Option<(bool, f32)>,
    pub tab: Option<Tab>,
    pub binds: &'a Keybinds,
    /// Where the player's head is on screen (NDC), so a craft can happen in the world instead of a window.
    pub anchor: Option<[f32; 2]>,
}

/// What the HUD asks the game loop to do this frame.
pub enum UiAction {
    Cmd(CommandKind),
    ToggleTab(Tab),
    BeginBuild(BuildingKind),
    ToggleGrid,
    Deselect,
    ToggleRecording,
    ClearRecorded,
    RecordWait,
    AssignRecorded(AgentId),
    /// Start a new world from this one (the loop).
    NewCycle,
    /// Pick a building up to set it down elsewhere.
    MoveBuilding(usize),
}

pub fn item_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Stone => "Taş",
        ItemKind::Wood => "Odun",
        ItemKind::Berries => "Yemiş",
        ItemKind::SharpStone => "Keskin taş",
        ItemKind::StoneAxe => "Taş balta",
        ItemKind::Clay => "Kil",
        ItemKind::Charcoal => "Kömür",
        ItemKind::IronOre => "Demir cevheri",
        ItemKind::IronBloom => "Dövme demir",
        ItemKind::RawMeat => "Çiğ et",
        ItemKind::CookedMeat => "Pişmiş et",
        ItemKind::Spear => "Mızrak",
        ItemKind::IronAxe => "Demir balta",
        ItemKind::IronPickaxe => "Demir kazma",
        ItemKind::Gear => "Dişli",
        ItemKind::Herb => "Şifalı ot",
        ItemKind::Bandage => "Sargı",
        ItemKind::CopperOre => "Bakır cevheri",
        ItemKind::TinOre => "Kalay cevheri",
        ItemKind::Bronze => "Bronz",
        ItemKind::BronzeAxe => "Bronz balta",
        ItemKind::BronzePickaxe => "Bronz kazma",
        ItemKind::Branch => "Dal",
        ItemKind::Fiber => "Lif",
        ItemKind::Clamp => "Kıskaç",
        ItemKind::LogPile => "Odun yığını",
        ItemKind::Hide => "Post",
        ItemKind::FiberWrap => "Lif örtü",
        ItemKind::HideCloak => "Post pelerin",
        ItemKind::HideBoots => "Post çizme",
        ItemKind::Torch => "Meşale",
        ItemKind::Splint => "Kırık tahtası",
        ItemKind::Flint => "Çakmaktaşı",
        ItemKind::Seed => "Tohum",
        ItemKind::Grain => "Buğday",
        ItemKind::Flour => "Un",
        ItemKind::Bread => "Ekmek",
        ItemKind::Hoe => "Çapa",
        ItemKind::Quern => "Öğütme taşı",
        ItemKind::RawPot => "Ham çömlek",
        ItemKind::Pot => "Çömlek",
        ItemKind::WaterPot => "Su dolu çömlek",
        ItemKind::Cloth => "Kumaş",
        ItemKind::Tunic => "Dokuma giysi",
        ItemKind::Bag => "Dokuma torba",
        ItemKind::Wheel => "Tekerlek",
        ItemKind::Cart => "Araba",
        ItemKind::Plow => "Saban",
        ItemKind::Hammer => "Taş çekiç",
        ItemKind::Knife => "Taş bıçak",
        ItemKind::Shovel => "Taş kürek",
        ItemKind::Lentil => "Mercimek",
        ItemKind::Flax => "Keten",
        ItemKind::Soup => "Mercimek çorbası",
        ItemKind::Manure => "Gübre",
        ItemKind::RawLargePot => "Ham büyük çömlek",
        ItemKind::LargePot => "Büyük çömlek",
        ItemKind::RawCrucible => "Ham pota",
        ItemKind::Crucible => "Pota",
        ItemKind::MoltenCopper => "Eriyik dolu pota",
        ItemKind::RawMoldAxe => "Ham balta kalıbı",
        ItemKind::MoldAxe => "Balta kalıbı",
        ItemKind::RawMoldPick => "Ham kazma kalıbı",
        ItemKind::MoldPick => "Kazma kalıbı",
        ItemKind::RawMoldKnife => "Ham bıçak kalıbı",
        ItemKind::MoldKnife => "Bıçak kalıbı",
        ItemKind::RawMoldIngot => "Ham külçe kalıbı",
        ItemKind::MoldIngot => "Külçe kalıbı",
        ItemKind::CopperIngot => "Bakır külçe",
        ItemKind::CopperAxeHead => "Bakır balta kafası",
        ItemKind::CopperPickHead => "Bakır kazma kafası",
        ItemKind::CopperKnifeHead => "Bakır bıçak ağzı",
        ItemKind::CopperAxe => "Bakır balta",
        ItemKind::CopperPickaxe => "Bakır kazma",
        ItemKind::CopperKnife => "Bakır bıçak",
        ItemKind::BadBronze => "Bozuk bronz",
        ItemKind::MoltenBronze => "Bronz eriyikli pota",
        ItemKind::BronzeAxeHead => "Bronz balta kafası",
        ItemKind::BronzePickHead => "Bronz kazma kafası",
        ItemKind::Plank => "Tahta",
        ItemKind::Saw => "Testere",
        ItemKind::SpongeIron => "Süngerimsi demir",
        ItemKind::Limestone => "Kireçtaşı",
        ItemKind::FireClay => "Ateş kili",
        ItemKind::RawFireBrick => "Ham ateş tuğlası",
        ItemKind::FireBrick => "Ateş tuğlası",
        ItemKind::PigIron => "Pik demir",
        ItemKind::HighCarbonSteel => "Yüksek karbonlu çelik",
        ItemKind::Steel => "Çelik",
        ItemKind::Coal => "Maden kömürü",
        ItemKind::Coke => "Kok",
        ItemKind::CastIronPart => "Dökme demir parça",
        ItemKind::Sand => "Kum",
        ItemKind::Glass => "Cam",
        ItemKind::CopperWire => "Bakır tel",
        ItemKind::Magnet => "Mıknatıs",
        ItemKind::LightBulb => "Ampul",
        ItemKind::Silicon => "Silisyum",
        ItemKind::Transistor => "Transistör",
        ItemKind::CircuitBoard => "Devre kartı",
        ItemKind::Oil => "Petrol",
        ItemKind::ProspectorPick => "Arama kazması",
        ItemKind::RawFish => "Çiğ balık",
        ItemKind::CookedFish => "Pişmiş balık",
        ItemKind::FishingRod => "Olta",



    }
}

pub fn milestone_name(m: Milestone) -> &'static str {
    match m {
        Milestone::FirstFire => "İlk ateşi kur",
        Milestone::Shelter => "Bir barınak yap",
        Milestone::Companion => "Bir hayvanı evcilleştir",
        Milestone::Charcoal => "Kömür üret",
        Milestone::Iron => "Demir kütle üret",
        Milestone::IronTools => "Demir balta ve kazma yap",
        Milestone::Mechanism => "Su gücüyle dövme (şahmerdan)",
        Milestone::Automaton => "Bir otomat üret",
        Milestone::IronAge => "DEMİR ÇAĞI TAMAMLANDI",
        Milestone::Bronze => "Bronz üret",
        Milestone::BronzeTools => "Bronz balta ve kazma yap",
        Milestone::BronzeAge => "BRONZ ÇAĞI TAMAMLANDI",
        Milestone::Visitor => "Bir yabancı ateşe geldi",
        Milestone::Harvest => "İlk hasadı kaldır",
        Milestone::Bread => "Ekmek pişir",
        Milestone::NeolithicAge => "CİLALI TAŞ ÇAĞI TAMAMLANDI",
        Milestone::Wheel => "Bir araba yap ve koş",
        Milestone::Plow => "Bir saban yap",
        Milestone::Winter => "İlk kışı aç kalmadan geç",
        Milestone::FirstMetal => "İlk metali dök",
        Milestone::CopperTools => "Bakır balta ve kazma yap",
        Milestone::CopperAge => "BAKIR ÇAĞI TAMAMLANDI",
        Milestone::AnimalPower => "Hayvan gücüyle öğüt",
        Milestone::Steel => "Çelik döv",
        Milestone::WindPower => "Rüzgârla çalışan bir makine",
        Milestone::BeltLine => "İlk bant hattı",
        Milestone::CardBot => "Kartlı otomat (rotası olan otomat)",
        Milestone::SteelAge => "ÇELİK VE MEKANİK ÇAĞ TAMAMLANDI",
        Milestone::Coal => "Maden kömürü çıkar",
        Milestone::SteamPower => "Buharla çalışan bir makine",
        Milestone::DrainMine => "Su basmış madeni boşalt",
        Milestone::Glass => "Cam erit",
        Milestone::SteamAge => "BUHAR ÇAĞI TAMAMLANDI",
        Milestone::Electricity => "Elektrik üret (dinamo)",
        Milestone::ElectricMotor => "Elektrik motoruyla çalışan bir makine",
        Milestone::ElectricLight => "Elektrik lambası yak",
        Milestone::ElectricAge => "ELEKTRİK ÇAĞI TAMAMLANDI",
        Milestone::Silicon => "Silisyum indir",
        Milestone::Transistor => "Transistör yap",
        Milestone::CircuitBoard => "Devre kartı yap",
        Milestone::ElectronicsAge => "ELEKTRONİK ÇAĞI TAMAMLANDI",
        Milestone::ComputerOn => "Bir bilgisayar çalıştır",
        Milestone::DesignWritten => "Tasarım masasında kendi tarifini yaz",
        Milestone::ComputerAge => "BİLGİSAYAR ÇAĞI: YENİ DÜNYAYA YOL AÇIK",

    }
}

pub fn kind_name(k: AgentKind) -> &'static str {
    match k {
        AgentKind::Human => "İnsan",
        AgentKind::Dog => "Köpek",
        AgentKind::Ox => "Öküz",
        AgentKind::Horse => "At",
        AgentKind::Bot => "Otomat",
    }
}

fn fmt_counts(counts: &[u16; ItemKind::COUNT]) -> String {
    let parts: Vec<String> = ItemKind::ALL
        .iter()
        .filter(|k| counts[**k as usize] > 0)
        .map(|k| format!("{} {}", counts[*k as usize], item_name(*k)))
        .collect();
    if parts.is_empty() { "boş".into() } else { parts.join(", ") }
}

pub fn step_text(step: &RouteStep, w: &World) -> String {
    let bname = |i: u32| {
        w.buildings
            .get(i as usize)
            .map_or("?".to_string(), |b| format!("{} #{}", rules().building(b.kind).name, i + 1))
    };
    match step {
        RouteStep::Harvest { kind, .. } => format!(
            "Topla: {}",
            match kind {
                alaz_sim::NodeKind::StoneOutcrop => "taş",
                alaz_sim::NodeKind::Tree => "odun",
                alaz_sim::NodeKind::BerryBush => "yemiş",
                alaz_sim::NodeKind::ClayDeposit => "kil",
                alaz_sim::NodeKind::IronOutcrop => "demir cevheri",
                alaz_sim::NodeKind::HerbPatch => "şifalı ot",
                alaz_sim::NodeKind::CopperOutcrop => "bakır cevheri",
                alaz_sim::NodeKind::TinOutcrop => "kalay cevheri",
                alaz_sim::NodeKind::Deadwood => "dal",
                alaz_sim::NodeKind::FiberPlant => "lif",
                alaz_sim::NodeKind::CaveMouth => "mağaradaki taş",
            }
        ),
        RouteStep::Deposit { building } => format!("Yükle -> {}", bname(*building)),
        RouteStep::Collect { building, item } => match item {
            Some(k) => format!("Al ({}) <- {}", item_name(*k), bname(*building)),
            None => format!("Al <- {}", bname(*building)),
        },
        RouteStep::Wait { ticks } => format!("Bekle {} sn", *ticks as u64 / TICKS_PER_SECOND),
    }
}

pub fn event_hint(e: &Event, w: &World, me: AgentId) -> Option<String> {
    Some(match e {
        Event::CaveRockfall(a) if *a == me => "Tavandan taşlar yağdı! Yaralandın. Daha derinde tavan daha tehlikeli.".into(),
        Event::CaveLeft { agent, dark } if *agent == me => {
            if *dark { "Meşale söndü. Karanlıkta emekleyerek çıktın, sıyrıklar aldın.".into() } else { "Mağaranın ağzına döndün.".into() }
        }
        Event::Fractured(a) if *a == me => "Bacağın kırıldı! Yavaş yürürsün. İki dal ve iki lifle kırık tahtası yap, bacağına bağla: üç kat hızlı kaynar.".into(),
        Event::WoundInfected(a) if *a == me => "Yaran iltihaplandı, ateşin çıktı. Şifalı ot çiğne, ateşin başında dinlen. Bir dahaki sefere yarayı sar.".into(),
        Event::CaveRockfall(_) | Event::CaveLeft { .. } | Event::Fractured(_) | Event::WoundInfected(_) => return None,
        Event::Fell { agent, height_mm } if *agent == me => {
            let m = *height_mm as f32 / 1000.0;
            if *height_mm < 4_000 {
                format!("{m:.1} metreden düştün, sert indin. Uçurumların kenarında WASD ile koşma: tıkla, yolu kendin bulursun.")
            } else {
                format!("{m:.1} metreden düştün! Ağır yaralandın. Bu yükseklikten bir daha düşmek öldürebilir.")
            }
        }
        Event::Crushed(a) if *a == me => "Devrilen bir ağacın altında kaldın! Biri ağaç keserken düşeceği yönden uzak dur.".into(),
        Event::RockHit(a) if *a == me => "Yuvarlanan bir taş çarptı! Kayalıkların dibinde, yamacın altında durma.".into(),
        Event::Rockfall { x, y } => {
            let a = w.agents.get(me.0 as usize)?;
            if (a.x - x).pow(2) + (a.y - y).pow(2) > 14_000i64 * 14_000 {
                return None;
            }
            "Yukarıda bir kaya koptu, aşağı yuvarlanıyor! Yolundan çekil.".into()
        }
        Event::Fell { .. } | Event::Crushed(_) | Event::RockHit(_) | Event::TreeDown { .. } | Event::RockBounce { .. } => return None,
        Event::CraftAbandoned(a) if *a == me => "İşi yarım bıraktın. Malzemelerin çantanda duruyor, yeniden başlayabilirsin.".into(),
        Event::Discovered(Discovery::SharpStone) => "Elinde iki taş var: Yap sekmesinde ellerine koyup Dene de, vurunca keskin bir kenar çıkar.".into(),
        Event::Discovered(Discovery::StoneAxe) => "Keskin taş, dal ve iki lifi Yap sekmesinde elinde birleştir: balta olur ve ağaçlar iki kat hızlı kesilir.".into(),
        Event::Discovered(Discovery::Fire) => "Kuru odunlar yeterli. Çakmaktaşını taşa vurup kıvılcım çıkarırsan ateş yakılabilir.".into(),
        Event::Discovered(Discovery::Shelter) => "Dallardan eğik bir çatı kurarsan geceyi daha rahat geçirirsin.".into(),
        Event::Discovered(Discovery::Kiln) => "Odunu havasız yakarsan kömür olur. Taşlarla bir kömür çukuru kurabilirsin.".into(),
        Event::Discovered(Discovery::Furnace) => "Kilden ve taştan bir ocak, madeni eritebilir.".into(),
        Event::Discovered(Discovery::Smelting) => "Bronzu işleyen el artık daha sert bir ateşi tanıyor: kömürle uzun süre ısıtılan demir cevheri demire dönüşür. Ocağı kullan.".into(),
        Event::Discovered(Discovery::Depot) => "Taşıdıkların sığmıyor. Ağaçtan bir ambar, eşyaları bir arada tutar.".into(),
        Event::Discovered(Discovery::Workshop) => "Demir kütleyi dövüp işleyebilirsin. Bir atölye, mekanik işçiler yapabilir.".into(),
        Event::Discovered(Discovery::Workbench) => "Düz bir tahta ve taş, işleri hassaslaştırır. Bir tezgah kurabilirsin.".into(),
        Event::Discovered(Discovery::Spear) => "Keskin taş, dal ve lifi Yap sekmesinde birleştir: mızrak olur, av hayvanlarına fırlatırsın.".into(),
        Event::Discovered(Discovery::Cooking) => "Et ateşte pişince daha doyurucu olur ve seni hasta etmez.".into(),
        Event::Discovered(Discovery::Ironwork) => "Demir kütleyi tezgahta dövüp alet yapabilirsin: balta ve kazma.".into(),
        Event::Discovered(Discovery::Mechanics) => "Dişliler gücü aktarır. Akan suyun yanına bir su çarkı kurarsan yakındaki tezgahlar iki kat hızlı çalışır.".into(),
        Event::Discovered(Discovery::Alloying) => "Üç bakır ile bir kalayı birlikte eritirsen bronz olur: demirden önce bilinen sert metal. Ocağı kullan.".into(),
        Event::Discovered(Discovery::Bronzework) => "Bronz kütleyi tezgahta dövüp alet yapabilirsin: balta ve kazma.".into(),
        Event::Discovered(Discovery::Clothing) => "Lif ve post, üşüyen bedeni örter. Yap sekmesinde örtü, pelerin ve çizme yapabilirsin; taşıdıkça giyilir ve sıcak tutar.".into(),
        Event::Discovered(Discovery::Clamp) => "İki dal ve lifi Yap sekmesinde birleştir: kıskaç olur, parçaları senin yerine tutar.".into(),
        Event::Discovered(Discovery::Farming) => "Bu uzun otun başakları tohum döküyor. Keskin taşı iki dala bağlarsan çapa olur; çapayla toprağı sürüp İnşa'dan tarla kur, tohumu ek.".into(),
        Event::Discovered(Discovery::Milling) => "Buğday taneleri çiğ yenmez. Üç taşı düzleyip öğütme taşı yap, taneleri un et, unu ateşte ekmek pişir.".into(),
        Event::Discovered(Discovery::Pottery) => "Islak kil şekil tutuyor. Üç kili halka halka üst üste koyup ateşte pişirirsen çömlek olur: suyu yanında taşırsın.".into(),
        Event::Discovered(Discovery::Weaving) => "Lifleri gergin iplere geçirip sık sık dokursan kumaş olur. Odun ve kil ağırlıklarla bir dokuma tezgahı kur; kumaştan giysi ve torba dikersin.".into(),
        Event::Discovered(Discovery::Husbandry) => "Hayvanlar bir arada, beslenince çoğalır. Odun kazıklarla bir ağıl kur, aynı türden iki hayvanı içine götür, buğdayla besle: yavru olur.".into(),
        Event::Discovered(Discovery::Draught) => "Öküzün gücü bronzla birleşince: tezgahta tekerlek, araba ve saban yapılır. Arabayı koşulan hayvan çok daha fazla taşır; sabanlı hayvan tarlanın yanında ekini iki kat hızlı büyütür.".into(),
        Event::Discovered(Discovery::Hammer) => "Yuvarlak bir taşı dala bağlarsan çekiç olur: kaya damarlarını söker, içinden çakmaktaşı çıkar.".into(),
        Event::Discovered(Discovery::Retired) => return None,
        Event::Discovered(Discovery::Fishing) => "Suda koyu gölgeler: balık sürüleri. Mızrakla ya da daha iyisi oltayla (keskin taş, iki dal, üç lif) sürünün yanındaki suya sağ tıkla. Balık ateşte pişer.".into(),
        Event::Discovered(Discovery::Geology) => "Kaya bir şey anlatır: bronzdan bir arama kazması döv, vurunca çevredeki gizli yatakları ve en yakınının yönünü söyler. Maden kuyusu yatağın üstüne kurulur.".into(),
        Event::Discovered(Discovery::Electricity) => "Mıknatısın yanında dönen bakır tel akım verir. Haddehanede tel çek, demir örste mıknatıs yap; dinamo, direk, motor ve lamba kur.".into(),
        Event::Discovered(Discovery::Electronics) => "Kumdan ve koktan silisyum indirilir. Silisyum ve telle transistör, transistörlerle devre kartı yapılır.".into(),
        Event::Discovered(Discovery::Computing) => "Devre kartları bir araya gelince hesap yapar: bir bilgisayar kur. Ağa bağlı bilgisayar tasarım masasını açar: kendi tarifini yazarsın, sonra yeni bir dünyaya geçebilirsin.".into(),
        Event::Discovered(Discovery::Steam) => "Kaynayan su kapağı kaldırır. Dökme demir silindirli bir buhar makinesi kömür ve suyla makineleri çevirir; bir pompa su basmış madeni boşaltır. Kum ocakta cama döner.".into(),
        Event::Discovered(Discovery::Steelmaking) => "Ateş kili büyük ısıya dayanır, kireçtaşı cürufu taşır. Ateş tuğlasından körüklü bir yüksek fırın pik demir verir; pik demir demir örste dövülünce çelik olur.".into(),
        Event::Discovered(Discovery::Casting) => "Bu yeşilimsi taş ateşte erir. Kilden bir pota ve kalıplar yap, çukur fırında bakırı erit, kalıba dök: ilk metal aletin olur.".into(),
        Event::Discovered(Discovery::Medicine) => "Acı otlar karın ve yara için iyidir. Çiğneyebilir ya da iki otu sargıya çevirebilirsin.".into(),
        Event::Struck { .. } => return None,
        Event::CraftAbandoned(_) => return None,
        Event::Crafted { agent, recipe, quality } => {
            if *agent != me {
                return None;
            }
            let name = &rules().recipes.get(*recipe as usize)?.name;
            match quality {
                2 => format!("{name}: ince iş! Fazladan bir parça çıktı."),
                1 => format!("{name}: iyi oldu."),
                _ => format!("{name}: olmadı, malzeme boşa gitti. Aletlerin olursa hedef büyür."),
            }
        }
        // in coop a bite on someone else (or on a worker) is not your wound
        Event::Bitten(a) if *a != me => return None,
        Event::Bitten(_) => "Kurt ısırdı! Yaran kanıyor. Ateşin yanına çekil ya da mızrakla karşılık ver.".into(),
        Event::Hunted { meat, .. } => format!("Av başarılı: {meat} parça çiğ et. Ateşte pişirmeden yeme."),
        Event::Milestone(Milestone::Visitor) => "Karanlıktan bir köpek ateşinin ışığına yaklaştı. Yanına git, böğürtlen ver: güvenini kazanırsa seninle çalışır.".into(),
        Event::Milestone(m) => format!("Hedef tamamlandı: {}", milestone_name(*m)),
        Event::Died(_) => "Dayanamadın. Her şeyi bırakıp yeniden başladın.".into(),
        Event::Rejected(_, why) => match why {
            Reject::NotLearned => "Bunu henüz bilmiyorsun.".into(),
            Reject::NoMaterials => "Malzeme yetmiyor.".into(),
            Reject::TooFar => "Çok uzaktasın, yaklaş.".into(),
            Reject::Blocked => "Buraya yerleştirilemez.".into(),
            Reject::Busy => "Bu iş hâlâ sürüyor.".into(),
            Reject::PackFull => "Çantan dolu. Bir şeyi yere bırak ya da ambara koy.".into(),
            Reject::Unreachable => "Oraya giden yol yok.".into(),
            Reject::NoIdea => "Bunları birbirine vurdun, bağladın, denedin... hiçbir şey olmadı.".into(),
            Reject::NeedsWater => "Su çarkı akan suyun (3 kare) yakınına kurulur.".into(),
            Reject::Escaped => "Av kaçtı. Daha yakından dene.".into(),
            Reject::NoLight => "Mağaraya girmek için meşale gerek: iki dal ve bir lifle Yap sekmesinde yapılır, yakmak için çakmaktaşı lazım.".into(),
            Reject::NeedsTool { tool, tier } => {
                use alaz_sim::rules::ToolKind;
                let metal = ["taş", "bakır", "bronz", "demir", "çelik"].get(*tier as usize).copied().unwrap_or("daha sert");
                match (tool, tier) {
                    (ToolKind::Axe, 0) => "Ağacı kesmek için balta gerek. Yerdeki dalları elle toplayabilirsin.".into(),
                    (ToolKind::Axe, _) => format!("Bu iş için en az {metal} balta gerek."),
                    (ToolKind::Pick, 0) => "Bu kaya elle sökülmüyor: taş çekiç gerek. Kayalığın üstündeki gevşek taşları elle alabilirsin.".into(),
                    (ToolKind::Pick, _) => format!("Burası çok sert: en az {metal} kazma gerek."),
                    (ToolKind::Knife, _) => format!("Bunun için {metal} bıçak gerek."),
                    (ToolKind::Shovel, _) => format!("Bunun için {metal} kürek gerek."),
                }
            }
            Reject::NoFish => "Burada balık yok. Sudaki koyu gölgeleri ara, sürünün yanına sağ tıkla.".into(),
            Reject::NoDeposit => "Maden kuyusu yalnız gizli bir yatağın üstüne kurulur. Arama kazmasıyla yeri bul.".into(),
            Reject::Unbalanced => "Tasarım masası kabul etmedi: tarif aldığından değerli bir şey veremez, ve yalnız tanıdığın şeylerle yazılır.".into(),
            Reject::Flooded => "En dip oda su altında. Mağara ağzının 15 m yakınına güçle çalışan bir pompa kur.".into(),
            Reject::NeedsStation(kind) => format!("Bunu {} başında yapmalısın.", rules().building(*kind).name.to_lowercase()),
            Reject::CannotHitch => "Araba ve sabanı yalnız öküz ya da at çeker; her biri birer tane.".into(),
            Reject::NeedsPair => "Yavru için o türden iki hayvan ağılın içinde olmalı. İşçi panelinden \"Ağıla götür\" de.".into(),
            Reject::NoFlint => "Kıvılcım çıkaracak bir şey yok: ateş yakmak için çakmaktaşı gerek. Taş damarlarını kazarken arada bir çıkar.".into(),
        },
        Event::Chopped { .. } => return None,
        Event::Caught { agent } if *agent != me => return None,
        Event::Caught { .. } => "Bir balık tuttun.".to_string(),
        Event::Prospected { agent, .. } if *agent != me => return None,
        Event::Prospected { kinds, nearest, .. } => {
            if kinds.is_empty() {
                "Taşı dinledin: 50 m içinde gizli bir yatak yok.".to_string()
            } else {
                let names: Vec<&str> = kinds.iter().map(|k| item_name(*k)).collect();
                let way = nearest.map(|(k, dx, dy)| {
                    let dir = match (dx.signum(), dy.signum()) {
                        (0, -1) => "kuzey", (0, 1) => "güney", (1, 0) => "doğu", (-1, 0) => "batı",
                        (1, -1) => "kuzeydoğu", (-1, -1) => "kuzeybatı", (1, 1) => "güneydoğu", (-1, 1) => "güneybatı", _ => "tam altında",
                    };
                    format!(" En yakını {} {} m {}.", item_name(k).to_lowercase(), ((dx * dx + dy * dy) as f32).sqrt().round(), dir)
                }).unwrap_or_default();
                format!("Taşı dinledin: çevrede {} yatağı var.{way} Maden kuyusunu yatağın üstüne kur.", names.join(", ").to_lowercase())
            }
        }
        Event::ToolBlunt { agent, .. } if *agent != me => return None,
        Event::ToolBlunt { item, .. } => format!("{} köreldi: hâlâ iş görür ama çok yavaş. Çanta sekmesinden onar.", item_name(*item)),
        Event::Spoiled { agent, .. } if *agent != me => return None,
        Event::Spoiled { item, n, .. } => format!("{n} {} bozuldu, gübreye döndü. Tahıl ve ekmek uzun dayanır; ambardaki büyük çömlek korur.", item_name(*item).to_lowercase()),
        Event::CropFrozen { .. } => "Kış ekini dondurdu. Ekimi sonbahardan önce bitir.".into(),
        Event::JobDone { building } => {
            let b = w.buildings.get(*building as usize)?;
            format!("{}: iş bitti.", rules().building(b.kind).name)
        }
        Event::Tamed(k) if *k == AgentKind::Dog => "Köpek güvenini kazandı. Seni takip eder, yanındayken kurtlar yaklaşamaz.".to_string(),
        Event::Tamed(k) => format!("{} güvenini kazandı ve artık seninle çalışıyor.", kind_name(*k)),
        Event::WorkerReady(k) => format!("{} hazır. Ona bir rota verebilirsin.", kind_name(*k)),
    })
}

/// Nearest water tile within the search radius of the agent.
pub fn nearest_water(w: &World, x_mm: i64, y_mm: i64) -> Option<(i32, i32)> {
    let (cx, cy) = ((x_mm / 1000) as i32, (y_mm / 1000) as i32);
    let r = MAX_TILE_SEARCH;
    let mut best: Option<(i64, (i32, i32))> = None;
    for ty in cy - r..=cy + r {
        for tx in cx - r..=cx + r {
            if w.map.terrain(tx, ty) == Some(Terrain::Water) {
                let d = ((tx - cx) as i64).pow(2) + ((ty - cy) as i64).pow(2);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, (tx, ty)));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn slot_cell(ui: &mut egui::Ui, w: &World, k: ItemKind, n: u16) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(72.0, 78.0), egui::Sense::click());
    let p = ui.painter();
    let icon = egui::Rect::from_min_size(rect.min, egui::vec2(72.0, 56.0));
    crate::icons::draw_slot(p, icon, k, w.has_seen(k));
    p.text(icon.right_bottom() - egui::vec2(5.0, 3.0), Align2::RIGHT_BOTTOM, n.to_string(), egui::FontId::proportional(22.0), Color32::WHITE);
    let name = crate::lang::item_word(w, k);
    p.text(egui::pos2(rect.center().x, rect.bottom() - 9.0), Align2::CENTER_CENTER, &name, egui::FontId::proportional(11.0), Color32::from_gray(205));
    resp.on_hover_text(name)
}

/// Each ingredient as its icon with "have/need", green when the bag has enough and red when it does not.
/// Whether everything that must be carried (and is not used up) is in the pack.
fn has_needs(needs: &[ItemKind], inv: &alaz_sim::world::Inventory) -> bool {
    needs.iter().all(|&k| inv.get(k) > 0)
}

/// Things that must be carried but are kept, after the materials: an icon each, marked "elde" (in hand).
fn needs_row(ui: &mut egui::Ui, w: &World, needs: &[ItemKind], inv: &alaz_sim::world::Inventory) {
    if needs.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Elde olmalı:").small().weak());
        for &k in needs {
            let ok = inv.get(k) > 0;
            let col = if ok { Color32::from_rgb(130, 215, 130) } else { Color32::from_rgb(235, 110, 110) };
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            crate::icons::draw_slot(ui.painter(), rect, k, w.has_seen(k));
            ui.label(RichText::new(if ok { "var" } else { "yok" }).color(col).small());
            resp.on_hover_text(format!("{} (harcanmaz)", crate::lang::item_word(w, k)));
        }
    });
}

fn need_row(ui: &mut egui::Ui, w: &World, items: &[ItemCount], inv: &alaz_sim::world::Inventory) {
    ui.horizontal_wrapped(|ui| {
        for it in items {
            let have = inv.get(it.item);
            let ok = have >= it.n;
            let col = if ok { Color32::from_rgb(130, 215, 130) } else { Color32::from_rgb(235, 110, 110) };
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            crate::icons::draw_slot(ui.painter(), rect, it.item, w.has_seen(it.item));
            ui.label(RichText::new(format!("{}/{}", have, it.n)).color(col));
            ui.add_space(4.0);
            resp.on_hover_text(crate::lang::item_word(w, it.item));
        }
    });
}

/// Perception level of the tribe, mirrored here so panels can dress themselves for the era.
static ERA: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);

pub const DISCOVERIES: [Discovery; 34] = [
    Discovery::SharpStone,
    Discovery::StoneAxe,
    Discovery::Hammer,
    Discovery::Clamp,
    Discovery::Fire,
    Discovery::Shelter,
    Discovery::Clothing,
    Discovery::Spear,
    Discovery::Cooking,
    Discovery::Medicine,
    Discovery::Farming,
    Discovery::Milling,
    Discovery::Pottery,
    Discovery::Weaving,
    Discovery::Husbandry,
    Discovery::Casting,
    Discovery::Depot,
    Discovery::Kiln,
    Discovery::Furnace,
    Discovery::Smelting,
    Discovery::Workbench,
    Discovery::Ironwork,
    Discovery::Mechanics,
    Discovery::Workshop,
    Discovery::Alloying,
    Discovery::Bronzework,
    Discovery::Draught,
    Discovery::Steelmaking,
    Discovery::Steam,
    Discovery::Electricity,
    Discovery::Electronics,
    Discovery::Computing,
    Discovery::Geology,
    Discovery::Fishing,
];

pub fn discovery_name(d: Discovery) -> &'static str {
    match d {
        Discovery::SharpStone => "Keskin taş",
        Discovery::StoneAxe => "Taş balta",
        Discovery::Clamp => "Kıskaç",
        Discovery::Fire => "Ateş",
        Discovery::Shelter => "Barınak",
        Discovery::Clothing => "Giysi",
        Discovery::Spear => "Mızrak",
        Discovery::Cooking => "Pişirme",
        Discovery::Medicine => "Şifa",
        Discovery::Depot => "Ambar",
        Discovery::Kiln => "Kömür",
        Discovery::Furnace => "Ocak",
        Discovery::Smelting => "Eritme",
        Discovery::Workbench => "Tezgah",
        Discovery::Ironwork => "Demircilik",
        Discovery::Mechanics => "Mekanik",
        Discovery::Workshop => "Atölye",
        Discovery::Alloying => "Alaşım",
        Discovery::Bronzework => "Bronz işi",
        Discovery::Farming => "Tarım",
        Discovery::Milling => "Öğütme",
        Discovery::Pottery => "Çömlekçilik",
        Discovery::Weaving => "Dokuma",
        Discovery::Husbandry => "Hayvancılık",
        Discovery::Draught => "Çeki gücü",
        Discovery::Hammer => "Çekiç",
        Discovery::Casting => "Döküm",
        Discovery::Retired => "",
        Discovery::Steelmaking => "Çelik",
        Discovery::Steam => "Buhar",
        Discovery::Electricity => "Elektrik",
        Discovery::Electronics => "Elektronik",
        Discovery::Computing => "Bilgisayar",
        Discovery::Geology => "Jeoloji",
        Discovery::Fishing => "Balıkçılık",
    }
}

/// The age the tribe lives in, from what it has achieved.
/// The body the tribe has grown into, from the first ape to the sage.
pub const STAGES: [&str; 6] = ["Maymun", "Alet yapan", "Ateşçi", "Avcı", "Usta", "Bilge"];

/// What each step changes about the body, in plain words.
pub fn stage_gift(stage: u8) -> &'static str {
    match stage {
        0 => "Kamburun çıkık, elin sakar. İşler biraz yavaş, el işinde hedef dar.",
        1 => "Alet tutmayı öğrendin. Elin biraz daha sağlam, duruşun dikleşiyor.",
        2 => "Ateşi tanıdın. Daha dik yürüyorsun, el işi kolaylaştı.",
        3 => "Avcı oldun. Vücudun düzeldi, işler hızlandı.",
        4 => "Ustalaştın. Eller emin, toplama ve zanaat belirgin hızlı.",
        _ => "Dik, becerikli, bilge bir insansın. Hedefler en geniş, işler en hızlı.",
    }
}

/// The age banners: shown as badges, not as goals to work toward.
pub fn is_age(m: Milestone) -> bool {
    matches!(m, Milestone::NeolithicAge | Milestone::CopperAge | Milestone::BronzeAge | Milestone::IronAge | Milestone::SteelAge | Milestone::SteamAge | Milestone::ElectricAge | Milestone::ElectronicsAge | Milestone::ComputerAge)
}

pub fn era_name(w: &World) -> &'static str {
    // latest age first: iron follows bronze
    if w.milestone_done(Milestone::ComputerAge) {
        "Bilgisayar Çağı"
    } else if w.milestone_done(Milestone::ElectronicsAge) {
        "Elektronik Çağı"
    } else if w.milestone_done(Milestone::ElectricAge) {
        "Elektrik Çağı"
    } else if w.milestone_done(Milestone::SteamAge) {
        "Buhar Çağı"
    } else if w.milestone_done(Milestone::SteelAge) {
        "Çelik ve Mekanik Çağ"
    } else if w.milestone_done(Milestone::IronAge) {
        "Demir Çağı"
    } else if w.milestone_done(Milestone::BronzeAge) {
        "Bronz Çağı"
    } else if w.milestone_done(Milestone::CopperAge) {
        "Bakır Çağı"
    } else if w.milestone_done(Milestone::Bronze) || w.milestone_done(Milestone::Iron) || w.milestone_done(Milestone::Charcoal) {
        "Maden Devri"
    } else if w.milestone_done(Milestone::NeolithicAge) {
        "Cilalı Taş Çağı"
    } else if w.milestone_done(Milestone::Harvest) {
        "İlk Çiftçiler"
    } else if w.milestone_done(Milestone::FirstFire) {
        "Ateş Devri"
    } else {
        "Taş Devri"
    }
}

fn draw_status(ctx: &egui::Context, w: &World, _me: &alaz_sim::world::Agent, ui_ctx: &UiCtx) {
    use crate::ui_art::{badge, chip, pips, stat_bar, Glyph};
    let level = w.perception_level();
    egui::Area::new(egui::Id::NULL.with("status")).anchor(Align2::LEFT_TOP, [12.0, 12.0]).show(ctx, |ui| {
        panel_frame().inner_margin(8).show(ui, |ui| {
            ui.set_width(250.0);
            if !ui_ctx.net_status.is_empty() {
                ui.colored_label(Color32::from_rgb(120, 190, 240), ui_ctx.net_status);
            }
            let m = minute_of_day(w.tick);
            let sky = w.weather();
            let daytime = (360..1200).contains(&m);
            ui.horizontal(|ui| {
                let (g, col) = match sky {
                    alaz_sim::time::Weather::Rain => (Glyph::Rain, Color32::from_rgb(170, 190, 215)),
                    alaz_sim::time::Weather::Snow => (Glyph::Snow, Color32::from_rgb(220, 232, 245)),
                    _ if daytime => (Glyph::Sun, Color32::from_rgb(250, 200, 70)),
                    _ => (Glyph::Moon, Color32::from_rgb(200, 210, 235)),
                };
                let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                badge(ui.painter(), rect.center(), 14.0, g, col);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut head = format!("{} {}", crate::lang::day_word(w), w.day() + 1);
                    if level >= 2 {
                        head += &format!("   {:02}:{:02}", m / 60, m % 60);
                    }
                    ui.label(RichText::new(head).strong());
                    let season = ["Bahar", "Yaz", "Güz", "Kış"][w.season() as usize];
                    let sky_word = match sky {
                        alaz_sim::time::Weather::Clear => "",
                        alaz_sim::time::Weather::Rain => "yağmur",
                        alaz_sim::time::Weather::Snow => "kar",
                    };
                    let mut parts: Vec<String> = Vec::new();
                    if level >= 3 {
                        parts.push(season.to_string());
                        parts.push(format!("{:.0}°C", w.ambient_x10() as f32 / 10.0));
                    }
                    if !sky_word.is_empty() {
                        parts.push(sky_word.to_string());
                    }
                    if !parts.is_empty() {
                        ui.label(RichText::new(parts.join("  ·  ")).small().color(Color32::from_rgb(170, 190, 220)));
                    }
                });
            });
            ui.add_space(3.0);
            let stage = level.min(5);
            ui.horizontal(|ui| {
                chip(ui, era_name(w), Color32::from_rgb(78, 60, 22), Color32::from_rgb(248, 218, 120));
                pips(ui, stage as usize + 1, 6, Color32::from_rgb(160, 214, 130)).on_hover_text(format!("Evrim: {} — {}", STAGES[stage as usize], stage_gift(stage)));
            });
            stat_bar(ui, Glyph::Eye, crate::lang::perception_word(level), w.perception_permille() as f32 / 1000.0, Color32::from_rgb(110, 146, 214), 250.0)
                .on_hover_text(format!("Evrim: {}", STAGES[stage as usize]));
        });
    });
}

/// A vital: a row of ten pixel icons; its name (and the exact amount, once the tribe counts) on hover.
fn vital(ui: &mut egui::Ui, w: &World, g: crate::ui_art::Glyph, name: &str, v: i32, col: Color32, _width: f32) {
    let level = w.perception_level();
    let tip = if level >= 4 { format!("{name}: {}%", v * 100 / MAX_STAT.max(1)) } else { name.to_string() };
    crate::ui_art::vital_icons(ui, g, v as f32 / MAX_STAT as f32, col, 2.0).on_hover_text(tip);
}

/// Bottom left: the body. Health and breath, and what is wrong with it.
fn draw_body(ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent) {
    use crate::ui_art::{chip, Glyph};
    egui::Area::new(egui::Id::NULL.with("body")).anchor(Align2::LEFT_BOTTOM, [12.0, -12.0]).show(ctx, |ui| {
        panel_frame().inner_margin(8).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let cap = me.health_cap(&w.settings);
            vital(ui, w, Glyph::Heart, crate::lang::stat_word(w, "Sağlık"), me.health, Color32::from_rgb(214, 70, 70), 180.0);
            vital(ui, w, Glyph::Breath, "Soluk", me.stamina, Color32::from_rgb(120, 200, 130), 180.0);
            if cap < MAX_STAT && w.perception_level() >= 2 {
                ui.label(RichText::new(format!("Beslenme eksik: en çok %{} can", cap * 100 / MAX_STAT)).small().color(Color32::from_rgb(230, 160, 90)));
            }
            let depth = alaz_sim::map::water_depth(&w.map, me.x.div_euclid(1000) as i32, me.y.div_euclid(1000) as i32);
            if depth == 2 {
                if me.stamina == 0 {
                    chip(ui, "Boğuluyorsun: kıyıya çık!", Color32::from_rgb(92, 26, 26), Color32::from_rgb(250, 140, 130));
                } else {
                    chip(ui, "Yüzüyorsun", Color32::from_rgb(22, 50, 84), Color32::from_rgb(150, 200, 250));
                }
            } else if depth == 1 {
                chip(ui, "Sığ suda", Color32::from_rgb(22, 50, 84), Color32::from_rgb(150, 200, 250));
            }
            if me.injury > 0 || me.sick > 0 || me.broken > 0 {
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(180.0);
                    if me.injury > 0 {
                        chip(ui, crate::lang::stat_word(w, "Kanıyorsun"), Color32::from_rgb(92, 26, 26), Color32::from_rgb(250, 140, 130));
                    }
                    if me.sick > 0 {
                        chip(ui, crate::lang::stat_word(w, "Hastasın"), Color32::from_rgb(54, 66, 22), Color32::from_rgb(196, 224, 110));
                    }
                    if me.broken > 0 {
                        chip(ui, &format!("Kırık bacak {} sn", me.broken / 60), Color32::from_rgb(84, 60, 24), Color32::from_rgb(240, 190, 110));
                    }
                });
            }
        });
    });
}

/// Bottom right: the needs. Food, water, warmth, and (once the tribe can tell) the food groups.
fn draw_needs(ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent) {
    use crate::ui_art::Glyph;
    egui::Area::new(egui::Id::NULL.with("needs")).anchor(Align2::RIGHT_BOTTOM, [-12.0, -12.0]).show(ctx, |ui| {
        panel_frame().inner_margin(8).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            vital(ui, w, Glyph::Food, crate::lang::stat_word(w, "Tokluk"), me.hunger, Color32::from_rgb(214, 156, 56), 180.0);
            vital(ui, w, Glyph::Drop, crate::lang::stat_word(w, "Su"), me.thirst, Color32::from_rgb(64, 140, 224), 180.0);
            vital(ui, w, Glyph::Flame, crate::lang::stat_word(w, "Sıcaklık"), me.warmth, Color32::from_rgb(232, 118, 44), 180.0);
            if w.perception_level() >= 2 {
                for (g, name) in alaz_sim::world::NUTRITION_GROUPS.iter().enumerate() {
                    let v = me.nutrition[g];
                    crate::ui_art::group_meter(ui, g, v as f32 / 1000.0, v >= 300).on_hover_text(format!("{name}: {v}‰ (300 ve üstü sağlığa katkı)"));
                }
            }
        });
    });
}

/// The very next concrete thing to do, from what the player has and knows: the first ten minutes as a
/// gentle script. Urgent needs come first; later the next unfinished goal takes over.
pub fn guide_text(w: &World, me: &alaz_sim::world::Agent) -> String {
    let inv = &me.inventory;
    let n = |k: ItemKind| inv.get(k);
    let fire = w.buildings.iter().any(|b| b.kind == alaz_sim::BuildingKind::Campfire);
    if !w.milestone_done(Milestone::FirstFire) || w.day() == 0 {
        if me.thirst < 2500 {
            return "Susadın: suya yaklaş, Su iç de.".into();
        }
        if me.hunger < 2500 && n(ItemKind::Berries) > 0 {
            return "Acıktın: Ye de, yemiş yersin.".into();
        }
        if me.hunger < 2500 {
            return "Acıktın: bir yemiş çalısından topla.".into();
        }
    }
    // autumn: the winter is coming and nothing grows in it
    if fire && w.season() == 2 && !w.milestone_done(Milestone::Winter) {
        let food = n(ItemKind::Grain) + n(ItemKind::Bread) + n(ItemKind::Lentil) + n(ItemKind::Flour) * 2;
        if food < 12 {
            return "Güz geldi: kışın ekin büyümez. Tahıl, mercimek ve ekmek biriktir; et ve yemiş çabuk bozulur.".into();
        }
    }
    if !fire {
        if !w.knows(Discovery::SharpStone) {
            return if n(ItemKind::Stone) >= 2 {
                "İki taşı Yap sekmesinde ellerine koy ve Dene de.".into()
            } else {
                "Yerdeki taşlara sağ tıkla, iki taş topla.".into()
            };
        }
        if n(ItemKind::SharpStone) == 0 && n(ItemKind::StoneAxe) == 0 && n(ItemKind::Spear) == 0 && n(ItemKind::Stone) >= 2 {
            return "Yap sekmesinde iki taşı ellerine koy, Yap de: keskin taş çıkar.".into();
        }
        if !w.knows(Discovery::StoneAxe) {
            return if n(ItemKind::Branch) >= 1 && n(ItemKind::Fiber) >= 2 {
                "Keskin taş, dal ve iki lif ellerinde olsun: Dene de, balta yap.".into()
            } else {
                "Balta için bir dal ve iki lif topla.".into()
            };
        }
        if n(ItemKind::Wood) < 3 {
            return "Baltanla bir ağaca sağ tıkla: odun topla, ateş için üç lazım.".into();
        }
        if n(ItemKind::Flint) == 0 && n(ItemKind::Hammer) == 0 {
            return "Ateş için çakmaktaşı lazım, o da kaya damarında: önce taş çekiç yap (iki taş, dal ve lif).".into();
        }
        if n(ItemKind::Flint) == 0 {
            return "Ateşi yakmak için çakmaktaşı lazım: çekiçle bir taş damarına sağ tıkla, kazarken arada bir çıkar.".into();
        }
        return "Yeterli odunun var: İnşa sekmesinden bir kamp ateşi kur.".into();
    }
    if me.warmth < 3000 {
        return "Üşüyorsun: ateşe yaklaş.".into();
    }
    let next = Milestone::ALL.iter().find(|m| !is_age(**m) && !w.milestone_done(**m));
    // the farming goals get spelled out step by step, like the road to the first fire
    let field = w.buildings.iter().find(|b| b.kind == alaz_sim::BuildingKind::Field);
    match next {
        Some(Milestone::Harvest) => {
            return if !w.knows(Discovery::Farming) {
                "Uzun otlara sağ tıkla: başaklarından arada bir tohum dökülür.".into()
            } else if n(ItemKind::Hoe) == 0 {
                "Keskin taş, iki dal ve bir lifi ellerinde dene: çapa olur.".into()
            } else if field.is_none() {
                "Çapan var: İnşa sekmesinden bir tarla kur.".into()
            } else if field.is_some_and(|f| f.job.is_some()) {
                "Ekin büyüyor. Bekle, sonra tarlaya sağ tıklayıp hasadı topla.".into()
            } else if field.is_some_and(|f| f.output_total() > 0) {
                "Ekin olgun: tarlaya sağ tıkla, hasadı topla.".into()
            } else if n(ItemKind::Seed) < 2 {
                "Ekim için iki tohum lazım: uzun otlardan topla.".into()
            } else {
                "Tarlaya sağ tıkla ve tohumu ek.".into()
            };
        }
        Some(Milestone::FirstMetal) | Some(Milestone::CopperTools) => {
            let kiln = w.buildings.iter().any(|b| b.kind == alaz_sim::BuildingKind::PitKiln);
            return if !w.knows(Discovery::Casting) {
                "Kayalıklarda yeşilimsi bakır taşı ara; üstteki parçalar elle alınır.".into()
            } else if !kiln {
                "Bakırı eritmek için kürekle bir çukur fırın kaz.".into()
            } else if n(ItemKind::Crucible) + n(ItemKind::MoltenCopper) == 0 {
                "Kilden ve liften bir pota yap, çukur fırında pişir.".into()
            } else if n(ItemKind::MoldAxe) + n(ItemKind::MoldPick) == 0 {
                "Kili taş baltana ve çekicine bastırıp balta ve kazma kalıbı yap, fırında pişir.".into()
            } else if n(ItemKind::MoltenCopper) == 0 {
                "Potayı dört bakır taşı, lif ve odunla çukur fırına koy: bakır erir.".into()
            } else {
                "Eriyik dolu potayı kalıpla birlikte Yap sekmesinde dök, sonra kafayı dala bağla.".into()
            };
        }
        Some(Milestone::Wheel) | Some(Milestone::Plow) => {
            let draught = w.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse));
            let want = if next == Some(&Milestone::Wheel) { "araba (iki tekerlek + 6 odun)" } else { "saban (3 odun + 1 bronz)" };
            return if !draught {
                "Çeki gücü için bir öküz ya da at evcilleştir: yemişle yaklaş.".into()
            } else if !w.knows(Discovery::Draught) {
                "Bronzu tezgahta işle: öküzün gücüyle tekerlek fikri gelir.".into()
            } else {
                format!("Tezgahta {want} yap.")
            };
        }
        Some(Milestone::Bread) => {
            return if n(ItemKind::Quern) == 0 {
                "Üç taşı ellerinde dene: yüzünü düzlersen öğütme taşı olur.".into()
            } else if n(ItemKind::Flour) == 0 {
                "Yap sekmesinde üç buğdayı öğütme taşıyla una çevir.".into()
            } else if !fire {
                "Unu pişirmek için bir kamp ateşi kur.".into()
            } else {
                "Kamp ateşine sağ tıkla: unu koyup ekmek pişir.".into()
            };
        }
        _ => {}
    }
    let farmers = w.milestone_done(Milestone::Harvest) && w.milestone_done(Milestone::Bread);
    let potter = n(ItemKind::Pot) + n(ItemKind::WaterPot) > 0 || w.milestone_done(Milestone::NeolithicAge);
    if farmers && !potter {
        let kiln = w.buildings.iter().any(|b| b.kind == alaz_sim::BuildingKind::PitKiln);
        return if n(ItemKind::RawPot) > 0 && n(ItemKind::Shovel) == 0 && !kiln {
            "Çömlek kamp ateşinde pişmez, çukur fırın gerek: önce taş kürek yap (taş, iki dal, lif).".into()
        } else if n(ItemKind::RawPot) > 0 && !kiln {
            "Kürekle bir çukur fırın kaz (İnşa sekmesi).".into()
        } else if n(ItemKind::RawPot) > 0 {
            "Ham çömleği iki lif ve iki odunla çukur fırına koy: yarım günde pişer.".into()
        } else if n(ItemKind::Clay) < 3 {
            "Çömlek için üç kil topla: su kenarındaki kumda bulunur.".into()
        } else {
            "Üç kili ellerinde dene: halka halka çömlek yaparsın.".into()
        };
    }
    match next {
        Some(m) => format!("{}: {}", crate::lang::goal_word(w), milestone_name(*m)),
        None => "Tüm hedefler tamam".to_string(),
    }
}

/// The next step, always visible so a new player knows what to try.
fn draw_goal_banner(ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent) {
    let text = guide_text(w, me);
    egui::Area::new(egui::Id::NULL.with("goal")).anchor(Align2::CENTER_TOP, [0.0, 12.0]).show(ctx, |ui| {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(34, 26, 16, 214))
            .stroke(Stroke::new(1.4, Color32::from_rgb(190, 150, 70)))
            .inner_margin(egui::Margin::symmetric(12, 6))
            .corner_radius(0)
            .shadow(egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(150) })
            .show(ui, |ui| {
                // never wider than the room between the corner panels; long goals wrap
                ui.set_max_width((ctx.content_rect().width() - 760.0).clamp(280.0, 620.0));
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    crate::ui_art::badge(ui.painter(), rect.center(), 11.0, crate::ui_art::Glyph::Flag, Color32::from_rgb(240, 190, 80));
                    ui.add(egui::Label::new(RichText::new(text).color(Color32::from_rgb(244, 218, 130))).wrap());
                });
            });
    });
}

fn draw_hints(ctx: &egui::Context, w: &World, ui_ctx: &UiCtx, tab_open: bool) {
    if ui_ctx.hints.is_empty() {
        return;
    }
    // a game's message line, not boxes in the middle of the view: just above the bar, newest last, fading out
    // (with a tab panel open the line moves up under the goal banner, out of the panel's way)
    let (at, off) = if tab_open { (Align2::CENTER_TOP, 96.0) } else { (Align2::CENTER_BOTTOM, -86.0) };
    egui::Area::new(egui::Id::NULL.with("hints")).anchor(at, [0.0, off]).interactable(false).show(ctx, |ui| {
        ui.set_max_width((ctx.content_rect().width() - 760.0).clamp(280.0, 620.0));
        for (text, ttl) in ui_ctx.hints.iter().rev().take(2).rev() {
            let alpha = ((ttl / 2.0).min(1.0) * 255.0) as u8;
            egui::Frame::new().fill(Color32::from_black_alpha(alpha / 2)).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(crate::lang::hint_text(w, text)).color(Color32::from_rgba_unmultiplied(246, 236, 214, alpha))).wrap());
            });
        }
    });
}

/// Bottom bar: panel tabs on the left, things you do right now on the right.
fn draw_action_bar(ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent, ui_ctx: &UiCtx, acts: &mut Vec<UiAction>) {
    use crate::ui_art::{icon_button, Glyph};
    let defs = rules();
    let inv = &me.inventory;
    let has_build = defs.buildings.iter().any(|b| w.knows(b.discovery));
    let has_routes = w.agents.iter().any(|a| a.kind.is_worker()) || w.knows(Discovery::Depot);
    // left edge, middle: the menus (things to look at and plan with)
    egui::Area::new(egui::Id::NULL.with("menus")).anchor(Align2::LEFT_CENTER, [12.0, 0.0]).show(ctx, |ui| {
        panel_frame().inner_margin(6).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let gold = Color32::from_rgb(244, 208, 120);
            let mut tab_btn = |ui: &mut egui::Ui, g: Glyph, label: &str, tab: Tab, action: Action, enabled: bool| {
                let key = ui_ctx.binds.short(action);
                if crate::ui_art::icon_only(ui, g, label, Some(key.as_str()), ui_ctx.tab == Some(tab), enabled, gold).clicked() {
                    acts.push(UiAction::ToggleTab(tab));
                }
            };
            tab_btn(ui, Glyph::Bag, crate::lang::tab_word(w, "Çanta"), Tab::Bag, Action::Bag, true);
            tab_btn(ui, Glyph::Hammer, crate::lang::tab_word(w, "Yap"), Tab::Craft, Action::Craft, true);
            tab_btn(ui, Glyph::House, crate::lang::tab_word(w, "İnşa"), Tab::Build, Action::Build, has_build);
            ui.separator();
            tab_btn(ui, Glyph::Flag, crate::lang::tab_word(w, "Hedefler"), Tab::Goals, Action::Goals, true);
            if has_routes {
                tab_btn(ui, Glyph::Route, crate::lang::tab_word(w, "Rotalar"), Tab::Routes, Action::Routes, true);
            }
            ui.separator();
            tab_btn(ui, Glyph::Help, "Yardım", Tab::Help, Action::Help, true);
        });
    });
    // bottom middle, between the body and the needs: what can be done for them right now
    egui::Area::new(egui::Id::NULL.with("bar")).anchor(Align2::CENTER_BOTTOM, [0.0, -12.0]).show(ctx, |ui| {
        panel_frame().inner_margin(8).show(ui, |ui| {
            ui.horizontal(|ui| {
                let btn = |ui: &mut egui::Ui, g: Glyph, text: String, on: bool| {
                    let tint = match g {
                        Glyph::Drop => Color32::from_rgb(90, 160, 235),
                        Glyph::Food => Color32::from_rgb(226, 150, 70),
                        Glyph::Leg => Color32::from_rgb(220, 190, 140),
                        _ => Color32::from_rgb(230, 100, 100),
                    };
                    icon_button(ui, g, &text, None, false, on, tint).clicked()
                };
                // survival: water and food
                let water = nearest_water(w, me.x, me.y);
                let pot_water = inv.get(ItemKind::WaterPot) > 0;
                let empty_pots = inv.get(ItemKind::Pot);
                let label = match (water.is_some(), pot_water) {
                    (true, _) if empty_pots > 0 => "Su iç, çömlek doldur".to_string(),
                    (false, true) => format!("Çömlekten iç ({})", inv.get(ItemKind::WaterPot)),
                    _ => "Su iç".to_string(),
                };
                if btn(ui, Glyph::Drop, label, water.is_some() || pot_water) {
                    match water {
                        Some((tx, ty)) => acts.push(UiAction::Cmd(CommandKind::Drink { tx, ty })),
                        None => acts.push(UiAction::Cmd(CommandKind::Eat { item: ItemKind::WaterPot })),
                    }
                }
                let food = [ItemKind::Bread, ItemKind::Soup, ItemKind::CookedMeat, ItemKind::Berries, ItemKind::RawMeat].into_iter().find(|k| inv.get(*k) > 0);
                if btn(ui, Glyph::Food, food.map_or("Ye".to_string(), |k| format!("Ye: {}", item_name(k))), food.is_some()) {
                    if let Some(item) = food {
                        acts.push(UiAction::Cmd(CommandKind::Eat { item }));
                    }
                }
                // care: only when something is wrong and there is a cure in the bag
                let care = (me.injury > 0 && inv.get(ItemKind::Bandage) > 0) || (me.broken > 0 && inv.get(ItemKind::Splint) > 0) || (me.sick > 0 && inv.get(ItemKind::Herb) > 0);
                if care {
                    ui.separator();
                }
                if me.injury > 0 && inv.get(ItemKind::Bandage) > 0 && btn(ui, Glyph::Plus, "Sargı sar".into(), true) {
                    acts.push(UiAction::Cmd(CommandKind::Eat { item: ItemKind::Bandage }));
                }
                if me.broken > 0 && inv.get(ItemKind::Splint) > 0 && btn(ui, Glyph::Leg, "Kırığı bağla".into(), true) {
                    acts.push(UiAction::Cmd(CommandKind::Eat { item: ItemKind::Splint }));
                }
                if me.sick > 0 && inv.get(ItemKind::Herb) > 0 && btn(ui, Glyph::Plus, "Ot çiğne".into(), true) {
                    acts.push(UiAction::Cmd(CommandKind::Eat { item: ItemKind::Herb }));
                }
                // tools: what a carried tool lets you do
                if inv.get(ItemKind::ProspectorPick) > 0 {
                    ui.separator();
                    if btn(ui, Glyph::Leg, "Ara".into(), true) {
                        acts.push(UiAction::Cmd(CommandKind::Prospect));
                    }
                }
            });
        });
    });
}

fn tab_bag(ui: &mut egui::Ui, w: &World, me: &alaz_sim::world::Agent, acts: &mut Vec<UiAction>) {
    let inv = &me.inventory;
    crate::ui_art::stat_bar(
        ui,
        crate::ui_art::Glyph::Bag,
        &format!("{}/{} yer dolu (aletler sayılmaz)", inv.load(), me.capacity()),
        inv.load() as f32 / me.capacity().max(1) as f32,
        Color32::from_rgb(150, 120, 70),
        200.0,
    );
    ui.add_space(4.0);
    let items: Vec<(ItemKind, u16)> = ItemKind::ALL.iter().map(|k| (*k, inv.get(*k))).filter(|(_, n)| *n > 0).collect();
    if items.is_empty() {
        ui.label("Çanta boş. Etraftan taş, odun ve yemiş topla: bir şeye sağ tıkla.");
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (k, n) in items {
            // right click (or a plain click) on a slot: put it down
            slot_cell(ui, w, k, n).context_menu(|ui| {
                if ui.button("1 tane bırak").clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Drop { item: k, n: 1 }));
                    ui.close();
                }
                if n > 1 && ui.button(format!("{} tane bırak", n.min(10))).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Drop { item: k, n: n.min(10) }));
                    ui.close();
                }
            });
        }
    });
    ui.label(RichText::new("Bırakmak için eşyaya sağ tıkla. Yerdekini toplamak için üstüne sağ tıkla.").small().weak());
    // tools wear with use; a blunt one still works, three times slower, until it is mended
    let worn: Vec<_> = w.rules().tools.iter().filter(|t| t.durability > 0 && me.inventory.get(t.item) > 0).collect();
    if !worn.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("Aletler").strong());
        for t in worn {
            let life = alaz_sim::world::tool_life(t, &w.settings).max(1);
            let used = me.wear_of(t.item);
            let left = life.saturating_sub(used);
            ui.horizontal(|ui| {
                let label = if left == 0 { format!("{}: kör", item_name(t.item)) } else { format!("{}: {left}/{life}", item_name(t.item)) };
                let col = if left == 0 { Color32::from_rgb(220, 90, 70) } else { Color32::from_rgb(140, 190, 140) };
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Hammer, &label, left as f32 / life as f32, col, 220.0);
                if let Some(mat) = t.repair.filter(|_| used > 0) {
                    let ok = me.inventory.get(mat) > 0;
                    if ui.add_enabled(ok, egui::Button::new(format!("Onar (1 {})", item_name(mat).to_lowercase()))).clicked() {
                        acts.push(UiAction::Cmd(CommandKind::Repair { item: t.item }));
                    }
                }
            });
        }
    }
}

fn tab_craft(ui: &mut egui::Ui, w: &World, me: &alaz_sim::world::Agent, hands: &mut [u8; 4], acts: &mut Vec<UiAction>) {
    let defs = rules();
    // hands: put things together and see what comes of it
    let held = |k: ItemKind, hands: &[u8; 4]| hands.iter().filter(|&&i| i == k as u8).count() as u16;
    // the bag can change under the hands (eaten, built with, dropped in a depot): let go of what is no longer there,
    // or "Dene" would try a mix the player cannot see they no longer have
    let mut taken = [0u16; ItemKind::COUNT];
    for slot in hands.iter_mut() {
        if let Some(&k) = ItemKind::ALL.get(*slot as usize) {
            taken[k as usize] += 1;
            if taken[k as usize] > me.inventory.get(k) {
                *slot = 255;
            }
        }
    }
    ui.label(RichText::new("Ellerin").strong());
    ui.horizontal(|ui| {
        for slot in 0..4 {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(60.0, 60.0), egui::Sense::click());
            match ItemKind::ALL.get(hands[slot] as usize) {
                Some(&k) => crate::icons::draw_slot(ui.painter(), rect, k, w.has_seen(k)),
                None => {
                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(26, 21, 17));
                    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(70, 56, 40)), egui::StrokeKind::Inside);
                }
            }
            let held_name = ItemKind::ALL.get(hands[slot] as usize).map(|&k| crate::lang::item_word(w, k)).unwrap_or_else(|| "boş el".to_string());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("El {}: {held_name}", slot + 1)));
            if resp.clicked() {
                hands[slot] = 255;
            }
        }
        let any = hands.iter().any(|&i| i != 255);
        if ui.add_enabled(any, egui::Button::new("Dene").min_size(egui::vec2(70.0, 60.0))).clicked() {
            acts.push(UiAction::Cmd(CommandKind::Combine { items: *hands }));
            *hands = [255; 4];
        }
    });
    let mut want = [0u16; ItemKind::COUNT];
    for &i in hands.iter() {
        if let Some(k) = ItemKind::ALL.get(i as usize) {
            want[*k as usize] += 1;
        }
    }
    let known_match = defs.recipes.iter().find(|r| {
        r.station.is_none() && r.ticks == 0 && w.knows(r.discovery) && {
            let mut need = [0u16; ItemKind::COUNT];
            for i in &r.inputs {
                need[i.item as usize] += i.n;
            }
            need == want
        }
    });
    ui.label(RichText::new(match known_match {
        Some(r) => format!("Bunu biliyorsun: {}", r.name),
        None if hands.iter().all(|&i| i == 255) => "Çantandan bir şey seç, elinde birleştir.".to_string(),
        None => "Ne olacağını bilmiyorsun. Dene.".to_string(),
    }).small().weak());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for &k in ItemKind::ALL.iter() {
            let have = me.inventory.get(k);
            let free = have.saturating_sub(held(k, hands));
            if free == 0 {
                continue;
            }
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(46.0, 46.0), egui::Sense::click());
            crate::icons::draw_slot(ui.painter(), rect, k, w.has_seen(k));
            ui.painter().text(rect.right_bottom() - egui::vec2(3.0, 2.0), Align2::RIGHT_BOTTOM, free.to_string(), egui::FontId::proportional(22.0), Color32::WHITE);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Çantadan al: {}", crate::lang::item_word(w, k))));
            if resp.on_hover_text(crate::lang::item_word(w, k)).clicked() {
                if let Some(s) = hands.iter().position(|&i| i == 255) {
                    hands[s] = k as u8;
                }
            }
        }
    });
    ui.separator();
    ui.label(RichText::new("Defter (bildiklerin)").strong());
    let hand: Vec<(usize, &_)> = defs.recipes.iter().enumerate().filter(|(_, r)| r.station.is_none() && w.knows(r.discovery)).collect();
    if hand.is_empty() {
        ui.label("Defterin boş. Elinde bir şeyler dene, yaparak öğrenirsin.");
    }
    // Clues: everything a hand recipe needs has been seen but the recipe is not known yet. No name, no steps:
    // just a nudge to try those things together in the hands.
    // a farming tool only makes sense once there is something to farm: those clues wait until seed or grain has been seen
    let era_ready = |d: Discovery| match d {
        Discovery::Farming => w.has_seen(ItemKind::Seed),
        Discovery::Milling => w.has_seen(ItemKind::Grain),
        _ => true,
    };
    let clues: Vec<&_> = defs.recipes.iter().filter(|r| r.station.is_none() && r.ticks == 0 && !w.knows(r.discovery) && era_ready(r.discovery) && !r.inputs.is_empty() && r.inputs.iter().all(|c| w.has_seen(c.item))).collect();
    for r in clues {
        egui::Frame::new().fill(Color32::from_black_alpha(60)).inner_margin(8).corner_radius(0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for c in &r.inputs {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                    crate::icons::draw_slot(ui.painter(), rect, c.item, true);
                }
                let line = "Bunlar bir arada işe yarar gibi. Elinde dene.";
                ui.label(RichText::new(line).small().weak());
            });
        });
    }
    // the book as a wall of cards: what it makes, big; what it takes underneath; click a lit card to make it
    let card_w = 132.0;
    let cols = ((ui.available_width() + 6.0) / (card_w + 6.0)).floor().max(1.0) as usize;
    let mut sorted = hand;
    // what can be made right now first
    sorted.sort_by_key(|(_, r)| !(me.inventory.has(&r.inputs) && has_needs(&r.needs, &me.inventory)));
    egui::Grid::new("craft_cards").spacing([6.0, 6.0]).show(ui, |ui| {
        for (n, (i, r)) in sorted.into_iter().enumerate() {
            let can = me.inventory.has(&r.inputs) && has_needs(&r.needs, &me.inventory);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_w, 150.0), egui::Sense::click());
            let p = ui.painter();
            let hover = resp.hovered() && can;
            p.rect_filled(rect, 0.0, if hover { Color32::from_rgb(58, 46, 30) } else { Color32::from_rgb(34, 27, 20) });
            let edge = if can { Color32::from_rgb(214, 160, 70) } else { Color32::from_rgb(70, 56, 40) };
            p.rect_stroke(rect, 0.0, Stroke::new(if hover { 3.0 } else { 2.0 }, edge), egui::StrokeKind::Inside);
            // the product
            if let Some(o) = r.outputs.first() {
                let icon = egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 38.0), egui::vec2(56.0, 56.0));
                crate::icons::draw_slot(p, icon, o.item, true);
                if o.n > 1 {
                    p.text(icon.right_bottom() - egui::vec2(2.0, 1.0), Align2::RIGHT_BOTTOM, format!("x{}", o.n), egui::FontId::proportional(22.0), Color32::WHITE);
                }
            }
            let name_col = if can { Color32::from_rgb(244, 220, 160) } else { Color32::from_rgb(150, 136, 112) };
            let galley = p.layout(r.name.clone(), egui::FontId::proportional(22.0), name_col, card_w - 10.0);
            p.galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 72.0), galley, name_col);
            // what it takes: small icons, red when short
            let k = r.inputs.len().max(1) as f32;
            let iw = 26.0;
            let mut x = rect.center().x - (k * (iw + 4.0) - 4.0) / 2.0;
            for c in &r.inputs {
                let ir = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - 36.0), egui::vec2(iw, iw));
                crate::icons::draw_slot(p, ir, c.item, w.has_seen(c.item));
                let short = me.inventory.get(c.item) < c.n;
                p.text(ir.right_bottom() + egui::vec2(1.0, 3.0), Align2::RIGHT_BOTTOM, c.n.to_string(), egui::FontId::proportional(11.0), if short { Color32::from_rgb(240, 90, 70) } else { Color32::WHITE });
                x += iw + 4.0;
            }
            let mut tip = format!("{}\n", r.name);
            for c in &r.inputs {
                tip += &format!("{} {} (sende {})\n", c.n, crate::lang::item_word(w, c.item), me.inventory.get(c.item));
            }
            for k in &r.needs {
                tip += &format!("Gerekir: {}\n", crate::lang::item_word(w, *k));
            }
            if !r.steps.is_empty() {
                let names: Vec<&str> = r.steps.iter().map(|s| s.label.as_str()).collect();
                tip += &format!("Adımlar: {}", names.join(" → "));
            }
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, format!("Yap: {}", r.name)));
            if resp.on_hover_text(tip).clicked() && can {
                acts.push(UiAction::Cmd(CommandKind::Craft { recipe: RecipeId(i as u16) }));
            }
            if (n + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
}

fn tab_build(ui: &mut egui::Ui, w: &World, me: &alaz_sim::world::Agent, ui_ctx: &UiCtx, acts: &mut Vec<UiAction>) {
    let defs = rules();
    let known: Vec<_> = defs.buildings.iter().filter(|b| w.knows(b.discovery)).collect();
    if known.is_empty() {
        ui.label("Henüz bir şey inşa etmeyi bilmiyorsun.");
    }
    for b in known {
        egui::Frame::new().fill(Color32::from_black_alpha(90)).inner_margin(8).corner_radius(0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(b.name.as_str()).strong());
                    need_row(ui, w, &b.cost, &me.inventory);
                    needs_row(ui, w, &b.needs, &me.inventory);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let on = ui_ctx.build_mode == Some(b.kind);
                    if ui.add_enabled(me.inventory.has(&b.cost) && has_needs(&b.needs, &me.inventory), egui::Button::new("Yerleştir").selected(on).min_size(egui::vec2(80.0, 28.0))).clicked() {
                        acts.push(UiAction::BeginBuild(b.kind));
                    }
                });
            });
        });
    }
    ui.add_space(4.0);
    let snap = if ui_ctx.grid_snap { "açık" } else { "kapalı" };
    if ui.small_button(format!("Izgaraya oturt: {snap} [{}]", ui_ctx.binds.short(Action::Grid))).clicked() {
        acts.push(UiAction::ToggleGrid);
    }
    if let Some(kind) = ui_ctx.build_mode {
        ui.colored_label(Color32::from_rgb(140, 220, 140), format!("{}: sol tık yerleştirir, sağ tık iptal eder", defs.building(kind).name));
    }
}

fn tab_goals(ui: &mut egui::Ui, w: &World) {
    let known: Vec<Discovery> = DISCOVERIES.iter().copied().filter(|d| w.knows(*d)).collect();
    if w.perception_level() >= 1 && !known.is_empty() {
        ui.label(RichText::new(format!("Keşifler ({}/{})", known.len(), DISCOVERIES.len())).strong());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            for d in known {
                crate::ui_art::chip(ui, discovery_name(d), Color32::from_rgb(40, 52, 36), Color32::from_rgb(190, 214, 170));
            }
        });
        ui.separator();
    }
    let goals: Vec<Milestone> = Milestone::ALL.iter().copied().filter(|m| !is_age(*m)).collect();
    let done = goals.iter().filter(|m| w.milestone_done(**m)).count();
    crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Flag, &format!("{done}/{} hedef", goals.len()), done as f32 / goals.len().max(1) as f32, Color32::from_rgb(210, 170, 70), 220.0);
    ui.add_space(4.0);
    let mut shown_next = false;
    for m in goals {
        let ok = w.milestone_done(m);
        let next = !ok && !shown_next;
        shown_next |= next;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
        let p = ui.painter();
        if next {
            p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(150, 110, 50, 70));
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(190, 150, 70)), egui::StrokeKind::Inside);
        }
        let c = egui::pos2(rect.left() + 15.0, rect.center().y);
        if ok {
            p.circle_filled(c, 8.0, Color32::from_rgb(60, 140, 70));
            p.line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(-1.0, 3.5)], Stroke::new(2.0, Color32::WHITE));
            p.line_segment([c + egui::vec2(-1.0, 3.5), c + egui::vec2(4.5, -3.5)], Stroke::new(2.0, Color32::WHITE));
        } else {
            p.circle_stroke(c, 8.0, Stroke::new(1.6, if next { Color32::from_rgb(240, 200, 100) } else { Color32::from_rgb(100, 86, 66) }));
        }
        let col = if ok { Color32::from_rgb(140, 200, 140) } else if next { Color32::from_rgb(248, 226, 150) } else { Color32::from_rgb(170, 156, 134) };
        p.text(egui::pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, milestone_name(m), egui::FontId::proportional(22.0), col);
    }
    for age in [Milestone::NeolithicAge, Milestone::CopperAge, Milestone::BronzeAge, Milestone::IronAge, Milestone::SteelAge, Milestone::SteamAge, Milestone::ElectricAge, Milestone::ElectronicsAge, Milestone::ComputerAge] {
        if w.milestone_done(age) {
            ui.add_space(2.0);
            crate::ui_art::chip(ui, milestone_name(age), Color32::from_rgb(78, 60, 22), Color32::from_rgb(248, 218, 120));
        }
    }
}

fn tab_routes(ui: &mut egui::Ui, w: &World, ui_ctx: &UiCtx, acts: &mut Vec<UiAction>) {
    if ui_ctx.recording {
        ui.colored_label(Color32::from_rgb(240, 120, 120), format!("● Kayıt: {} adım", ui_ctx.recorded.len()));
        ui.horizontal(|ui| {
            if ui.button("Kaydı bitir").clicked() {
                acts.push(UiAction::ToggleRecording);
            }
            if ui.button("Bekle 5 sn").clicked() {
                acts.push(UiAction::RecordWait);
            }
        });
    } else {
        ui.horizontal(|ui| {
            if ui.button("Rota kaydet").clicked() {
                acts.push(UiAction::ToggleRecording);
            }
            if !ui_ctx.recorded.is_empty() && ui.small_button("Sil").clicked() {
                acts.push(UiAction::ClearRecorded);
            }
        });
    }
    for s in ui_ctx.recorded {
        ui.label(RichText::new(format!("- {}", step_text(s, w))).small());
    }
    if !ui_ctx.recording && ui_ctx.recorded.is_empty() {
        ui.label(RichText::new("Kayıt sırasında yaptıkların (topla, yükle, al) işçinin tekrarlayacağı rota olur. Sonra bir işçi seçip ata.").small().weak());
    }
}

/// The size of the whole interface (kept between runs in ui.cfg beside the saves).
pub fn ui_zoom_controls(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Arayüz boyutu").strong());
        let z = ui.ctx().zoom_factor();
        let mut set = None;
        if ui.button(" - ").clicked() {
            set = Some((z - 0.1).max(0.6));
        }
        if ui.button(format!("%{:.0}", z * 100.0)).on_hover_text("Sıfırla").clicked() {
            set = Some(1.0);
        }
        if ui.button(" + ").clicked() {
            set = Some((z + 0.1).min(2.0));
        }
        if let Some(z) = set {
            ui.ctx().set_zoom_factor(z);
            let _ = std::fs::create_dir_all(crate::saveio::base_dir());
            let _ = std::fs::write(crate::saveio::base_dir().join("ui.cfg"), format!("zoom={z:.2}\n"));
        }
    });
}

/// The interface size saved last time (1.0 when none).
pub fn saved_ui_zoom() -> f32 {
    std::fs::read_to_string(crate::saveio::base_dir().join("ui.cfg"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("zoom=").and_then(|v| v.trim().parse::<f32>().ok())))
        .unwrap_or(1.0)
        .clamp(0.6, 2.0)
}

fn tab_help(ui: &mut egui::Ui, binds: &Keybinds) {
    ui_zoom_controls(ui);
    ui.label(RichText::new("Ctrl + 0 da arayüzü normal boyuta döndürür.").small().weak());
    ui.add_space(6.0);
    ui.label(RichText::new("Fare").strong());
    ui.label("Sol tık: yürü, bina ya da işçi aç, yerleştir. Sağ tık: topla / evcilleştir / ava git / balık tut / inşayı iptal et");
    ui.label("Tekerlek: yakınlaş   ·   Orta tuş sürükle: kamerayı döndür ve eğ   ·   Shift + orta tuş: kamerayı kaydır");
    ui.add_space(6.0);
    ui.label(RichText::new("Klavye (Menü > Tuşlar'dan değiştirebilirsin)").strong());
    egui::Grid::new("help_keys").num_columns(2).spacing([20.0, 3.0]).show(ui, |ui| {
        for a in Action::ALL {
            ui.label(a.label());
            ui.label(RichText::new(binds.label(a)).monospace());
            ui.end_row();
        }
    });
}

fn panel_frame() -> egui::Frame {
    // the look of every panel follows the era: rough and earthy at first, plainer and cooler as the tribe learns
    let (fill, line, width, round) = match ERA.load(std::sync::atomic::Ordering::Relaxed) {
        0 | 1 => (Color32::from_rgba_unmultiplied(44, 32, 21, 238), Color32::from_rgb(128, 90, 52), 3.0, 3),
        2 | 3 => (Color32::from_rgba_unmultiplied(30, 24, 19, 238), Color32::from_rgb(104, 84, 56), 1.5, 10),
        _ => (Color32::from_rgba_unmultiplied(24, 27, 33, 240), Color32::from_rgb(92, 112, 136), 1.0, 6),
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(width, line))
        .inner_margin(12)
        .corner_radius(round)
        .shadow(egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(150) })
}

/// Warm, rounded look shared by every panel and menu.
/// The game's own pixel font, drawn to the same pixel grid as the world (see tools/make_pixel_font.py). Its pixel
/// is 1/11 of the font size: sizes 11, 22, 33 and 44 put each font pixel on whole screen pixels.
const PIXEL_FONT: &[u8] = include_bytes!("../assets/alaz-pixel.ttf");
static FONTS_SET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn set_pixel_font(ctx: &egui::Context) {
    if FONTS_SET.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("alaz-pixel".into(), std::sync::Arc::new(egui::FontData::from_static(PIXEL_FONT)));
    // first choice everywhere; the default fonts stay behind it for anything it does not draw
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().insert(0, "alaz-pixel".into());
    }
    ctx.set_fonts(fonts);
    // and the interface size chosen last time
    ctx.set_zoom_factor(saved_ui_zoom());
}

pub fn apply_style(ctx: &egui::Context) {
    set_pixel_font(ctx);
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(Color32::from_rgb(238, 228, 208));
    v.window_fill = Color32::from_rgb(30, 24, 19);
    v.panel_fill = Color32::from_rgb(30, 24, 19);
    v.selection.bg_fill = Color32::from_rgb(150, 110, 50);
    let r = egui::CornerRadius::ZERO;
    let edge = Stroke::new(2.0, Color32::from_rgb(20, 14, 10));
    for (w, fill) in [
        (&mut v.widgets.inactive, Color32::from_rgb(64, 52, 40)),
        (&mut v.widgets.hovered, Color32::from_rgb(98, 76, 50)),
        (&mut v.widgets.active, Color32::from_rgb(134, 104, 62)),
    ] {
        w.corner_radius = r;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = edge;
    }
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.open.corner_radius = r;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(11.0, 5.0);
        use egui::{FontFamily::Proportional, FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(22.0, Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(22.0, Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(22.0, Proportional));
        s.text_styles.insert(TextStyle::Heading, FontId::new(33.0, Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(22.0, egui::FontFamily::Monospace));
        s.visuals.window_shadow = egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(150) };
        s.visuals.popup_shadow = egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(150) };
        s.visuals.window_corner_radius = egui::CornerRadius::ZERO;
        s.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    });
}

pub fn draw(ctx: &egui::Context, w: &World, ui_ctx: &UiCtx, knap: &mut KnapState, acts: &mut Vec<UiAction>) {
    apply_style(ctx);
    ERA.store(w.perception_level(), std::sync::atomic::Ordering::Relaxed);
    let Some(me) = w.agents.get(ui_ctx.me.0 as usize) else { return };

    draw_status(ctx, w, me, ui_ctx);
    draw_body(ctx, w, me);
    draw_needs(ctx, w, me);
    // while the hands are busy with a workpiece, nothing else may talk over it
    if w.craft_view(ui_ctx.me).is_none() {
        draw_goal_banner(ctx, w, me);
        draw_hints(ctx, w, ui_ctx, ui_ctx.tab.is_some());
    }
    draw_action_bar(ctx, w, me, ui_ctx, acts);
    // while a workpiece is on the bench the bench panel steps aside instead of lying under it
    if let Some(tab) = ui_ctx.tab.filter(|_| w.craft_view(ui_ctx.me).is_none()) {
        egui::Area::new(egui::Id::NULL.with("tab")).anchor(Align2::CENTER_BOTTOM, [0.0, -70.0]).show(ctx, |ui| {
            panel_frame().show(ui, |ui| {
                ui.set_width(600.0_f32.min(ctx.content_rect().width() - 40.0));
                // leave room for the action bar below and the status line above, whatever the window size
                let max_h = (ctx.content_rect().height() - 62.0 - 130.0).clamp(120.0, 520.0);
                ui.horizontal(|ui| {
                    let (title, key) = match tab {
                        Tab::Bag => ("Çanta", Action::Bag),
                        Tab::Craft => ("Yapabildiklerin", Action::Craft),
                        Tab::Build => ("İnşa", Action::Build),
                        Tab::Goals => ("Hedefler", Action::Goals),
                        Tab::Routes => ("Görev rotası", Action::Routes),
                        Tab::Help => ("Yardım", Action::Help),
                    };
                    let (g, gc) = match tab {
                        Tab::Bag => (crate::ui_art::Glyph::Bag, Color32::from_rgb(214, 170, 100)),
                        Tab::Craft => (crate::ui_art::Glyph::Hammer, Color32::from_rgb(200, 200, 210)),
                        Tab::Build => (crate::ui_art::Glyph::House, Color32::from_rgb(210, 140, 90)),
                        Tab::Goals => (crate::ui_art::Glyph::Flag, Color32::from_rgb(240, 190, 80)),
                        Tab::Routes => (crate::ui_art::Glyph::Route, Color32::from_rgb(130, 200, 150)),
                        Tab::Help => (crate::ui_art::Glyph::Help, Color32::from_rgb(160, 190, 240)),
                    };
                    crate::ui_art::heading(ui, g, title, gc);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(format!("Kapat [{}]", ui_ctx.binds.short(key))).clicked() {
                            acts.push(UiAction::ToggleTab(tab));
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| match tab {
                    Tab::Bag => tab_bag(ui, w, me, acts),
                    Tab::Craft => tab_craft(ui, w, me, &mut knap.hands, acts),
                    Tab::Build => tab_build(ui, w, me, ui_ctx, acts),
                    Tab::Goals => tab_goals(ui, w),
                    Tab::Routes => tab_routes(ui, w, ui_ctx, acts),
                    Tab::Help => tab_help(ui, ui_ctx.binds),
                });
            });
        });
    }

    if let Some(n) = ui_ctx.anchor {
        let r = ctx.content_rect();
        let p = egui::pos2(r.left() + (n[0] + 1.0) * 0.5 * r.width(), r.top() + (1.0 - n[1]) * 0.5 * r.height());
        ctx.data_mut(|d| d.insert_temp(egui::Id::NULL.with("craft_anchor"), p));
    }
    draw_cave(ctx, w, ui_ctx.me, acts);
    match w.craft_view(ui_ctx.me) {
        Some(v) => match v.kind {
            alaz_sim::MinigameKind::Knap => draw_knapping(ctx, &v, ui_ctx.last_strike, knap, acts),
            alaz_sim::MinigameKind::Fit => draw_fit(ctx, &v, ui_ctx.last_strike, acts),
            alaz_sim::MinigameKind::Tension => draw_tension(ctx, &v, ui_ctx.last_strike, knap, acts),
            alaz_sim::MinigameKind::Timing => draw_craft_gauge(ctx, &v, ui_ctx.last_strike, ui_ctx.binds, acts),
            alaz_sim::MinigameKind::Stack => draw_stack(ctx, &v, ui_ctx.last_strike, acts),
        },
        None => knap.reset(),
    }

    // the workpiece needs the screen: side panels wait until the job is done
    let in_cave = w.agents.iter().any(|a| a.id == ui_ctx.me && matches!(a.task, alaz_sim::world::Task::Cave { .. }));
    let selection = if w.craft_view(ui_ctx.me).is_some() || in_cave { None } else { ui_ctx.selection };
    match selection {
        Some(Selection::Building(i)) => {
            if let Some(b) = w.buildings.get(i) {
                draw_building(ctx, w, i, b, me, acts);
            }
        }
        Some(Selection::Agent(i)) => {
            if let Some(a) = w.agents.get(i) {
                draw_worker(ctx, w, i, a, me, ui_ctx, acts);
            }
        }
        None => {}
    }
}

/// A minigame sits just above the worker's head, in the world, clamped to the screen.
fn craft_area(ctx: &egui::Context, name: &str) -> egui::Area {
    let screen = ctx.content_rect();
    let p = ctx.data(|d| d.get_temp::<egui::Pos2>(egui::Id::NULL.with("craft_anchor"))).unwrap_or(screen.center());
    // beside the worker, never over them: to the right if there is room, otherwise to the left
    const PANEL_W: f32 = CRAFT_PANEL_W;
    let y = (p.y + 60.0).clamp(screen.top() + 20.0, (screen.bottom() - 20.0).max(screen.top() + 20.0));
    // never over the status column on the left or the action bar at the bottom, whatever the window size
    let left = if screen.width() >= 700.0 { screen.left() + 226.0 } else { screen.left() + 8.0 };
    let keep = egui::Rect::from_min_max(egui::pos2(left, screen.top() + 8.0), egui::pos2(screen.right() - 8.0, (screen.bottom() - 70.0).max(screen.top() + 120.0)));
    if p.x + 70.0 + PANEL_W < screen.right() {
        egui::Area::new(egui::Id::NULL.with(name)).pivot(Align2::LEFT_CENTER).fixed_pos(egui::pos2(p.x + 70.0, y)).constrain_to(keep)
    } else {
        egui::Area::new(egui::Id::NULL.with(name)).pivot(Align2::RIGHT_CENTER).fixed_pos(egui::pos2(p.x - 70.0, y)).constrain_to(keep)
    }
}

/// Local state of the knapping minigame UI (input only; the sim decides what a strike is worth).
pub struct KnapState {
    holding: bool,
    /// 0..=1 swing force while the button is held; it rises and falls so it has to be timed.
    charge: f32,
    rising: bool,
    /// Where strikes landed on the rim so far: angle in degrees and whether it was a good one.
    notches: Vec<(f32, bool)>,
    seen_done: u8,
    /// Item indexes held in the hands, 255 = empty (what the player is trying to combine).
    pub hands: [u8; 4],
}

impl Default for KnapState {
    fn default() -> Self {
        Self { holding: false, charge: 0.0, rising: false, notches: Vec::new(), seen_done: 0, hands: [255; 4] }
    }
}

impl KnapState {
    fn reset(&mut self) {
        self.holding = false;
        self.charge = 0.0;
        self.notches.clear();
        self.seen_done = 0;
    }
}

/// Angle on the stone's rim, in degrees, 0 = up and clockwise, as the sim expects.
/// Sizes of the minigame work areas, kept modest so the worker and the world stay in view.
/// Widest a minigame panel gets; longer text wraps.
const CRAFT_PANEL_W: f32 = 390.0;
const KNAP_SIZE: f32 = 230.0;
const FIT_SIZE: f32 = 220.0;

fn rim_point(center: egui::Pos2, radius: f32, deg: f32) -> egui::Pos2 {
    let r = deg.to_radians();
    egui::pos2(center.x + radius * r.sin(), center.y - radius * r.cos())
}

/// Knapping: hold the stone, aim the hammerstone at its rim with the mouse, hold the button to swing
/// and release to strike. The green arc is where a flake will come off; the bar on the right is the force.
fn draw_knapping(ctx: &egui::Context, v: &alaz_sim::CraftView, last: Option<(bool, f32)>, st: &mut KnapState, acts: &mut Vec<UiAction>) {
    let def = &rules().recipes[v.recipe];
    if v.done < st.seen_done {
        st.notches.clear();
    }
    st.seen_done = v.done;
    craft_area(ctx, "knap").show(ctx, |ui| {
        ui.set_max_width(CRAFT_PANEL_W);
        panel_frame().show(ui, |ui| {
            step_header(ui, def, v);
            ui.horizontal(|ui| {
                let (resp, painter) = ui.allocate_painter(egui::vec2(KNAP_SIZE, KNAP_SIZE), egui::Sense::click_and_drag());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Yontma taşı"));
                let rect = resp.rect;
                let c = rect.center();
                let r0 = KNAP_SIZE * 0.369;
                painter.rect_filled(rect, 0.0, Color32::from_rgb(52, 46, 40));
                // the stone: an uneven disc, bitten by every strike
                let n = 48;
                let pts: Vec<egui::Pos2> = (0..n)
                    .map(|i| {
                        let deg = i as f32 * 360.0 / n as f32;
                        let bump = 1.0 + 0.045 * ((i * 7 % 5) as f32 - 2.0) / 2.0;
                        // what the stone has really lost: the removed depth of the two nearest sectors, blended
                        let f = (deg - 22.5).rem_euclid(360.0) / 45.0;
                        let (i0, t) = (f.floor() as usize % 8, f.fract());
                        let t = t * t * (3.0 - 2.0 * t);
                        let lost = v.stack[i0].1 as f32 * (1.0 - t) + v.stack[(i0 + 1) % 8].1 as f32 * t;
                        let r = r0 * bump * (1.0 - 0.009 * lost).max(0.3);
                        rim_point(c, r, deg)
                    })
                    .collect();
                let mut mesh = egui::epaint::Mesh::default();
                mesh.colored_vertex(c, Color32::from_rgb(138, 134, 128));
                for q in &pts {
                    mesh.colored_vertex(*q, Color32::from_rgb(138, 134, 128));
                }
                for i in 0..pts.len() as u32 {
                    mesh.add_triangle(0, 1 + i, 1 + (i + 1) % pts.len() as u32);
                }
                painter.add(egui::Shape::mesh(mesh));
                painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(3.0, Color32::from_rgb(70, 68, 64))));
                for &(a, good) in &st.notches {
                    let (p0, p1) = (rim_point(c, r0 * 0.78, a), rim_point(c, r0 * 0.98, a));
                    painter.line_segment([p0, p1], Stroke::new(if good { 2.0 } else { 1.0 }, Color32::from_rgb(60, 58, 54)));
                    if good {
                        painter.circle_filled(rim_point(c, r0 * 0.7, a), 7.0, Color32::from_rgb(214, 210, 200));
                    }
                }
                // per sector: what is still to be taken off (a strike deeper than this plus 8 ruins the sector)
                for i in 0..8 {
                    let (need, gone) = (v.stack[i].0.max(0) as i32, v.stack[i].1 as i32);
                    let deg = 22.5 + 45.0 * i as f32;
                    let (txt, col) = if gone >= need {
                        ("tamam".to_string(), Color32::from_rgb(110, 240, 130))
                    } else {
                        (format!("{}", need - gone), Color32::from_rgb(240, 200, 110))
                    };
                    painter.text(rim_point(c, r0 + 34.0, deg), Align2::CENTER_CENTER, txt, egui::FontId::proportional(11.0), col);
                }
                // where a flake will come off
                let zone: Vec<egui::Pos2> = (0..=24)
                    .map(|i| rim_point(c, r0 + 9.0, v.center as f32 - v.half as f32 + 2.0 * v.half as f32 * i as f32 / 24.0))
                    .collect();
                let zc = match last {
                    Some((true, _)) => Color32::from_rgb(110, 240, 130),
                    Some((false, _)) => Color32::from_rgb(240, 100, 100),
                    None => Color32::from_rgba_unmultiplied(100, 210, 110, 230),
                };
                painter.add(egui::Shape::line(zone, Stroke::new(9.0, zc)));
                // aim: the hammerstone follows the mouse; the strike lands where it points at the rim
                let mouse = ctx.input(|i| i.pointer.hover_pos());
                let mut aim = None;
                if let Some(m) = mouse.filter(|m| rect.contains(*m)) {
                    let (dx, dy) = (m.x - c.x, m.y - c.y);
                    if dx.abs() + dy.abs() > 6.0 {
                        let deg = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
                        aim = Some(deg);
                        painter.line_segment([c, rim_point(c, r0, deg)], Stroke::new(1.0, Color32::from_white_alpha(60)));
                        let tip = rim_point(c, r0 + 4.0, deg);
                        painter.circle_filled(m, 16.0, Color32::from_rgb(96, 92, 88));
                        painter.circle_stroke(m, 16.0, Stroke::new(2.0, Color32::from_rgb(40, 40, 40)));
                        painter.circle_filled(tip, 5.0, Color32::from_rgb(255, 230, 120));
                    }
                }
                // swing force: hold to charge, it swings up and back
                let dt = ctx.input(|i| i.stable_dt).min(0.05);
                if resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down()) && aim.is_some() {
                    if !st.holding {
                        st.holding = true;
                        st.charge = 0.0;
                        st.rising = true;
                    }
                    let speed = 1.1;
                    st.charge += if st.rising { speed * dt } else { -speed * dt };
                    if st.charge >= 1.0 {
                        st.charge = 1.0;
                        st.rising = false;
                    } else if st.charge <= 0.0 {
                        st.charge = 0.0;
                        st.rising = true;
                    }
                    ctx.request_repaint();
                }
                // released off the stone: the swing is called off. (It used to land dead on the target angle,
                // so dragging out of the panel before letting go was a free perfect strike.)
                if st.holding && aim.is_none() && ctx.input(|i| i.pointer.primary_released()) {
                    st.holding = false;
                    st.charge = 0.0;
                }
                if let Some(deg) = aim.filter(|_| st.holding && ctx.input(|i| i.pointer.primary_released())) {
                    st.holding = false;
                    let power = (st.charge * 100.0).round() as u8;
                    let (da, dp) = (((deg - v.center as f32 + 540.0) % 360.0 - 180.0).abs(), (power as i32 - v.power as i32).unsigned_abs());
                    st.notches.push((deg, da <= v.half as f32 && dp <= v.power_half));
                    acts.push(UiAction::Cmd(CommandKind::StrikeAt { angle: deg.round() as u16 % 360, power }));
                    st.charge = 0.0;
                }
                // force bar
                let bar = ui.allocate_painter(egui::vec2(40.0, KNAP_SIZE), egui::Sense::hover());
                let br = bar.0.rect;
                let bp = bar.1;
                bp.rect_filled(br, 0.0, Color32::from_gray(35));
                let y_of = |p: f32| br.bottom() - br.height() * (p / 100.0).clamp(0.0, 1.0);
                let band = egui::Rect::from_min_max(
                    egui::pos2(br.left(), y_of(v.power as f32 + v.power_half as f32)),
                    egui::pos2(br.right(), y_of(v.power as f32 - v.power_half as f32)),
                );
                bp.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(100, 210, 110, 210));
                let fy = y_of(st.charge * 100.0);
                bp.rect_filled(egui::Rect::from_min_max(egui::pos2(br.left() - 3.0, fy - 2.0), egui::pos2(br.right() + 3.0, fy + 2.0)), 0.0, Color32::WHITE);
                bp.text(egui::pos2(br.center().x, br.top() + 8.0), Align2::CENTER_CENTER, "güç", egui::FontId::proportional(11.0), Color32::from_gray(160));
            });
            let base = def.steps[v.step as usize].window;
            if v.half > base {
                ui.label(RichText::new(format!("Elindeki aletler hedefi büyütüyor (+{}°)", v.half - base)).small().color(Color32::from_rgb(140, 220, 140)));
            } else {
                ui.label(RichText::new("Elinde yardımcı alet yok: hedef dar. Ağır bir alet taşı daha rahat yontar.").small().weak());
            }
            ui.label(RichText::new("Yürürsen vazgeçersin.").small().weak());
        });
    });
}

/// The timing game: a marker sweeps across the bar, hit the green zone with Space or the button.
fn draw_craft_gauge(ctx: &egui::Context, v: &alaz_sim::CraftView, last: Option<(bool, f32)>, binds: &Keybinds, acts: &mut Vec<UiAction>) {
    let def = &rules().recipes[v.recipe];
    craft_area(ctx, "craft").show(ctx, |ui| {
        ui.set_max_width(CRAFT_PANEL_W);
        panel_frame().show(ui, |ui| {
            ui.set_min_width(380.0);
            step_header(ui, def, v);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(380.0, 96.0), egui::Sense::hover());
            // a miss jolts the piece; a hit throws chips off it
            let left = last.map_or(0.0, |(_, t)| t.max(0.0) / 0.35);
            let shake = match last {
                Some((false, _)) => ((1.0 - left) * 40.0).sin() * 7.0 * left,
                _ => 0.0,
            };
            let rect = rect.translate(egui::vec2(shake, 0.0));
            let p = ui.painter();
            p.rect_filled(rect, 0.0, Color32::from_rgb(52, 42, 33));
            let x_of = |t: f32| rect.left() + 14.0 + (rect.width() - 28.0) * (t / v.span as f32).clamp(0.0, 1.0);
            // the piece of wood being worked, with bark and knots
            let cy = rect.center().y + 14.0;
            let body = egui::Rect::from_min_max(egui::pos2(rect.left() + 10.0, cy - 15.0), egui::pos2(rect.right() - 10.0, cy + 15.0));
            p.rect_filled(body, 0.0, Color32::from_rgb(118, 84, 52));
            for k in 0..9 {
                let x = body.left() + 18.0 + k as f32 * 40.0;
                p.line_segment([egui::pos2(x, cy - 10.0), egui::pos2(x + 14.0, cy + 6.0)], Stroke::new(1.5, Color32::from_rgb(84, 58, 36)));
            }
            let zone = egui::Rect::from_min_max(
                egui::pos2(x_of(v.center as f32 - v.half as f32), cy - 17.0),
                egui::pos2(x_of(v.center as f32 + v.half as f32), cy + 17.0),
            );
            let zone_color = match last {
                Some((true, _)) => Color32::from_rgb(110, 240, 130),
                Some((false, _)) => Color32::from_rgb(240, 100, 100),
                None => Color32::from_rgb(108, 214, 118),
            };
            p.rect_filled(zone, 0.0, Color32::from_rgba_unmultiplied(zone_color.r(), zone_color.g(), zone_color.b(), 70));
            p.rect_stroke(zone, 0.0, Stroke::new(2.5, zone_color), egui::StrokeKind::Middle);
            // the heart of the zone: dead centre is the clean blow
            let core = (v.half as f32 * 0.3).max(1.0);
            let heart = egui::Rect::from_min_max(egui::pos2(x_of(v.center as f32 - core), cy - 17.0), egui::pos2(x_of(v.center as f32 + core), cy + 17.0));
            p.rect_filled(heart, 0.0, Color32::from_rgba_unmultiplied(250, 230, 120, 90));
            // a marker close to the zone glows, so the moment can be felt coming
            let near = 1.0 - ((v.pos as f32 - v.center as f32).abs() / (v.half as f32 * 3.0)).clamp(0.0, 1.0);
            if let Some((true, _)) = last {
                // chips fly up and out from where the blow landed
                let age = 1.0 - left;
                let hx = x_of(v.center as f32);
                for k in 0..10 {
                    let a = k as f32 * 0.63 + 0.3;
                    let (dx, dy) = (a.cos() * 70.0 * age, -a.sin().abs() * 60.0 * age + 80.0 * age * age);
                    p.rect_filled(egui::Rect::from_center_size(egui::pos2(hx + dx, cy - 6.0 + dy), egui::vec2(4.0, 4.0)), 0.0, Color32::from_rgba_unmultiplied(214, 168, 110, (left * 255.0) as u8));
                }
                p.text(egui::pos2(hx, rect.top() + 4.0), Align2::CENTER_TOP, "İsabet!", egui::FontId::proportional(22.0), Color32::from_rgba_unmultiplied(150, 250, 150, (left * 255.0) as u8));
            } else if let Some((false, _)) = last {
                p.text(egui::pos2(rect.center().x, rect.top() + 4.0), Align2::CENTER_TOP, "Iska", egui::FontId::proportional(22.0), Color32::from_rgba_unmultiplied(250, 120, 110, (left * 255.0) as u8));
            }
            // the scraper moves along it; strike when it is over the marked spot
            let mx = x_of(v.pos as f32);
            if near > 0.0 {
                p.circle_filled(egui::pos2(mx, cy - 26.0), 10.0 + 8.0 * near, Color32::from_rgba_unmultiplied(255, 220, 120, (near * 60.0) as u8));
            }
            p.add(egui::Shape::convex_polygon(
                vec![egui::pos2(mx - 9.0, cy - 40.0), egui::pos2(mx + 9.0, cy - 40.0), egui::pos2(mx + 3.0, cy - 12.0), egui::pos2(mx - 3.0, cy - 12.0)],
                Color32::from_rgb(190, 192, 200),
                Stroke::new(1.5, Color32::from_rgb(60, 60, 68)),
            ));
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(mx - 5.0, cy - 58.0), egui::pos2(mx + 5.0, cy - 38.0)), 0.0, Color32::from_rgb(96, 66, 40));
            let base = def.steps[v.step as usize].window;
            if v.half > base {
                ui.label(RichText::new(format!("Aletlerin hedefi büyütüyor (+{})", v.half - base)).small().color(Color32::from_rgb(140, 220, 140)));
            } else {
                ui.label(RichText::new("Elinde yardımcı alet yok: hedef dar.").small().weak());
            }
            ui.horizontal(|ui| {
                // the key shown is the one actually bound, not always Space
                if ui.button(RichText::new(format!("Vur!  ({})", binds.short(Action::Primary))).size(22.0)).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Strike));
                }
                ui.label(RichText::new("Yürürsen vazgeçersin, malzeme gitmez.").small().weak());
            });
        });
    });
}

/// Title, the row of steps (done, current, to come) and what to do in this one.
fn step_header(ui: &mut egui::Ui, def: &alaz_sim::rules::RecipeDef, v: &alaz_sim::CraftView) {
    ui.label(RichText::new(def.name.as_str()).strong().size(22.0));
    ui.horizontal_wrapped(|ui| {
        for (i, s) in def.steps.iter().enumerate() {
            let n = i + 1;
            if (i as u8) < v.step {
                ui.label(RichText::new(format!("{n}. {}", s.label)).small().color(Color32::from_rgb(130, 215, 130)).strikethrough());
            } else if i as u8 == v.step {
                ui.label(RichText::new(format!("{n}. {}", s.label)).strong().color(Color32::from_rgb(245, 215, 100)));
            } else {
                ui.label(RichText::new(format!("{n}. {}", s.label)).small().color(Color32::from_gray(140)));
            }
        }
    });
    let hint = match v.kind {
        alaz_sim::MinigameKind::Timing => "Çubuktaki işaretçi yeşil bölgedeyken vur.",
        alaz_sim::MinigameKind::Knap => "Kenardaki sayı o yüzeyden alınacak kalan kalınlık. Nişan al, basılı tutup gücü ayarla, bırak; sayıyı 8 aşan vuruş taşı bozar.",
        alaz_sim::MinigameKind::Fit => "Parçayı yeşil halkanın içine tıklayarak yerleştir.",
        alaz_sim::MinigameKind::Tension => "Basılı tut, güç yeşil banda gelince bırak.",
        alaz_sim::MinigameKind::Stack => "Odunu yığının üstüne tıklayarak bırak; ağırlık merkezi altındakinin üstünde kalmalı.",
    };
    ui.label(RichText::new(format!("{}  ({}/{})", hint, (v.done + 1).min(v.strikes), v.strikes)).small());
    // how the strikes of this step went so far: hit, miss, still to come
    ui.horizontal(|ui| {
        for i in 0..v.strikes {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            if i < v.hits {
                ui.painter().circle_filled(rect.center(), 6.0, Color32::from_rgb(110, 230, 120));
            } else if i < v.done {
                ui.painter().circle_filled(rect.center(), 6.0, Color32::from_rgb(230, 100, 100));
            } else {
                ui.painter().circle_stroke(rect.center(), 6.0, Stroke::new(1.5, Color32::from_gray(150)));
            }
        }
    });
    ui.add_space(4.0);
}

/// Stacking: the pile grows upward from the ground bar; the next log hangs under the pointer, click to drop it.
fn draw_stack(ctx: &egui::Context, v: &alaz_sim::CraftView, last: Option<(bool, f32)>, acts: &mut Vec<UiAction>) {
    let def = &rules().recipes[v.recipe];
    craft_area(ctx, "stack").show(ctx, |ui| {
        ui.set_max_width(CRAFT_PANEL_W);
        panel_frame().show(ui, |ui| {
            ui.set_min_width(340.0);
            step_header(ui, def, v);
            let (w, h) = (330.0, 190.0);
            let (resp, painter) = ui.allocate_painter(egui::vec2(w, h), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Yığın alanı"));
            let rect = resp.rect;
            painter.rect_filled(rect, 0.0, Color32::from_rgb(52, 42, 33));
            let k = w / 210.0;
            let cx = rect.center().x;
            let base = rect.bottom() - 18.0;
            let bar_h = 14.0;
            // the ground it stands on
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(cx - v.half as f32 * k, base), egui::pos2(cx + v.half as f32 * k, base + 8.0)), 0.0,
                Color32::from_rgb(110, 96, 70),
            );
            let log = |painter: &egui::Painter, c: f32, wd: f32, row: usize, fill: Color32| {
                let top = base - (row as f32 + 1.0) * bar_h;
                let r = egui::Rect::from_min_max(egui::pos2(cx + (c - wd / 2.0) * k, top), egui::pos2(cx + (c + wd / 2.0) * k, top + bar_h - 1.0));
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::from_rgb(70, 48, 28)), egui::StrokeKind::Inside);
                painter.circle_filled(egui::pos2(r.right() - 5.0, r.center().y), 3.5, Color32::from_rgb(190, 150, 100));
            };
            for (i, &(c, wd)) in v.stack[..v.stack_n as usize].iter().enumerate() {
                log(&painter, c as f32, wd as f32, i, Color32::from_rgb(122, 86, 52));
            }
            if let Some(p) = resp.hover_pos() {
                let x = ((p.x - cx) / k).clamp(-100.0, 100.0);
                let tint = match last {
                    Some((false, _)) => Color32::from_rgba_unmultiplied(230, 120, 100, 170),
                    _ => Color32::from_rgba_unmultiplied(170, 130, 90, 170),
                };
                log(&painter, x, v.span as f32, v.stack_n as usize, tint);
                // a plumb line from the pointer to the ground
                painter.line_segment([egui::pos2(p.x, base - (v.stack_n as f32 + 1.0) * bar_h), egui::pos2(p.x, base)], Stroke::new(1.0, Color32::from_white_alpha(60)));
                if resp.clicked() {
                    acts.push(UiAction::Cmd(CommandKind::PlaceAt { x: x.round() as i16, y: 0 }));
                }
            }
        });
    });
}

/// Fitting a part into place: the green ring is where it belongs, click on it.
fn draw_fit(ctx: &egui::Context, v: &alaz_sim::CraftView, last: Option<(bool, f32)>, acts: &mut Vec<UiAction>) {
    let def = &rules().recipes[v.recipe];
    craft_area(ctx, "fit").show(ctx, |ui| {
        ui.set_max_width(CRAFT_PANEL_W);
        panel_frame().show(ui, |ui| {
            ui.set_min_width(300.0);
            step_header(ui, def, v);
            let (resp, painter) = ui.allocate_painter(egui::vec2(FIT_SIZE, FIT_SIZE), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Yerleştirme alanı"));
            let rect = resp.rect;
            let c = rect.center();
            let k = FIT_SIZE / 290.0 * 1.35;
            let to_px = |x: f32, y: f32| egui::pos2(c.x + x * k, c.y + y * k);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(52, 42, 33));
            // the handle lies across the bench with a notch cut where the stone must sit
            let hy = to_px(0.0, v.ty as f32).y;
            let handle = egui::Rect::from_min_max(egui::pos2(rect.left() + 10.0, hy - 13.0), egui::pos2(rect.right() - 10.0, hy + 13.0));
            painter.rect_filled(handle, 0.0, Color32::from_rgb(120, 86, 52));
            for k in 0..6 {
                let x = handle.left() + 20.0 + k as f32 * 46.0;
                painter.line_segment([egui::pos2(x, hy - 7.0), egui::pos2(x + 16.0, hy + 5.0)], Stroke::new(1.4, Color32::from_rgb(86, 60, 38)));
            }
            let tp0 = to_px(v.tx as f32, v.ty as f32);
            painter.add(egui::Shape::convex_polygon(
                vec![egui::pos2(tp0.x - 15.0, hy - 13.0), egui::pos2(tp0.x + 15.0, hy - 13.0), egui::pos2(tp0.x, hy + 9.0)],
                Color32::from_rgb(40, 30, 22),
                Stroke::NONE,
            ));
            let ring = match last {
                Some((true, _)) => Color32::from_rgb(110, 240, 130),
                Some((false, _)) => Color32::from_rgb(240, 100, 100),
                None => Color32::from_rgb(100, 210, 110),
            };
            let tp = to_px(v.tx as f32, v.ty as f32);
            painter.circle_stroke(tp, v.half as f32 * k, Stroke::new(3.0, ring));
            painter.circle_filled(tp, 3.0, ring);
            let mouse = ctx.input(|i| i.pointer.hover_pos()).filter(|m| rect.contains(*m));
            if let Some(m) = mouse {
                // the stone head you are carrying
                painter.add(egui::Shape::convex_polygon(
                    vec![egui::pos2(m.x - 17.0, m.y + 6.0), egui::pos2(m.x - 9.0, m.y - 12.0), egui::pos2(m.x + 8.0, m.y - 14.0), egui::pos2(m.x + 18.0, m.y + 2.0), egui::pos2(m.x + 4.0, m.y + 14.0)],
                    Color32::from_rgb(158, 158, 168),
                    Stroke::new(2.0, Color32::from_rgb(64, 64, 72)),
                ));
                if resp.clicked() {
                    let (x, y) = (((m.x - c.x) / k).round() as i32, ((m.y - c.y) / k).round() as i32);
                    acts.push(UiAction::Cmd(CommandKind::PlaceAt { x: x.clamp(-100, 100) as i16, y: y.clamp(-100, 100) as i16 }));
                }
            }
            let base = def.steps[v.step as usize].window;
            if v.half > base {
                ui.label(RichText::new(format!("Kıskaç parçayı tutuyor: halka daha geniş (+{})", v.half - base)).small().color(Color32::from_rgb(140, 220, 140)));
            } else {
                ui.label(RichText::new("Parçayı elinle tutuyorsun, halka dar. Bir kıskaç işi kolaylaştırır.").small().weak());
            }
        });
    });
}

/// Lashing and pressing: hold to pull, let go when the pressure is in the green band.
fn draw_tension(ctx: &egui::Context, v: &alaz_sim::CraftView, last: Option<(bool, f32)>, st: &mut KnapState, acts: &mut Vec<UiAction>) {
    let def = &rules().recipes[v.recipe];
    craft_area(ctx, "tension").show(ctx, |ui| {
        ui.set_max_width(CRAFT_PANEL_W);
        panel_frame().show(ui, |ui| {
            ui.set_min_width(340.0);
            step_header(ui, def, v);
            let (resp, painter) = ui.allocate_painter(egui::vec2(330.0, 110.0), egui::Sense::click_and_drag());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Sıkıştırma alanı"));
            let rect = resp.rect;
            painter.rect_filled(rect, 0.0, Color32::from_rgb(52, 42, 33));
            // the handle with the stone set in its notch, and the fibre wound around both
            let mid = rect.center();
            painter.rect_filled(egui::Rect::from_center_size(mid, egui::vec2(250.0, 24.0)), 0.0, Color32::from_rgb(120, 86, 52));
            painter.add(egui::Shape::convex_polygon(
                vec![egui::pos2(mid.x + 30.0, mid.y - 8.0), egui::pos2(mid.x + 76.0, mid.y - 30.0), egui::pos2(mid.x + 84.0, mid.y + 28.0), egui::pos2(mid.x + 30.0, mid.y + 10.0)],
                Color32::from_rgb(158, 158, 168),
                Stroke::new(2.0, Color32::from_rgb(64, 64, 72)),
            ));
            let turns = 2 + (st.charge * 10.0).round() as i32;
            for i in 0..turns {
                let x = mid.x + 16.0 + i as f32 * 3.6;
                painter.line_segment([egui::pos2(x, mid.y - 20.0), egui::pos2(x + 5.0, mid.y + 22.0)], Stroke::new(2.4, Color32::from_rgb(194, 206, 120)));
            }
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            if resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_down()) {
                if !st.holding {
                    st.holding = true;
                    st.charge = 0.0;
                    st.rising = true;
                }
                st.charge += if st.rising { 0.9 * dt } else { -0.9 * dt };
                if st.charge >= 1.0 {
                    st.charge = 1.0;
                    st.rising = false;
                } else if st.charge <= 0.0 {
                    st.charge = 0.0;
                    st.rising = true;
                }
                ctx.request_repaint();
            }
            if st.holding && ctx.input(|i| i.pointer.primary_released()) {
                st.holding = false;
                acts.push(UiAction::Cmd(CommandKind::Tighten { power: (st.charge * 100.0).round() as u8 }));
                st.charge = 0.0;
            }
            // pressure bar with the band to hit
            let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(330.0, 26.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(bar_rect, 0.0, Color32::from_gray(35));
            let x_of = |q: f32| bar_rect.left() + bar_rect.width() * (q / 100.0).clamp(0.0, 1.0);
            let band = egui::Rect::from_min_max(
                egui::pos2(x_of(v.power as f32 - v.power_half as f32), bar_rect.top()),
                egui::pos2(x_of(v.power as f32 + v.power_half as f32), bar_rect.bottom()),
            );
            let col = match last {
                Some((true, _)) => Color32::from_rgb(90, 230, 110),
                Some((false, _)) => Color32::from_rgb(230, 90, 90),
                None => Color32::from_rgba_unmultiplied(90, 190, 100, 210),
            };
            p.rect_filled(band, 0.0, col);
            let fx = x_of(st.charge * 100.0);
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(fx - 2.0, bar_rect.top() - 3.0), egui::pos2(fx + 2.0, bar_rect.bottom() + 3.0)), 0.0, Color32::WHITE);
            let base = def.steps[v.step as usize].window;
            if v.half > base {
                ui.label(RichText::new(format!("Kıskaç yardım ediyor: bant daha geniş (+{})", v.half - base)).small().color(Color32::from_rgb(140, 220, 140)));
            } else {
                ui.label(RichText::new("Elle sıkıyorsun, bant dar.").small().weak());
            }
        });
    });
}

fn draw_building(
    ctx: &egui::Context,
    w: &World,
    i: usize,
    b: &alaz_sim::Building,
    me: &alaz_sim::world::Agent,
    acts: &mut Vec<UiAction>,
) {
    let defs = rules();
    let def = defs.building(b.kind);
    let inv = &me.inventory;
    egui::Area::new(egui::Id::NULL.with("building")).anchor(Align2::RIGHT_TOP, [-12.0, 12.0]).show(ctx, |ui| {
        panel_frame().show(ui, |ui| {
            ui.set_min_width(280.0);
            // a right-aligned button would otherwise stretch the panel across half the screen
            ui.set_max_width(330.0);
            ui.horizontal(|ui| {
                let g = match b.kind {
                    k if k.burns() => crate::ui_art::Glyph::Flame,
                    BuildingKind::Shelter => crate::ui_art::Glyph::House,
                    BuildingKind::Depot => crate::ui_art::Glyph::Bag,
                    _ => crate::ui_art::Glyph::Hammer,
                };
                crate::ui_art::heading(ui, g, &format!("{} #{}", def.name, i + 1), Color32::from_rgb(232, 150, 70));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Kapat").clicked() {
                        acts.push(UiAction::Deselect);
                    }
                    if ui.small_button("Taşı").on_hover_text("Binayı içindekilerle başka bir yere koy").clicked() {
                        acts.push(UiAction::MoveBuilding(i));
                    }
                });
            });
            ui.separator();
            if b.kind.burns() {
                let frac = b.fuel as f32 / FIRE_FUEL_MAX as f32;
                let secs = b.fuel as u64 / TICKS_PER_SECOND;
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Flame, &format!("Yakıt {}:{:02}", secs / 60, secs % 60), frac, Color32::from_rgb(232, 118, 44), 270.0);
                if ui.add_enabled(inv.get(ItemKind::Wood) > 0, egui::Button::new("Odun at (+1 odun)")).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Refuel { building: i as u32 }));
                }
            }
            if def.heat_max > 0 {
                // the eye reads heat as colour first; only a learned tribe puts a number on it
                let t = b.temp;
                let label = if w.perception_level() >= 3 { format!("Sıcaklık {t} °C · {}", heat_word(t)) } else { format!("Sıcaklık: {}", heat_word(t)) };
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Flame, &label, t as f32 / def.heat_max as f32, heat_colour(t), 270.0);
                if let Some(job) = &b.job {
                    let need = defs.recipes[job.recipe as usize].heat;
                    if b.temp < need {
                        let goal = if w.perception_level() >= 3 { format!("{need} °C") } else { heat_word(need).to_string() };
                        ui.label(RichText::new(format!("Isınıyor: iş {goal} olunca başlar.")).small().color(Color32::from_rgb(240, 190, 110)));
                    }
                }
            }
            if b.kind == BuildingKind::Shelter {
                ui.label("Yakınında durunca gece soğuğundan korunursun.");
            }
            if b.kind == BuildingKind::Ruin {
                // what the eye makes of it depends on how much the tribe understands
                let was = BuildingKind::ALL.get(b.fuel as usize).map(|k| rules().building(*k).name.clone()).unwrap_or_default();
                ui.label(if w.perception_level() >= 3 { format!("Önceki bir dünyadan kalma: {was}.") } else { "Yıkılmış taşlar. Birileri bir şey yapmış.".to_string() });
            }
            if b.kind == BuildingKind::Computer {
                let live = w.electric_live().get(i).copied().unwrap_or(false);
                if !live {
                    ui.label(RichText::new("Kapalı: bir dinamo ağına (12 m içinde direk) bağla.").small().color(Color32::from_rgb(235, 140, 90)));
                } else {
                    design_table(ui, w, acts);
                }
            }
            if b.kind == BuildingKind::SteamEngine {
                ui.label(RichText::new(if b.steaming() { "Buhar var: 8 m içindeki makineleri çeviriyor." } else { "Kömür (ya da kok) ve dolu bir çömlek yükle, buhar kaldır: kazan 120 °C'yi geçince çalışır." }).small().weak());
            }
            if b.kind == BuildingKind::Pump {
                ui.label(RichText::new("Güç varken 15 m içindeki mağaranın en dip odasını kuru tutar.").small().weak());
            }
            if b.kind == BuildingKind::Conveyor {
                ui.label(RichText::new("Güç varken (8 m içinde su çarkı ya da yel değirmeni, 6 m içinde manej) 3 m içindeki istasyonların ürününü 12 m içindeki en yakın ambara taşır.").small().weak());
            }
            if b.kind == BuildingKind::Bloomery || b.kind == BuildingKind::BlastFurnace {
                ui.label(RichText::new("Demir için 1150 °C gerek: 3 m içinde çalışan bir körük olmazsa 1000 °C'yi geçmez. Körüğü başında durarak, manejle ya da su çarkıyla çalıştır.").small().weak());
            }
            if b.kind == BuildingKind::TripHammer {
                ui.label(RichText::new("8 m içindeki bir su çarkı (ya da manejde bir hayvan) çekici kaldırır: süngerimsi demiri kendi döver.").small().weak());
            }
            if b.kind == BuildingKind::HandMill {
                ui.label(RichText::new("Başında biri durunca ya da 6 m içindeki bir manejde öküz veya at dönünce öğütür; hayvan üç kat hızlı.").small().weak());
            }
            if b.kind == BuildingKind::AnimalGin {
                ui.label(RichText::new("Bir öküz ya da atı buraya getir (işçi panelinde ağıla götürür gibi): 6 m içindeki değirmeni çevirir.").small().weak());
            }
            if b.kind == BuildingKind::Field {
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Food, &format!("Toprak verimi %{}", b.fertility / 10), b.fertility as f32 / 1000.0, Color32::from_rgb(140, 110, 60), 270.0);
                if w.season() == 3 {
                    ui.label(RichText::new("Kış: ekin büyümez.").small().color(Color32::from_rgb(170, 200, 240)));
                }
                ui.label(RichText::new("Her hasat toprağı yorar; boş kalan tarla dinlenir, ağıldan gübre koymak verimi artırır.").small().weak());
            }
            if b.kind == BuildingKind::Pen {
                let r2 = alaz_sim::world::PEN_RADIUS_MM * alaz_sim::world::PEN_RADIUS_MM;
                let inside: Vec<String> = [AgentKind::Ox, AgentKind::Horse, AgentKind::Dog]
                    .into_iter()
                    .map(|k| (k, w.agents.iter().filter(|a| a.kind == k && (a.x - b.x).pow(2) + (a.y - b.y).pow(2) <= r2).count()))
                    .filter(|(_, n)| *n > 0)
                    .map(|(k, n)| format!("{n} {}", kind_name(k).to_lowercase()))
                    .collect();
                ui.label(if inside.is_empty() { "Ağıl boş.".to_string() } else { format!("İçeride: {}", inside.join(", ")) });
                ui.label(RichText::new("Aynı türden iki hayvan içerideyken yem verirsen yavru olur.").small().weak());
            }
            if b.kind == BuildingKind::Depot {
                ui.label(format!("İçindekiler: {}", fmt_counts(&b.output)));
                ui.label(RichText::new("Aç kalan işçiler buradaki yemiş/kömürden kendi kendine yer.").small().weak());
                let jars = b.output[ItemKind::LargePot as usize].min(4);
                ui.label(RichText::new(if jars > 0 {
                    format!("{jars} büyük çömlek: buradaki yiyecek {} kat yavaş bozulur.", 1 + jars)
                } else {
                    "Buraya konan büyük çömlekler yiyeceğin bozulmasını yavaşlatır.".to_string()
                }).small().weak());
                if ui.button("Hepsini yükle (araçlar hariç)").clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Deposit { building: i as u32 }));
                }
                ui.horizontal_wrapped(|ui| {
                    for k in ItemKind::ALL {
                        let n = b.output[k as usize];
                        if n > 0 && ui.small_button(format!("Al: {} ({})", item_name(k), n)).clicked() {
                            acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: Some(k) }));
                        }
                    }
                });
            }
            let recipes: Vec<(usize, &_)> =
                defs.recipes.iter().enumerate().filter(|(_, r)| r.station == Some(b.kind) && r.discovery != Discovery::Retired).collect();
            // alloying: the share of the loaded ores against the range that makes good metal
            if let Some(al) = recipes.iter().find_map(|(_, r)| r.alloy) {
                let (base, add) = (b.input[al.base as usize] as u32, b.input[al.add as usize] as u32);
                let realistic = w.settings.difficulty == alaz_sim::world::Difficulty::Realistic;
                let (lo, hi) = if realistic { (al.lo_realistic, al.hi_realistic) } else { (al.lo, al.hi) };
                if base + add > 0 {
                    let share = base * 100 / (base + add);
                    let good = (lo as u32..=hi as u32).contains(&share);
                    let col = if good { Color32::from_rgb(130, 215, 130) } else { Color32::from_rgb(235, 140, 90) };
                    let label = if w.perception_level() >= 3 { format!("{}: %{share} (iyi: %{lo}–{hi})", item_name(al.base)) } else if good { "Karışım iyi görünüyor".to_string() } else { "Karışım tutmuyor".to_string() };
                    crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Flame, &label, share as f32 / 100.0, col, 270.0);
                    ui.label(RichText::new(format!("{} taş yüklü: {} külçe çıkar (4 taş bir külçe).", base + add, (base + add) / al.ores_per_ingot.max(1))).small().weak());
                } else {
                    ui.label(RichText::new(format!("Bakır ve kalay taşı yükle: {}–{} bakır ölçüsünde bronz olur.", lo, hi)).small().weak());
                }
            }
            if !recipes.is_empty() {
                ui.label(format!("Girdi deposu: {}", fmt_counts(&b.input)));
                if ui.button("Girdileri yükle").clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Deposit { building: i as u32 }));
                }
                if let Some(job) = &b.job {
                    let r = &defs.recipes[job.recipe as usize];
                    let done = 1.0 - job.remaining as f32 / r.ticks as f32;
                    let left = job.remaining as u64 / TICKS_PER_SECOND;
                    crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Hammer, &format!("{} · {} sn kaldı", r.name, left), done, Color32::from_rgb(120, 190, 120), 270.0);
                }
                for (ri, r) in recipes {
                    if !w.knows(r.discovery) {
                        ui.add_enabled(false, egui::Button::new(format!("{} (henüz bilinmiyor)", r.name)));
                        continue;
                    }
                    let label = format!("{} ({} sn)", r.name, r.ticks as u64 / TICKS_PER_SECOND);
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 60))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(70, 56, 40)))
                        .corner_radius(0)
                        .inner_margin(6)
                        .show(ui, |ui| {
                            ui.set_width(258.0);
                            need_row(ui, w, &r.inputs, inv);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("->").strong().color(Color32::from_rgb(240, 200, 100)));
                                match r.spawns {
                                    Some(k) => {
                                        ui.label(kind_name(k));
                                    }
                                    None => {
                                        for it in &r.outputs {
                                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                                            crate::icons::draw_slot(ui.painter(), rect, it.item, w.has_seen(it.item));
                                            resp.on_hover_text(crate::lang::item_word(w, it.item));
                                            ui.label(format!("x{}", it.n));
                                        }
                                    }
                                }
                            });
                    ui.horizontal(|ui| {
                        let can_start = b.job.is_none() && inv.has(&r.inputs);
                        if ui.add_enabled(can_start, egui::Button::new(label)).clicked() {
                            acts.push(UiAction::Cmd(CommandKind::StartJob {
                                building: i as u32,
                                recipe: RecipeId(ri as u16),
                            }));
                        }
                        let auto_on = b.auto == Some(ri as u16);
                        if ui.selectable_label(auto_on, "Otomatik").clicked() {
                            acts.push(UiAction::Cmd(CommandKind::SetAuto {
                                building: i as u32,
                                recipe: if auto_on { None } else { Some(RecipeId(ri as u16)) },
                            }));
                        }
                    });
                        });
                }
                if b.output_total() > 0 && ui.button(format!("Topla: {}", fmt_counts(&b.output))).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: None }));
                }
            }
            ui.label(RichText::new("Kullanmak için yanına git (3 m).").small().weak());
        });
    });
}

/// What the design table is being set to (kept in the UI's memory between frames).
#[derive(Clone, Default)]
struct DesignState {
    ins: [(usize, u16); 3],
    out: (usize, u16),
}

/// The design table: pick up to three inputs and an output from what has been seen; the table shows what each side
/// is worth and writes the recipe into this world's rules if it is not worth more than it takes.
fn design_table(ui: &mut egui::Ui, w: &World, acts: &mut Vec<UiAction>) {
    let id = egui::Id::new("design_table");
    let mut st: DesignState = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    let seen: Vec<ItemKind> = ItemKind::ALL.iter().copied().filter(|k| w.has_seen(*k)).collect();
    if seen.is_empty() {
        return;
    }
    let values = w.item_values();
    ui.label(RichText::new("Tasarım masası").strong());
    ui.label(RichText::new("Yeni bir el tarifi yaz: bu dünyanın kurallarına girer ve yeni dünyaya taşınır.").small().weak());
    let pick = |ui: &mut egui::Ui, salt: &str, slot: &mut (usize, u16), allow_none: bool| {
        ui.horizontal(|ui| {
            let label = if slot.1 == 0 && allow_none { "—".to_string() } else { item_name(seen[slot.0.min(seen.len() - 1)]).to_string() };
            egui::ComboBox::from_id_salt(salt).selected_text(label).show_ui(ui, |ui| {
                for (k, item) in seen.iter().enumerate() {
                    if ui.selectable_label(slot.0 == k, item_name(*item)).clicked() {
                        slot.0 = k;
                        if slot.1 == 0 {
                            slot.1 = 1;
                        }
                    }
                }
            });
            ui.add(egui::DragValue::new(&mut slot.1).range(if allow_none { 0..=20 } else { 1..=20 }));
        });
    };
    for (n, slot) in st.ins.iter_mut().enumerate() {
        pick(ui, &format!("design_in_{n}"), slot, n > 0);
    }
    if st.ins[0].1 == 0 {
        st.ins[0].1 = 1;
    }
    ui.label("→");
    pick(ui, "design_out", &mut st.out, false);
    if st.out.1 == 0 {
        st.out.1 = 1;
    }
    let inputs: Vec<(ItemKind, u16)> = st.ins.iter().filter(|s| s.1 > 0).map(|s| (seen[s.0.min(seen.len() - 1)], s.1)).collect();
    let output = (seen[st.out.0.min(seen.len() - 1)], st.out.1);
    let take: u64 = inputs.iter().map(|&(k, n)| values[k as usize] as u64 * n as u64).sum();
    let give = values[output.0 as usize] as u64 * output.1 as u64;
    let ok = give <= take;
    ui.label(RichText::new(format!("Değer: alır {take}, verir {give}")).color(if ok { Color32::from_rgb(130, 215, 130) } else { Color32::from_rgb(235, 110, 110) }));
    if ui.add_enabled(ok, egui::Button::new("Tarifi yaz")).clicked() {
        acts.push(UiAction::Cmd(CommandKind::Design { inputs, output }));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, st));
    ui.separator();
    ui.label(RichText::new("Döngü: bu dünyanın hâli yeni bir dünyanın tohumu olur. Yazdığın kurallar ve yaptıkların harabe olarak oraya geçer; sen yine bir maymun olarak başlarsın.").small().weak());
    if ui.button("Döngüyü başlat: yeni dünya").clicked() {
        acts.push(UiAction::NewCycle);
    }
}

/// What a resource node is called, by what it gives.
pub fn node_label(w: &World, n: &alaz_sim::world::Node) -> String {
    use alaz_sim::NodeKind as N;
    let name = match n.kind {
        N::StoneOutcrop => "Taş yığını",
        N::Tree => "Ağaç",
        N::BerryBush => "Yemiş çalısı",
        N::ClayDeposit => "Kil yatağı",
        N::IronOutcrop => "Demir damarı",
        N::HerbPatch => "Şifalı ot",
        N::CopperOutcrop => "Bakır damarı",
        N::TinOutcrop => "Kalay damarı",
        N::Deadwood => "Kuru dallar",
        N::FiberPlant => "Lif otu",
        N::CaveMouth => "Mağara ağzı",
    };
    if w.perception_level() >= 2 && n.kind != N::CaveMouth {
        format!("{name} ({} kaldı)", n.amount)
    } else {
        name.to_string()
    }
}

/// A small label by the pointer: what it is on, and what a right click does there.
pub fn pointer_label(ctx: &egui::Context, text: &str) {
    let Some(p) = ctx.pointer_hover_pos() else { return };
    egui::Area::new(egui::Id::new("pointer_label")).fixed_pos(p + egui::vec2(16.0, 14.0)).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
        egui::Frame::new().fill(Color32::from_black_alpha(190)).corner_radius(0).inner_margin(egui::Margin::symmetric(7, 4)).show(ui, |ui| {
            // one line, as long as it needs: a pointer label is never wrapped word by word
            ui.add(egui::Label::new(RichText::new(text).color(Color32::from_rgb(246, 238, 222)).size(22.0)).extend());
        });
    });
}

/// The colour hot things glow at (TerraFirmaCraft's scale), as a word.
pub fn heat_word(t: i16) -> &'static str {
    match t {
        i16::MIN..=79 => "soğuk",
        80..=209 => "sıcak",
        210..=479 => "çok sıcak",
        480..=579 => "soluk kırmızı",
        580..=729 => "koyu kırmızı",
        730..=929 => "parlak kırmızı",
        930..=1099 => "turuncu",
        1100..=1299 => "sarı",
        1300..=1399 => "sarı beyaz",
        1400..=1499 => "beyaz",
        _ => "parlak beyaz",
    }
}

fn heat_colour(t: i16) -> Color32 {
    match t {
        i16::MIN..=209 => Color32::from_rgb(150, 130, 110),
        210..=579 => Color32::from_rgb(170, 70, 50),
        580..=929 => Color32::from_rgb(220, 60, 40),
        930..=1099 => Color32::from_rgb(240, 130, 40),
        1100..=1299 => Color32::from_rgb(250, 210, 70),
        _ => Color32::from_rgb(255, 245, 210),
    }
}

fn draw_worker(
    ctx: &egui::Context,
    w: &World,
    i: usize,
    a: &alaz_sim::world::Agent,
    me: &alaz_sim::world::Agent,
    ui_ctx: &UiCtx,
    acts: &mut Vec<UiAction>,
) {
    egui::Area::new(egui::Id::NULL.with("worker")).anchor(Align2::RIGHT_TOP, [-12.0, 12.0]).show(ctx, |ui| {
        panel_frame().show(ui, |ui| {
            ui.set_min_width(280.0);
            // a right-aligned button would otherwise stretch the panel across half the screen
            ui.set_max_width(330.0);
            ui.horizontal(|ui| {
                crate::ui_art::heading(ui, crate::ui_art::Glyph::Route, &format!("{} #{}", kind_name(a.kind), i), Color32::from_rgb(130, 200, 150));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Kapat").clicked() {
                        acts.push(UiAction::Deselect);
                    }
                });
            });
            ui.separator();
            if let Some(item) = a.kind.upkeep_item() {
                let label = if a.hunger == 0 { format!("{} bitti — hareketsiz", item_name(item)) } else { item_name(item).to_string() };
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Food, &label, a.hunger as f32 / MAX_STAT as f32, Color32::from_rgb(214, 156, 56), 270.0);
                if ui
                    .add_enabled(me.inventory.get(item) > 0, egui::Button::new(format!("Besle (1 {})", item_name(item))))
                    .clicked()
                {
                    acts.push(UiAction::Cmd(CommandKind::Feed { target: a.id }));
                }
            }
            crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Bag, &format!("Taşıdığı {}/{}", a.inventory.load(), a.capacity()), a.inventory.load() as f32 / a.capacity().max(1) as f32, Color32::from_rgb(150, 120, 70), 270.0);
            if a.inventory.total() > 0 {
                ui.label(RichText::new(fmt_counts(&a.inventory.counts)).small());
            }
            if a.route.is_empty() {
                ui.label("Rotası yok (beklemede).");
            } else {
                ui.label(RichText::new(format!("Rota: {} adım", a.route.len())).strong());
                for (k, s) in a.route.iter().enumerate() {
                    let cur = k as u32 == a.route_i;
                    let text = format!("{} {}", if cur { ">" } else { "-" }, step_text(s, w));
                    if cur {
                        ui.label(RichText::new(text).color(Color32::from_rgb(140, 220, 140)));
                    } else {
                        ui.label(RichText::new(text).small());
                    }
                }
            }
            let can = !ui_ctx.recorded.is_empty() && !ui_ctx.recording;
            let btn = format!("Kayıtlı rotayı ata ({} adım)", ui_ctx.recorded.len());
            if ui.add_enabled(can, egui::Button::new(btn)).clicked() {
                acts.push(UiAction::AssignRecorded(a.id));
            }
            let pen = w
                .buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == BuildingKind::Pen)
                .min_by_key(|(_, b)| (a.x - b.x).pow(2) + (a.y - b.y).pow(2))
                .map(|(pi, _)| pi as u32);
            if matches!(a.kind, AgentKind::Ox | AgentKind::Horse) {
                for (item, label) in [(ItemKind::Cart, "Arabayı koş"), (ItemKind::Plow, "Sabanı koş")] {
                    if me.inventory.get(item) > 0 && a.inventory.get(item) == 0 {
                        let near = (me.x - a.x).pow(2) + (me.y - a.y).pow(2) <= alaz_sim::world::USE_RADIUS_MM.pow(2);
                        let resp = ui.add_enabled(near, egui::Button::new(label));
                        if resp.on_disabled_hover_text("Hayvanın yanına git (3 m).").clicked() {
                            acts.push(UiAction::Cmd(CommandKind::Hitch { worker: a.id, item }));
                        }
                    }
                }
                let field = w
                    .buildings
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.kind == BuildingKind::Field)
                    .min_by_key(|(_, b)| (a.x - b.x).pow(2) + (a.y - b.y).pow(2))
                    .map(|(fi, _)| fi as u32);
                let gin = w
                    .buildings
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.kind == BuildingKind::AnimalGin)
                    .min_by_key(|(_, b)| (a.x - b.x).pow(2) + (a.y - b.y).pow(2))
                    .map(|(gi, _)| gi as u32);
                if let Some(gin) = gin {
                    if ui.button("Maneje götür").on_hover_text("Manejin çevresinde döner: 6 m içindeki değirmeni çevirir.").clicked() {
                        let route = vec![RouteStep::Deposit { building: gin }, RouteStep::Wait { ticks: 10 * 60 * TICKS_PER_SECOND as u32 }];
                        acts.push(UiAction::Cmd(CommandKind::SetRoute { worker: a.id, route }));
                    }
                }
                if let Some(field) = field.filter(|_| a.inventory.get(ItemKind::Plow) > 0) {
                    if ui.button("Tarlaya götür").on_hover_text("Sabanla tarlanın yanında durur: ekin iki kat hızlı büyür.").clicked() {
                        let route = vec![RouteStep::Deposit { building: field }, RouteStep::Wait { ticks: 10 * 60 * TICKS_PER_SECOND as u32 }];
                        acts.push(UiAction::Cmd(CommandKind::SetRoute { worker: a.id, route }));
                    }
                }
            }
            if let Some(pen) = pen.filter(|_| a.kind != AgentKind::Bot) {
                // walk in, drop any fodder carried, then stay put a long while (the route loops)
                if ui.button("Ağıla götür").on_hover_text("Ağılda kalır; taşıdığı yemi yemliğe koyar.").clicked() {
                    let route = vec![RouteStep::Deposit { building: pen }, RouteStep::Wait { ticks: 10 * 60 * TICKS_PER_SECOND as u32 }];
                    acts.push(UiAction::Cmd(CommandKind::SetRoute { worker: a.id, route }));
                }
            }
            if ui.add_enabled(!a.route.is_empty(), egui::Button::new("Rotayı durdur")).clicked() {
                acts.push(UiAction::Cmd(CommandKind::SetRoute { worker: a.id, route: Vec::new() }));
            }
            if !a.kind.can_harvest() {
                ui.label(RichText::new("Hayvanlar toplayamaz, sadece taşır.").small().weak());
            }
        });
    });
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use alaz_sim::command::Command;
    use egui_kittest::kittest::Queryable;

    struct State {
        w: World,
        knap: KnapState,
        acts: Vec<UiAction>,
        tab: Option<Tab>,
        binds: Keybinds,
    }

    fn app(ctx: &egui::Context, s: &mut State) {
        let ui_ctx = UiCtx {
            hints: &[],
            build_mode: None,
            grid_snap: false,
            selection: None,
            recording: false,
            recorded: &[],
            me: AgentId(0),
            net_status: "",
            last_strike: None,
            tab: s.tab,
            binds: &s.binds,
            anchor: Some([0.0, 0.0]),
        };
        draw(ctx, &s.w, &ui_ctx, &mut s.knap, &mut s.acts);
    }

    /// Apply what the UI asked for, the way the game loop does.
    fn pump(h: &mut egui_kittest::Harness<'_, State>) {
        h.run();
        let acts: Vec<UiAction> = std::mem::take(&mut h.state_mut().acts);
        for a in acts {
            let s = h.state_mut();
            match a {
                UiAction::Cmd(k) => s.w.step(&[Command { agent: AgentId(0), kind: k }]),
                UiAction::ToggleTab(t) => s.tab = if s.tab == Some(t) { None } else { Some(t) },
                _ => {}
            }
        }
        h.run();
    }

    #[test]
    fn the_guide_walks_a_newcomer_from_stones_to_the_first_fire() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.forget_everything();
        let say = |w: &World| guide_text(w, &w.agents[0]);
        assert!(say(&w).contains("taş"), "{}", say(&w));
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
        assert!(say(&w).contains("Yap sekmesinde"), "{}", say(&w));
        w.learn(Discovery::SharpStone);
        assert!(say(&w).contains("keskin taş çıkar"), "still tells to make the sharp stone: {}", say(&w));
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 0;
        w.agents[0].inventory.counts[ItemKind::SharpStone as usize] = 1;
        assert!(say(&w).contains("dal"), "{}", say(&w));
        w.learn(Discovery::StoneAxe);
        assert!(say(&w).contains("odun"), "{}", say(&w));
        w.agents[0].inventory.counts[ItemKind::Wood as usize] = 3;
        assert!(say(&w).contains("çakmaktaşı"), "a fire needs flint first: {}", say(&w));
        w.agents[0].inventory.counts[ItemKind::Flint as usize] = 1;
        assert!(say(&w).contains("İnşa"), "{}", say(&w));
        w.agents[0].thirst = 1000;
        assert!(say(&w).contains("Su iç"), "thirst comes first: {}", say(&w));
    }

    #[test]
    fn the_notebook_hints_at_what_can_be_tried_once_the_things_have_been_seen() {
        let mk = |seen: bool| {
            let mut w = World::new(5);
            let (sx, sy) = w.spawn_mm;
            w.spawn_agent(sx, sy);
            w.forget_everything();
            if seen {
                w.mark_seen(ItemKind::Stone);
            }
            let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: Some(Tab::Craft), binds: Keybinds::default() };
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_ui_state(|ui, s| app(ui.ctx(), s), state);
            h.run();
            h
        };
        let blind = mk(false);
        assert!(blind.query_by_label_contains("işe yarar gibi").is_none(), "nothing seen, no clue");
        let seen = mk(true);
        assert!(seen.query_by_label_contains("işe yarar gibi").is_some(), "stones seen: the notebook nudges");
    }

    #[test]
    fn a_player_can_combine_two_stones_with_the_mouse_and_play_the_knapping() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.forget_everything();
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
        w.mark_seen(ItemKind::Stone);
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: None, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        h.run();
        // open the Yap tab with a click on its button
        h.get_by_label_contains("Yap [").click();
        pump(&mut h);
        assert_eq!(h.state().tab, Some(Tab::Craft), "the Yap button opens the craft tab");
        // pick the stone twice, then Dene
        h.get_by_label_contains("Çantadan al").click();
        h.run();
        h.get_by_label_contains("Çantadan al").click();
        h.run();
        assert!(h.state().knap.hands.iter().filter(|&&i| i == ItemKind::Stone as u8).count() == 2, "two stones in hands");
        h.get_by_label("Dene").click();
        pump(&mut h);
        pump(&mut h);
        assert!(h.state().w.craft_view(AgentId(0)).is_some(), "the knapping minigame started from the click");
        assert!(h.state().w.knows(alaz_sim::Discovery::SharpStone), "trying taught the discovery");
        // the minigame draws, and stays drawable for a few frames
        for _ in 0..3 {
            h.run();
        }
        assert!(h.query_all_by_label_contains("Keskin taş").count() > 0, "the knapping screen names the work");
    }

    #[test]
    fn clicking_a_filled_hand_slot_empties_it() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
        w.mark_seen(ItemKind::Stone);
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: Some(Tab::Craft), binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        h.run();
        h.get_by_label_contains("Çantadan al").click();
        h.run();
        h.get_by_label_contains("El 1: ").click();
        h.run();
        assert!(h.state().knap.hands.iter().all(|&i| i == 255));
    }

    #[test]
    fn holding_and_releasing_the_mouse_on_the_stone_strikes_where_it_points() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
        w.mark_seen(ItemKind::Stone);
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: None, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        h.state_mut().w.step(&[Command { agent: AgentId(0), kind: CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255] } }]);
        h.run();
        let v = h.state().w.craft_view(AgentId(0)).expect("knapping started");
        let rect = h.get_by_label("Yontma taşı").rect();
        let c = rect.center();
        let aim = rim_point(c, KNAP_SIZE * 0.28, v.center as f32);
        h.event(egui::Event::PointerMoved(aim));
        h.run();
        h.event(egui::Event::PointerButton { pos: aim, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        let frames = (v.power as f32 / 100.0 / 1.1 * 60.0).round() as usize;
        for _ in 0..frames.max(1) {
            h.step();
        }
        h.event(egui::Event::PointerButton { pos: aim, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
        let struck: Vec<(u16, u8)> = h.state().acts.iter().filter_map(|a| if let UiAction::Cmd(CommandKind::StrikeAt { angle, power }) = a { Some((*angle, *power)) } else { None }).collect();
        assert_eq!(struck.len(), 1, "one strike per hold and release: {:?}", h.state().acts.len());
        let (angle, power) = struck[0];
        let da = ((angle as i32 - v.center as i32 + 540).rem_euclid(360) - 180).unsigned_abs();
        assert!(da <= 3, "aimed at the marked spot: angle {angle} vs {}", v.center);
        assert!((power as i32 - v.power as i32).unsigned_abs() <= v.power_half, "force near the target: {power} vs {}", v.power);
    }

    #[test]
    fn letting_go_off_the_stone_calls_the_swing_off() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
        w.mark_seen(ItemKind::Stone);
        w.step(&[Command { agent: AgentId(0), kind: CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255] } }]);
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: None, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        h.run();
        let rect = h.get_by_label("Yontma taşı").rect();
        let on = rect.center() + egui::vec2(30.0, 0.0);
        h.event(egui::Event::PointerMoved(on));
        h.step();
        h.event(egui::Event::PointerButton { pos: on, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        for _ in 0..20 {
            h.step();
        }
        let off = egui::pos2(rect.right() + 200.0, rect.bottom() + 40.0);
        h.event(egui::Event::PointerMoved(off));
        h.step();
        h.event(egui::Event::PointerButton { pos: off, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
        assert!(!h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::StrikeAt { .. }))), "no strike from a release off the stone");
        assert!(!h.state().knap.holding);
    }

    #[test]
    fn hands_let_go_of_what_left_the_bag() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 1;
        w.mark_seen(ItemKind::Stone);
        let mut knap = KnapState::default();
        knap.hands = [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255];
        let state = State { w, knap, acts: Vec::new(), tab: Some(Tab::Craft), binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        h.run();
        let n = h.state().knap.hands.iter().filter(|&&i| i == ItemKind::Stone as u8).count();
        assert_eq!(n, 1, "only the one stone still in the bag stays in hand");
    }

    /// A whole stone axe, made only through simulated mouse input on the real screens: every step is played the way a person would.
    #[test]
    fn a_stone_axe_can_be_made_start_to_finish_with_the_mouse() {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        for (k, n) in [(ItemKind::SharpStone, 1), (ItemKind::Branch, 1), (ItemKind::Fiber, 2)] {
            w.agents[0].inventory.counts[k as usize] = n;
            w.mark_seen(k);
        }
        w.step(&[Command { agent: AgentId(0), kind: CommandKind::Combine { items: [ItemKind::SharpStone as u8, ItemKind::Branch as u8, ItemKind::Fiber as u8, ItemKind::Fiber as u8] } }]);
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: None, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 720.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, s| app(ui.ctx(), s), state);
        let apply = |h: &mut egui_kittest::Harness<'_, State>| {
            let acts: Vec<UiAction> = std::mem::take(&mut h.state_mut().acts);
            for a in acts {
                if let UiAction::Cmd(k) = a {
                    h.state_mut().w.step(&[Command { agent: AgentId(0), kind: k }]);
                }
            }
        };
        let mut seen_kinds = Vec::new();
        for _ in 0..3000 {
            let Some(v) = h.state().w.craft_view(AgentId(0)) else { break };
            if !seen_kinds.contains(&(v.step, v.kind as u8)) {
                seen_kinds.push((v.step, v.kind as u8));
            }
            match v.kind {
                alaz_sim::MinigameKind::Timing => {
                    h.step();
                    h.state_mut().w.step(&[]);
                    let v = h.state().w.craft_view(AgentId(0)).unwrap();
                    if v.pos.abs_diff(v.center) <= v.half / 3 {
                        h.get_by_label_contains("Vur!").click();
                        h.step();
                        apply(&mut h);
                    }
                }
                alaz_sim::MinigameKind::Fit => {
                    h.step();
                    h.step();
                    let rect = h.get_by_label("Yerleştirme alanı").rect();
                    let k = FIT_SIZE / 290.0 * 1.35;
                    let p = egui::pos2(rect.center().x + v.tx as f32 * k, rect.center().y + v.ty as f32 * k);
                    h.event(egui::Event::PointerMoved(p));
                    h.step();
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    h.step();
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
                    h.step();
                    apply(&mut h);
                }
                alaz_sim::MinigameKind::Stack => {
                    h.step();
                    h.step();
                    let rect = h.get_by_label("Yığın alanı").rect();
                    let p = egui::pos2(rect.center().x, rect.center().y);
                    h.event(egui::Event::PointerMoved(p));
                    h.step();
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    h.step();
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
                    h.step();
                    apply(&mut h);
                }
                alaz_sim::MinigameKind::Tension => {
                    h.step();
                    h.step();
                    let rect = h.get_by_label("Sıkıştırma alanı").rect();
                    let p = rect.center();
                    h.event(egui::Event::PointerMoved(p));
                    h.step();
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    for _ in 0..((v.power as f32 / 100.0 / 0.9 * 60.0).round() as usize).max(1) {
                        h.step();
                    }
                    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
                    h.step();
                    apply(&mut h);
                }
                alaz_sim::MinigameKind::Knap => {
                    h.step();
                    h.step();
                    let c = h.get_by_label("Yontma taşı").rect().center();
                    let aim = rim_point(c, KNAP_SIZE * 0.28, v.center as f32);
                    h.event(egui::Event::PointerMoved(aim));
                    h.step();
                    h.event(egui::Event::PointerButton { pos: aim, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
                    for _ in 0..((v.power as f32 / 100.0 / 1.1 * 60.0).round() as usize).max(1) {
                        h.step();
                    }
                    h.event(egui::Event::PointerButton { pos: aim, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
                    h.step();
                    apply(&mut h);
                }
            }
        }
        assert!(h.state().w.craft_view(AgentId(0)).is_none(), "the craft ended; steps seen: {seen_kinds:?}");
        assert!(seen_kinds.len() >= 4, "all four steps were played: {seen_kinds:?}");
        assert_eq!(h.state().w.agents[0].inventory.get(ItemKind::StoneAxe), 1, "a stone axe came out");
    }

    /// The work areas must stay fully on screen, even in a small window and with the worker near an edge.
    #[test]
    fn minigame_areas_fit_small_windows() {
        for (size, anchor) in [((800.0f32, 600.0f32), [0.0f32, 0.0f32]), ((800.0, 600.0), [-0.95, 0.9]), ((1000.0, 720.0), [0.95, -0.9])] {
            let mut w = World::new(5);
            let (sx, sy) = w.spawn_mm;
            w.spawn_agent(sx, sy);
            w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
            w.mark_seen(ItemKind::Stone);
            w.step(&[Command { agent: AgentId(0), kind: CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255] } }]);
            let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: None, binds: Keybinds::default() };
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(size.0, size.1)).with_step_dt(1.0 / 60.0).build_ui_state(
                move |ui, s| {
                    let ui_ctx_anchor: [f32; 2] = anchor;
                    let ui_ctx = UiCtx {
                        hints: &[], build_mode: None, grid_snap: false, selection: None, recording: false, recorded: &[], me: AgentId(0),
                        net_status: "", last_strike: None, tab: s.tab, binds: &s.binds, anchor: Some(ui_ctx_anchor),
                    };
                    draw(ui.ctx(), &s.w, &ui_ctx, &mut s.knap, &mut s.acts);
                },
                state,
            );
            h.step();
            h.step();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1));
            let r = h.get_by_label("Yontma taşı").rect();
            assert!(screen.contains_rect(r), "knapping area {r:?} outside {screen:?} (anchor {anchor:?})");
        }
    }
}


/// Inside a cave: a dark chamber lit only by the torch, with the choices of the moment underneath.
fn draw_cave(ctx: &egui::Context, w: &World, me: AgentId, acts: &mut Vec<UiAction>) {
    use alaz_sim::world::{Task, CAVE_DEEPER_TICKS, CAVE_MAX_DEPTH, CAVE_MINE_TICKS, CAVE_TORCH_TICKS};
    let Some(Task::Cave { depth, light, act, work, .. }) = w.agents.iter().find(|a| a.id == me).map(|a| a.task) else { return };
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("cave")).fixed_pos(egui::pos2(screen.center().x - 230.0, (screen.center().y - 230.0).max(70.0))).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_max_width(460.0);
        panel_frame().show(ui, |ui| {
            ui.set_min_width(440.0);
            ui.label(RichText::new("Mağara").strong().size(22.0));
            let names = ["Girişteki oda", "İkinci oda: hava ağırlaşıyor", "Derin galeri: damarlar zengin", "En dip: tavan çatırdıyor"];
            ui.label(RichText::new(names[depth.min(3) as usize]).small());
            let (resp, p) = ui.allocate_painter(egui::vec2(440.0, 200.0), egui::Sense::hover());
            let r = resp.rect;
            p.rect_filled(r, 0.0, Color32::from_rgb(10, 9, 12));
            let t = ctx.input(|i| i.time) as f32;
            let flick = 1.0 + 0.05 * (t * 9.0).sin() + 0.03 * (t * 23.0).sin();
            let frac = (light as f32 / CAVE_TORCH_TICKS as f32).clamp(0.0, 1.0);
            let torch = egui::pos2(r.left() + 60.0, r.center().y + 26.0);
            let radius = (70.0 + 170.0 * frac.sqrt()) * flick;
            // rock around the chamber: ragged ceiling and floor, as dark shapes
            let wall = |y0: f32, sign: f32, seed: f32| -> Vec<egui::Pos2> {
                (0..=44).map(|i| {
                    let x = r.left() + i as f32 * 10.0;
                    let n = ((i as f32 * 1.7 + seed).sin() * 9.0 + (i as f32 * 0.55 + seed * 2.0).sin() * 14.0).abs();
                    egui::pos2(x, y0 + sign * n)
                }).collect()
            };
            for (y0, s, seed) in [(r.top() + 22.0, 1.0, 1.0 + depth as f32), (r.bottom() - 26.0, -1.0, 4.0 + depth as f32)] {
                let mut pts = wall(y0, s, seed);
                let edge_y = if s > 0.0 { r.top() } else { r.bottom() };
                pts.push(egui::pos2(r.right(), edge_y));
                pts.push(egui::pos2(r.left(), edge_y));
                p.add(egui::Shape::convex_polygon(pts, Color32::from_rgb(36, 32, 30), Stroke::NONE));
            }
            // the pool of firelight, built from soft rings
            for k in (1..=12).rev() {
                let f = k as f32 / 12.0;
                p.circle_filled(torch, radius * f, Color32::from_rgba_unmultiplied(255, 170, 70, (9.0 + 14.0 * (1.0 - f)) as u8));
            }
            // ore glints, only where the light reaches; richer seams further in
            let ore_col = [Color32::from_rgb(190, 110, 60), Color32::from_rgb(70, 200, 160), Color32::from_rgb(220, 224, 236)];
            for i in 0..(5 + depth as usize * 3) {
                let s = (i as f32 * 12.9898 + depth as f32 * 78.233).sin() * 43758.545;
                let (fx, fy) = (s.fract().abs(), (s * 1.7).fract().abs());
                let pos = egui::pos2(r.left() + 110.0 + fx * 310.0, r.top() + 40.0 + fy * 120.0);
                if pos.distance(torch) < radius * 0.85 {
                    p.circle_filled(pos, 3.0, ore_col[(i + depth as usize) % 3]);
                    p.circle_filled(pos, 1.2, Color32::WHITE);
                }
            }
            // the torch and the miner
            p.circle_filled(egui::pos2(torch.x, torch.y - 14.0), 7.0 * flick, Color32::from_rgb(255, 210, 90));
            p.circle_filled(egui::pos2(torch.x, torch.y - 14.0), 3.5, Color32::from_rgb(255, 250, 200));
            p.line_segment([egui::pos2(torch.x, torch.y - 8.0), torch], Stroke::new(3.0, Color32::from_rgb(110, 76, 40)));
            p.circle_filled(egui::pos2(torch.x - 14.0, torch.y + 4.0), 8.0, Color32::from_rgb(190, 150, 110));
            p.rect_filled(egui::Rect::from_min_size(egui::pos2(torch.x - 20.0, torch.y + 12.0), egui::vec2(14.0, 24.0)), 0.0, Color32::from_rgb(110, 90, 60));
            if act != 0 {
                let total = if act == 1 { CAVE_MINE_TICKS } else { CAVE_DEEPER_TICKS } as f32;
                let bar = egui::Rect::from_min_size(egui::pos2(r.left() + 120.0, r.bottom() - 18.0), egui::vec2(300.0, 6.0));
                p.rect_filled(bar, 0.0, Color32::from_gray(40));
                p.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(300.0 * (work as f32 / total).min(1.0), 6.0)), 0.0, Color32::from_rgb(240, 190, 90));
                p.text(egui::pos2(r.left() + 270.0, r.bottom() - 30.0), Align2::CENTER_CENTER, if act == 1 { "Kazıyorsun..." } else { "İlerliyorsun..." }, egui::FontId::proportional(11.0), Color32::from_gray(200));
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
            // light left
            let (lb, _) = ui.allocate_exact_size(egui::vec2(440.0, 8.0), egui::Sense::hover());
            ui.painter().rect_filled(lb, 0.0, Color32::from_gray(35));
            ui.painter().rect_filled(egui::Rect::from_min_size(lb.min, egui::vec2(440.0 * frac, 8.0)), 0.0, Color32::from_rgb(255, 170, 60));
            ui.label(RichText::new(format!("Meşale: {} sn  ·  Tavan çökme riski kazarken: %{}", light / 60, 5 + 7 * depth as u32)).small());
            ui.horizontal(|ui| {
                let idle = act == 0;
                if ui.add_enabled(idle, egui::Button::new("Damarı kaz")).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::CaveMine));
                }
                if ui.add_enabled(idle && depth < CAVE_MAX_DEPTH, egui::Button::new("Daha derine in")).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::CaveDeeper));
                }
                if ui.button("Dışarı çık").clicked() {
                    acts.push(UiAction::Cmd(CommandKind::CaveLeave));
                }
            });
        });
    });
}

/// Red edges after a blow, fading with `k` (1 to 0); drawn under every panel.
pub fn hurt_vignette(ctx: &egui::Context, k: f32) {
    let r = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::background());
    let red = |a: f32| egui::Color32::from_rgba_unmultiplied(170, 16, 12, (a * 255.0) as u8);
    let edge = red(0.55 * k);
    let clear = red(0.0);
    let w = r.width().min(r.height()) * (0.16 + 0.1 * k);
    let mut mesh = egui::epaint::Mesh::default();
    // four bands, each fading from the screen edge inward
    let bands = [
        (r.left_top(), r.right_top(), egui::vec2(0.0, w)),
        (r.left_bottom(), r.right_bottom(), egui::vec2(0.0, -w)),
        (r.left_top(), r.left_bottom(), egui::vec2(w, 0.0)),
        (r.right_top(), r.right_bottom(), egui::vec2(-w, 0.0)),
    ];
    for (a, b, d) in bands {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(a, edge);
        mesh.colored_vertex(b, edge);
        mesh.colored_vertex(b + d, clear);
        mesh.colored_vertex(a + d, clear);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    p.add(egui::Shape::mesh(mesh));
}

/// A small compass in the lower right: K (north), D (east), G (south), B (west) laid flat on the ground the way the
/// camera sees it, turning as the view orbits, with the sun (or moon) on its rim where the light comes from.
pub fn compass(ctx: &egui::Context, yaw_deg: f32, pitch_deg: f32, sun: [f32; 3], day: bool) {
    use egui::{pos2, vec2, Color32, FontId, Stroke};
    let r = ctx.content_rect();
    let rad = 30.0;
    // above the needs panel in the bottom right corner
    let c = pos2(r.right() - rad - 30.0, r.bottom() - rad - 162.0);
    let (ys, yc) = yaw_deg.to_radians().sin_cos();
    let flat = pitch_deg.to_radians().sin().clamp(0.45, 1.0);
    // a ground direction (east, south) on screen
    let to = |dx: f32, dz: f32, k: f32| c + vec2(dx * yc - dz * ys, (dx * ys + dz * yc) * flat) * k;
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("compass")));
    let ring: Vec<egui::Pos2> = (0..32).map(|i| {
        let a = i as f32 / 32.0 * std::f32::consts::TAU;
        to(a.cos(), a.sin(), rad)
    }).collect();
    p.add(egui::Shape::convex_polygon(ring.clone(), Color32::from_rgba_unmultiplied(20, 18, 14, 170), Stroke::new(1.5, Color32::from_rgb(150, 132, 96))));
    // the needle: red half to the north
    let (n, s_) = (to(0.0, -1.0, rad * 0.78), to(0.0, 1.0, rad * 0.78));
    let side = to(1.0, 0.0, rad * 0.16) - c;
    p.add(egui::Shape::convex_polygon(vec![n, c + side, c - side], Color32::from_rgb(200, 60, 48), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![s_, c - side, c + side], Color32::from_rgb(214, 206, 186), Stroke::NONE));
    for (dx, dz, t) in [(0.0, -1.0, "K"), (1.0, 0.0, "D"), (0.0, 1.0, "G"), (-1.0, 0.0, "B")] {
        let at = to(dx, dz, rad + 11.0);
        let col = if t == "K" { Color32::from_rgb(236, 92, 76) } else { Color32::from_rgb(232, 222, 196) };
        p.text(at + vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, t, FontId::proportional(22.0), Color32::from_black_alpha(200));
        p.text(at, egui::Align2::CENTER_CENTER, t, FontId::proportional(22.0), col);
    }
    // where the light comes from
    let hl = (sun[0] * sun[0] + sun[2] * sun[2]).sqrt();
    if hl > 0.05 {
        let at = to(sun[0] / hl, sun[2] / hl, rad);
        let col = if day { Color32::from_rgb(255, 204, 72) } else { Color32::from_rgb(190, 200, 230) };
        p.circle(at, 5.5, col, Stroke::new(1.0, Color32::from_black_alpha(160)));
    }
}
