# A.L.A.Z — Ape Learns Another Zing

Taş devrinden bilgisayar çağına uzanan bir hayatta kalma, keşif ve otomasyon oyunu. Ağaçtan yeni inmiş, hiçbir şey bilmeyen bir maymun-insanla başlarsın:
- iki taşı birbirine vurup keskin taş yaparsın
- ateşi, çömleği, bakırı, demiri, buharı, elektriği bulursun
- sonunda kendi kurallarını yazabilen bir bilgisayar kurarsın

Döngünün sonunda bu dünyanın izleri, bir sonraki dünyada harabe olarak durur.

Rust ile yazılmış kendi motoru var: wgpu, winit ve egui. Simülasyon tamamen deterministik ve tamsayı tabanlı; çok oyunculu kip lockstep çalışır. Platform: Linux.

> Tasarım belgesi: [DESIGN.md](DESIGN.md) · Yol haritası ve kararlar: [PLAN.md](PLAN.md) · Son hata ayıklama raporu: [docs/hata-ayiklama-raporu.md](docs/hata-ayiklama-raporu.md)

---

## İçindekiler

- [Öne çıkanlar](#öne-çıkanlar)
- [Kurulum ve çalıştırma](#kurulum-ve-çalıştırma)
- [Kontroller](#kontroller)
- [Oynanış](#oynanış)
- [Çağlar](#çağlar)
- [Dünya](#dünya)
- [Mimari](#mimari)
- [Kurallar dosyası (modlama)](#kurallar-dosyası-modlama)
- [Kayıtlar](#kayıtlar)
- [Çok oyunculu](#çok-oyunculu)
- [Geliştirici araçları ve debug kapıları](#geliştirici-araçları-ve-debug-kapıları)
- [Testler](#testler)
- [Durum ve bilinen sorunlar](#durum-ve-bilinen-sorunlar)

---

## Öne çıkanlar

- **Yaparak öğrenme:** Tarif listesi yok. Elindekileri birlikte dener, yaptığın şeyden öğrenirsin. Kabile ne kadar çok şey bilirse dünyayı o kadar net görür; algı ilerledikçe renkler, sayılar ve ayrıntılar açılır.
- **El işi mini oyunları:** taş yontma (nişan ve güç), parça yerleştirme, sicim gerdirme, zamanlamalı dövme, üst üste dizme. Aletler hedefi büyütür.
- **Gerçekçi zincirler** (TerraFirmaCraft ve Vintage Story'den esinlenerek):
  - ısıya bağlı fırınlar; körük olmadan bloomery 1000 °C'yi geçemez
  - oranlı alaşım (tunç); kil kalıplar ve pota; aşınan ve onarılan aletler
  - bozulan yiyecek, mevsimler ve kış, toprağın yorulması, beslenme grupları
- **Güç ve otomasyon** (Create'ten esinlenerek):
  - güç kaynakları: hayvan manejleri, su çarkı, yel değirmeni, buhar makinesi, dinamo ve elektrik şebekesi
  - otomasyon: taşıyıcı bantlar, rotalı otomatlar
- **Canlı dünya:**
  - evcilleştirilebilen köpek, öküz ve at; kurtlar; avlanma
  - mağaralar, gizli maden yatakları ve arama kazması
- **Su:** Rüzgârla şekillenen Gerstner dalgaları, akan nehirler, yüzme ve boğulma, suda yüzüp akıntıyla giden eşyalar.
- **Fizik:** Devrilen ağaçlar, yuvarlanan kayalar, düşme hasarı, zıplama.
- **Ortak rüzgâr:** Yön ve güç tek kaynaktan gelir. Otlar, yapraklar, dalgalar, bulutlar, duman ve yağmur hep aynı rüzgârla hareket eder.
- **Büyük dünyalar:** 128 m'den 262 km'ye. Büyük dünyalar Minecraft gibi chunk chunk, oyuncu yaklaştıkça üretilir.
- **Hikâye ve öğretici:** Açılış anlatısı, her çağa bir sayfa ve kendiliğinden işaretlenen "İlk adımlar" listesi.
- **Son oyun:** Bilgisayarın tasarım masasında kendi tarifini yazarsın; tarif, oyunun bir değer dengesi denetiminden geçer. Döngü, bu dünyanın durumundan yeni bir dünya başlatır.

## Kurulum ve çalıştırma

Gerekenler: Rust (2024 edition, güncel stable), Vulkan destekli bir ekran kartı sürücüsü, Linux (Wayland veya X11).

```bash
git clone https://github.com/alazndy/alaz.git
cd alaz
cargo run --release -p alaz            # menüyle açılır
```

Komut satırı seçenekleri:

| Seçenek | Anlamı |
|---|---|
| `--new` | Menüyü atlayıp yeni dünya |
| `--seed N` | Tohum |
| `--size N` | Dünya kenarı (metre; 128 … 262144) |
| `--hardcore` | Ölünce kayıt silinir |
| `--demo` | Gösteri sahnesi (geliştirme) |
| `--host [--port N]` | Çok oyunculu sunucu (varsayılan port 7777) |
| `--join ADRES` | Sunucuya katıl |

Kayıtların yeri sırasıyla `$ALAZ_SAVE_DIR`, `$XDG_DATA_HOME/alaz`, `~/.local/share/alaz`.

## Kontroller

Varsayılan tuşlar. Hepsi oyun içinden yeniden atanabilir.

| Tuş | Eylem | Tuş | Eylem |
|---|---|---|---|
| W A S D | Yürü | Sol Shift | Koş |
| Space | Zıpla / vur (mini oyun) | Tab | 2D ↔ 2.5D |
| Q / E | Kamerayı döndür | Oklar | Kamerayı kaydır |
| F | Karakteri takip et | X | Dur |
| I | Çanta | C | Yap |
| B | İnşa | H | Hedefler |
| R | Rotalar | G | Izgaraya oturt |
| F1 | Yardım | F5 / F9 | Kaydet / yükle |
| M | Ses | Esc | Menü / iptal |

Fare ile:
- **Sol tık:** yürü, bina ya da işçi aç.
- **Sağ tık:** topla, devrik gövdeyi parçala, yerdekini al.
- **İmleç etiketi:** İmlecin altındaki şeyin adını ve sağ tıkın ne yapacağını gösterir.

## Oynanış

**Sıfırdan ilk gece:** Ekrandaki "İlk adımlar" listesi ve üstteki sarı hedef şeridi yolu gösterir:
1. İki taş topla, birbirine vur: keskin taş.
2. Kuru dal ve lif otu topla. Keskin taş, dal ve iki lifle taş balta yap.
3. Ağaç kes; gövde devrilir, parçala, odunları topla.
4. İki taş, dal ve liften taş çekiç yap. Çekiçle bir taş damarını kaz: arada çakmaktaşı çıkar.
5. Kamp ateşi kur, odunla besle. Barınak kur, su iç, yemiş ye.

**İhtiyaçlar:**
- **Ekranın alt köşeleri:** Sol altta sağlık ve soluk, sağ altta tokluk, su ve sıcaklık, altında üç besin grubu.
- **Beslenme:** Tek tip beslenmek, ulaşılabilecek en yüksek canı düşürür.
- **Gece:** Soğuk ve kurt getirir; ateş ikisine de iyi gelir.

**Su:**
- Nehirler akar.
- Klavyeyle yürüyerek suya girersin; sığda yavaşlarsın, derinde yüzersin.
- Derinde soluk tükenir; soluk biterse boğulursun.

## Çağlar

Her çağ kendi hedefleriyle tamamlanır; arayüz ve renkler çağla birlikte değişir.

| Çağ | Öne çıkanlar |
|---|---|
| Taş | Keskin taş, balta, çekiç, mızrak, ateş, barınak |
| Cilalı Taş | Tarım (tohum, tarla, hasat), çömlekçilik ve çukur fırın, dokuma, ağıl ve hayvan yetiştirme, depo |
| Bakır | Pota ve kil kalıplar, bakır eritme ve döküm, bakır aletler |
| Tunç | Bakır ile kalayın oranlı alaşımı, örs, testere ve tahta, araba ve saban |
| Demir | Bloomery ve körük, süngerimsi demir, şahmerdan, su gücü |
| Çelik ve Mekanik | Yüksek fırın, pik demir, çelik, yel değirmeni, taşıyıcı bant, otomat |
| Buhar | Maden kömürü, kok, buhar makinesi, pompa ile su basmış madenler, cam |
| Elektrik | Haddehane ve bakır tel, mıknatıs, dinamo, direk şebekesi, motor, ampul |
| Elektronik | Silisyum, transistör, devre kartı |
| Bilgisayar | Bilgisayar, tasarım masası (kendi tarifin), döngü: yeni dünya |

Yan dallar:
- **Jeoloji:** gizli yataklar, arama kazması, maden kuyusu, petrol.
- **Kiler:** yiyeceği yavaş bozar.
- **Sağlık:** sargı ve atel.

## Dünya

Üç dünya üreticisi var. Seçimi dünya boyutu yapar; kayıtlı dünyalar her zaman yapıldıkları üreticiyle açılır.

| Üretici | Ne zaman | Özellikler |
|---|---|---|
| v1 | Eski kayıtlar | Gürültü tabanlı arazi |
| v2 | 4 km ve altı yeni dünyalar | Nehirler (dere → nehir, denize kesintisiz) ve göller. Küçük dünyalarda çayırda maden damarları |
| v3 | 4 km'den büyük yeni dünyalar | 9 km'lik kıtalar ve okyanuslar, bükülmüş kıyılar, dağ kuşaklarında sıradağlar ve karlı zirveler, enlem kuşaklarına göre iklim (tundra, orman, çayır, çöl), uzun nehirler |

**Başlangıç yeri:** Yeni dünyalarda doğma noktası, bir yeni oyuncunun yaşayabileceği bir yer olarak aranır. Bu yerde bulunması gerekenler:
- **Hemen yanında:** çayır ya da orman; yakında ağaç, taş yığınları, dal, lif, yemiş ve su.
- **Yürüme mesafesinde:** kil ve bakır.
- **Çevresi:** ada değil, kara.

**Boyutlar:**
- 4 km'ye kadar olan dünyalar bir kerede üretilir.
- Daha büyükleri ("Devasa 32768", "Sınırsız" 262 km) 64 m'lik chunk'lar halinde, oyuncular yaklaştıkça üretilir. Arazi bellekte tutulmaz, tohumdan hesaplanıp 64×64'lük bloklarda önbelleğe alınır.

## Mimari

```
crates/
  sim/     deterministik oyun dünyası: harita, ajanlar, binalar, tarifler, fizik, kayıt (render bağımlılığı yok)
    data/rules.toml   bütün binalar, tarifler, aletler, kapılar (veriye dayalı)
    src/debug.rs      debug kapıları: oyunu satır satır süren betik dili
  render/  wgpu çizici: piksel sanatı geçişi, gölgeler, su/rüzgâr gölgelendiricileri, modeller
  net/     lockstep çok oyunculu (sunucu/istemci)
  game/    pencere, giriş, arayüz (egui), menü, hikâye, ses
```

**Determinizm:**
- Sim yalnız tamsayı kullanır (mm, permille, sabit nokta) ve saniyede 60 adım ilerler.
- Rastgelelik tohumdan karma ile gelir.
- `World::hash()` durumun özetidir. Kayıtlar ve çok oyunculu eşitleme bununla doğrulanır.

**Çizim:**
- Sim adımlarının arası çizimde ara değerlenir.
- Gölgeler, dalgalar ve rüzgâr sim'i etkilemez. Tek istisna, yüzen eşyaların rüzgârla kayması; o sim'de ve deterministik.

## Kurallar dosyası (modlama)

`crates/sim/data/rules.toml` oyunun içeriğidir. Bölümleri:
- `[[building]]`: maliyet, yarıçap, ısı
- `[[recipe]]`: girdi, çıktı, istasyon, süre, ısı, kalıp, alaşım, mini oyun adımları
- `[[tool]]`: tür, kademe, dayanıklılık, onarım
- `[[gate]]`: hangi kaynak hangi aleti ister
- bozulma ve mağara kademeleri

Kayıtlar tarifleri indeksle anar; bu yüzden **yeni tarifler dosyanın sonuna eklenir**, var olanların sırası değişmez. Bilgisayardaki tasarım masasının yazdığı tarifler o dünyanın kendi kurallarına eklenir ve kayda girer.

## Kayıtlar

Sürümlü ikili biçim (postcard), şu an sürüm 24. Eski sürümler yüklenirken yükseltilir. Kayıt yalnız değişenleri tutar:
- değişen kaynaklar
- binalar ve ajanlar
- chunk sırası
- kendi kurallar

Gerisi tohumdan yeniden üretilir. Yüklenen dünyanın hash'i kayıttakiyle karşılaştırılır.

## Çok oyunculu

```bash
cargo run --release -p alaz -- --host            # sunucu (port 7777)
cargo run --release -p alaz -- --join 192.168.1.5
```

- **Lockstep:** Her oyuncunun komutları her adımda dağıtılır, herkes aynı sim'i çalıştırır.
- **Katılma:** Katılan oyuncu dünyanın kaydını alır.
- **Döngü:** Yeni dünyayı yalnız sunucu başlatır.

## Geliştirici araçları ve debug kapıları

**Debug betik dili** (`crates/sim/src/debug.rs`). Her satır, bir oyuncunun yapacağı bir iş:

```text
gather stone 2              # en yakın taşa yürü, iki tane al
combine stone stone         # dene (yontma oyunu kusursuz oynanır)
craft stone_axe             # bir tarifi mini oyunuyla yap
build campfire              # yakında uygun yere kur (oyunun yerleşim kurallarıyla)
refuel campfire             # yürü, ateşi besle
startjob pit_kiln fire_casting_kit / deposit depot / collect depot wood / auto campfire cook_meat
eat berries / drink / walk 1 0 5 / goto 1200 800 / sprint on / jump / drop fiber 1 / pick / split
hunt deer / tame dog / feed / hitch cart / route tree / repair stone_axe / prospect
cave enter|mine|deeper|leave / design stone 1 wood 2 / cycle
tp coast|swim|cave|peak / give stone 10 / hour 21 / learn / worker bot   # yalnız debug
shot NAME / state
```

**Nerede çalışır:**
- **Oyunda:** `ALAZ_SCRIPT=betik.txt cargo run --release -p alaz -- --new`. `SHOT ad` satırları günlüğe işaret basar; bir izleyici betik bu anlarda ekran görüntüsü alabilir.
- **Ekransız:**
  ```bash
  ALAZ_PLAY_SCRIPT=betik.txt ALAZ_PLAY_GEN=2 ALAZ_PLAY_SIZE=1024 \
    cargo test --release -p alaz-sim -- --ignored play_script --nocapture
  ```

**Tarama ve rapor testleri:**

| Test | Ne yapar |
|---|---|
| `sweep_every_building_and_recipe` | Her binayı kurar, her tarifi yapar (her koşuda çalışır) |
| `every_command_has_a_working_gate` | Her oyuncu komutunu kapısından geçirir |
| `worldgen_report` (`--ignored`) | Üreticileri ölçer. Değişkenler: `ALAZ_REPORT_GEN`, `ALAZ_REPORT_SEEDS`, `ALAZ_REPORT_SIZE`, `ALAZ_REPORT_ROWS` |
| `dump_v2_map`, `dump_v3_map` (`--ignored`) | Harita görüntüsü (`ALAZ_MAP_PPM`, `ALAZ_MAP_STEP`) |
| `time_land_functions` (`--ignored`) | Arazi fonksiyonlarının hızını ölçer |

**Ortam değişkenleri:**

| Değişken | Ne yapar |
|---|---|
| `ALAZ_BENCH=30 ALAZ_NOVSYNC=1` | Performans ölçümü: kare, çizim ve sim süreleri |
| `ALAZ_PERF=1` | 5 saniyede bir performans dökümü |
| `ALAZ_DEMO_AT=coast\|swim\|forest\|peak\|…` | Gösteri sahnesinde yeri seçer |
| `ALAZ_DEMO_WALK=dx,dy` | Yürüme tuşunu basılı tutar |
| `ALAZ_DEMO_IDLE` | Gösteride el işi başlatmaz |
| `ALAZ_DEMO_HOUR`, `ALAZ_DEMO_WEATHER` | Saati ve havayı seçer |
| `ALAZ_WINDOW=x,y,w,h` | Pencere konumu ve boyutu |
| `ALAZ_MUTE` | Sesi kapatır |

## Testler

```bash
cargo test --release --workspace
```

180 civarı test var (sim 153, arayüz 21, ağ 4, çizim 1; sim testlerinin 6'sı elle çalıştırılan araçlar). Kapsadıkları:
- **Sim:** her çağın akışı, kayıt uyumluluğu (sürüm 17-24), determinizm, fizik, su ve rüzgâr, dünya üreticileri, doğma kalitesi, debug kapıları, tam tarama.
- **Diğer crate'ler:** arayüz testleri (egui_kittest), çok oyunculu ve render testleri.

## Durum ve bilinen sorunlar

Bütün çağlar oynanabilir durumda. Ayrıntılı ve güncel liste için [docs/hata-ayiklama-raporu.md](docs/hata-ayiklama-raporu.md) ve [PLAN.md](PLAN.md). Başlıcaları:
- **Doğma araması:** v3 dünyalarda birkaç saniye, kötü tohumlarda daha uzun sürebiliyor.
- **Görüntü:** Gece deniz karadan parlak, kıyıda zikzak desen var, dev ağaçlar 2.5D görünümde ekranı kapatabiliyor.
- **Uzay çağı:** Planlanıyor.
