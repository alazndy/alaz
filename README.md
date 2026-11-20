# A.L.A.Z — Ape Learns Another Zing

Taş devrinden bilgisayar çağına uzanan bir hayatta kalma, keşif ve otomasyon oyunu. Ağaçtan yeni inmiş, hiçbir şey bilmeyen bir maymun-insanla başlarsın:
- iki taşı birbirine vurup keskin taş yaparsın
- ateşi, çömleği, bakırı, demiri, buharı, elektriği bulursun
- sonunda kendi kurallarını yazabilen bir bilgisayar kurarsın

Döngünün sonunda bu dünyanın izleri, bir sonraki dünyada harabe olarak durur.

Rust ile yazılmış kendi motoru var: wgpu, winit ve egui. Simülasyon tamamen deterministik ve tamsayı tabanlı; çok oyunculu kip lockstep çalışır. Platform: Linux ve Windows.

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
  - araziyle birleşen mağara ağları (meşale, kazma, cevher damarları, tavan çökmesi ve su baskını), gizli maden yatakları ve arama kazması
- **Su:** Rüzgârla şekillenen Gerstner dalgaları, akan nehirler, yüzme ve boğulma, suda yüzüp akıntıyla giden eşyalar.
- **Fizik:** Devrilen ağaçlar, yuvarlanan kayalar, düşme hasarı, zıplama.
- **Ortak rüzgâr:** Yön ve güç tek kaynaktan gelir. Otlar, yapraklar, dalgalar, bulutlar, duman ve yağmur hep aynı rüzgârla hareket eder.
- **Büyük dünyalar:** 128 m'den 262 km'ye. Büyük dünyalar Minecraft gibi chunk chunk, oyuncu yaklaştıkça üretilir.
- **Hikâye ve öğretici:** Açılış anlatısı, her çağa bir sayfa ve kendiliğinden işaretlenen "İlk adımlar" listesi.
- **Elementler:** Yağmur zemini ıslatır; çamur ve birikinti yavaşlatır, buzda koşan kayar. Kuru otlar rüzgârla yanar, yağmur söndürür, kül bir günde yeşerir.
- **Giysiler:** Her giysinin ısı, yağmur, ısırık, darbe, düşme ve ateş koruması, yükü ve aşınması var.
- **Kamp ve günlük:** Akşam ateş başında günün özeti ve yarın için hedef; günlük (J) olayları, keşifleri ve tarifleri tutar.
- **Modlanabilir görseller:** `assets/` klasörüne konan MagicaVoxel (`.vox`), glTF (`.glb`) modelleri ve PNG ikon/dokular kodla çizilenlerin yerine geçer (bkz. [assets/README.md](assets/README.md)).
- **Son oyun:** Bilgisayarın tasarım masasında kendi tarifini yazarsın; tarif, oyunun bir değer dengesi denetiminden geçer. Döngü, bu dünyanın durumundan yeni bir dünya başlatır.

## Kurulum ve çalıştırma

**Hazır paketler:** [Sürümler](https://github.com/alazndy/alaz/releases) sayfasından:
- **Windows:** `…-windows-setup.exe` kurar; `…-windows-portable.zip` kurmadan çalışır.
- **Linux:** `…-linux-x86_64.tar.gz` dosyasını aç, `./install.sh` çalıştır. Oyun `~/.local/bin` altına kurulur ve menüye eklenir; sudo gerekmez.

Kurulum ayrıntıları için sürüm notlarına bak. Paketler `.github/workflows/release.yml` ile üretilir: bir `v*` etiketi gönderilince Windows ve Linux paketleri derlenip sürüm sayfası oluşur.

**Kaynaktan:**

Gerekenler: Rust (2024 edition, güncel stable), Vulkan ya da DirectX 12 destekli bir ekran kartı sürücüsü; Linux (Wayland veya X11) ya da Windows 10/11.

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

Kayıtların yeri: `$ALAZ_SAVE_DIR`; yoksa Windows'ta `%APPDATA%\ALAZ`, Linux'ta `$XDG_DATA_HOME/alaz` ya da `~/.local/share/alaz`.

## Kontroller

Varsayılan tuşlar. Hepsi oyun içinden yeniden atanabilir.

| Tuş | Eylem | Tuş | Eylem |
|---|---|---|---|
| W A S D | Yürü | Sol Shift | Koş |
| Space | Zıpla / vur (mini oyun) | | |
| Q / E | Kamerayı döndür | Oklar | Kamerayı kaydır |
| F | Karakteri takip et | X | Dur |
| I | Çanta | C | Yap |
| B | İnşa | H | Hedefler |
| J | Günlük | Z | Eşya bırak |
| R | Rotalar | G | Izgaraya oturt |
| F1 | Yardım | F5 / F9 | Kaydet / yükle |
| M | Ses | Esc | Menü / iptal |

Fare ile:
- **Sol tık:** yürü, bina ya da işçi aç.
- **Sağ tık:** topla, devrik gövdeyi parçala, yerdekini al.
- **İmleç etiketi:** İmlecin altındaki şeyin adını ve sağ tıkın ne yapacağını gösterir.
- **Envanter penceresi:** I/C/B ya da bir makineye tıkla. Eşyayı çantadan sürükleyip makineye ya da ellere bırak; tık = 1, Shift+tık = hepsi; pencerenin dışına sürükle = yere bırak.
- **Defter:** J ile ayrı açılır, envanterle birlikte açık kalabilir; geniş ekranda pencerenin soluna yanaşır.
- **Eşya rehberi:** U, işaretçinin altındaki eşyanın sayfasını açar. Eşya seçilmemişse listeyi açar. Defter’den kaynakları, tarifleri ve inşa maliyetlerini inceleyebilirsin.
- **Keşif ağacı:** Defter’in Keşifler sekmesinde dört çağın ana yolunu ve yan dallarını izle. Ağacı sürükleyerek kaydır, tekerlekle yakınlaştır. Soluk soru işaretleri henüz ulaşamadığın keşifleri gösterir. Hedefler (H) sıradaki keşfi Defter’de açar.

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

Yeni dünyalarda geniş mağara boşlukları, tüneller ve dar kollar aynı üç boyutlu yeraltı alanından çıkar. Dünyanın yüzeyiyle kesişen ve yürünebilir devamı olan boşluklar giriş oluşturur. Bir ağ birden fazla yüzey ağzını bağlayabilir. Kazılan kaya ve çöküntüler dünya koordinatıyla kaydedilir ve her girişten aynı şekilde görünür. Çöküntü geçişi kapatır. Sağ tıklayarak temizlenir ve kazmayla daha hızlı açılır. Eski kayıtlar kendi mağara üreticisini kullanır.

128–256 m arası yeni dünyalarda üretici, başlangıca karadan bağlanan bir mağara ağzı arar. Gerekirse yüzeye açılan bir galeri bulmak için yeraltı alanını yeniden örnekler. Bu arama sırasında yüzey arazisi sabit kalır.

Mağara ağzına sağ tıklayınca karakter ağza yürür ve yanında ışık varsa içeri girer. Mağarada tıklayarak yürürken oyun su basmış geçitleri ve gereken kazma seviyesini hesaba katar.

Kayalık yamaçta tavanı sağlam kalan bir galeri, kayaya açılan bir ağız oluşturur. Açık arazide alçalan galerinin girişi, mağara tabanına inen bir geçit olarak görünür.

«Çıkışa git» yürüyerek ulaşılabilen en yakın ağzı seçer. Çöküntü rotayı kapatırsa yol yeniden hesaplanır. Keşif haritası, oyun açıkken gördüğün galerileri, çıkışları, çöküntüleri ve su basmış geçitleri hatırlar.

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
  relay-proto/  aracı ile oyun arasındaki mesajlar ve oda kodu (render ve sim bağımlılığı yok)
  relay/   internet aracısı (alaz-relay): kodla eşleştirir, bağlantıları aktarır; Linux/Unix
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

Sürümlü ikili biçim (postcard). Doğal mağaralı dünyalar sürüm 32, diğer dünyalar sürüm 30 kullanır. Kayıt başlıca şunları tutar:
- değişen kaynaklar
- binalar ve ajanlar
- chunk sırası
- kendi kurallar
- doğal mağaralarda kazılan kaya ve çöküntüler

Gerisi tohumdan yeniden üretilir. Yüklenen dünyanın hash'i kayıttakiyle karşılaştırılır.

Ana dosya eksik veya bozuk olduğunda oyun sağlam yedeğe döner. Kayıt menüsü bu yuvadan devam etmeye izin verir. Sonraki kayıt sağlam yedeği korur.

Doğal mağaralarda keşfettiğin yerler kayıt yuvasındaki `cave-map.alz` dosyasında saklanır. Yedeği açarsan o kayıt alınırken bildiğin yerleri görürsün. Keşif haritası çok oyunculu dünya verisine eklenmez.

## Çok oyunculu

**Menüden:**
- Ana menüde **Çok oyunculu** düğmesine bas.
- **Sunucu kur:** seçili yuvadaki dünyayı açar. Yuva boşsa menüdeki ayarlarla yeni dünya kurar. Port varsayılan 7777.
- **Ağdaki oyunlar:** aynı ağdaki sunucular kendiliğinden listelenir. **Katıl** ile bağlan.
- **Adresle katıl:** liste boş kalırsa sunucunun IP adresini yaz (ör. `192.168.1.5`).
- Bağlantı arka planda kurulur; hata menüde görünür.
- Portlar: oyun TCP 7777, ağda bulma UDP 7778. Windows ilk sunucu kurulumunda güvenlik duvarı izni sorar: izin ver.
- Duraklatma menüsü sunucunun adresini ve oyuncu sayısını gösterir.

### İnternetten (kodla)

Aracı sunucu çalıştığında arkadaşınla port açmadan internet üzerinden oynayabilirsin. Kuran oyuncu bir kod alır. Arkadaşı kodu yazarak katılır.

- **Ağa aç:** Çok oyunculu ekranında **Ağa aç**'a bas. `K7Q4-XM2P` gibi bir kod çıkar. **Kopyala** düğmesiyle panoya kopyalayabilirsin. Bağlı oyuncu sayısı da görünür.
- **Kodla katıl:** Arkadaşın **Kodla katıl** alanına kodu yazar. Büyük-küçük harf, boşluk ve tire önemli değildir.
- **Yeni kod:** Yeni bir kod alır. Eski kod hemen geçersiz olur.
- **Ağı kapat:** Kod geçersiz olur. Ağda olan oyuncular oyunda kalır.
- Kod, ağı açık tuttuğun sürece geçerlidir. Bir odada en çok 8 oyuncu olabilir (kuran dahil).
- Kodu güvendiğin arkadaşlarınla paylaş. Sunucu oyuncu komutlarını bağlantıya atanmış karakterle sınırlar. Oyuncular ortak işçi komutları verebilir.
- Çok oyunculu ekranından **Ağa aç** yalnız internet yolunu açar; LAN portu açılmaz. Oyun zaten LAN için açıksa ikisi de çalışır.
- Oyun bağlantıyı `relay.alazlab.com` (port 443, TLS) adresindeki aracı sunucu üzerinden kurar. Aracı oyuncuları eşleştirir. LAN ve internet oyunları aynı eşzamanlı simülasyonu kullanır.
- Aracı kod ya da oyun içeriğini kaydetmez; yalnız sayaç tutar. TLS aracıda sonlandığı için aracı trafiği teknik olarak görebilir.
- Herkes aynı oyun sürümünü kullanmalı.
- Aracının adresini `ALAZ_RELAY=host:port` ortam değişkeniyle değiştirebilirsin (test ya da kendi aracın için). Aracıyı kurma adımları: `packaging/relay/RUNBOOK.md`.
- LAN ile oynama ("Sunucu kur", "Ağdaki oyunlar", `--host`, `--join`) eskisi gibi çalışır.

**Komut satırından:**

```bash
cargo run --release -p alaz -- --host            # sunucu (port 7777)
cargo run --release -p alaz -- --join 192.168.1.5
```

- **Lockstep:** Her oyuncunun komutları her adımda dağıtılır, herkes aynı sim'i çalıştırır.
- **Katılma:** Katılan oyuncu dünyanın kaydını alır.
- **Döngü:** Yeni dünyayı yalnız sunucu başlatır. Bağlı oyunculara yeni dünyada birer karakter atanır.

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
| `dump_cave_slices` (`--ignored`) | Üç boyutlu mağara alanının yatay ve dikey kesitleri ile örnek alandaki en büyük bağlı ağın üstten görünümü |
| `seeded_cave_upload_cost` (`--ignored`) | Üç mağara yürüyüşünde çizim hazırlığının işlemci süresi ve ekran kartına gönderilen yükseklik verisi |
| `time_land_functions` (`--ignored`) | Arazi fonksiyonlarının hızını ölçer |

Mağara önizlemesi için bir çıktı klasörü oluştur ve `ALAZ_CAVE_SLICE_DIR` ile göster.
`ALAZ_CAVE_SEED` tohumu, `ALAZ_CAVE_GEN` yüzey üreticisini (`2` veya `3`),
`ALAZ_CAVE_XY_Z` yatay kesitin yüksekliğini ve `ALAZ_CAVE_XZ_Y` dikey kesitin konumunu seçer.
Kesit konumları metre cinsindendir.

```bash
mkdir -p /tmp/alaz-cave-preview
ALAZ_CAVE_SLICE_DIR=/tmp/alaz-cave-preview ALAZ_CAVE_SEED=42 \
  cargo test --locked -p alaz-sim dump_cave_slices -- --ignored
```

**Ortam değişkenleri:**

| Değişken | Ne yapar |
|---|---|
| `ALAZ_BENCH=30 ALAZ_NOVSYNC=1` | Performans ölçümü: kare, çizim ve sim süreleri |
| `ALAZ_PERF=1` | 5 saniyede bir performans dökümü |
| `ALAZ_DEMO_AT=coast\|swim\|forest\|peak\|…` | Gösteri sahnesinde yeri seçer |
| `ALAZ_DEMO_WALK=dx,dy` | Yürüme tuşunu basılı tutar |
| `ALAZ_DEMO_IDLE` | Gösteride el işi başlatmaz |
| `ALAZ_DEMO_HOUR`, `ALAZ_DEMO_WEATHER` | Saati ve havayı seçer |
| `ALAZ_DEMO_EVO=0..9` | Beden ve portreyi bu evrim koordinatında çizer (kesirli olabilir: 4.5 iki aşamanın arası); `ALAZ_DEMO_EVO_CARD=<aşama>` yeni aşama kartını gösterir |
| `ALAZ_DEMO_LINEUP=<kıyafet maskesi>[,lod\|walk\|sit]` | Oyuncunun önüne on evrim aşamasını yan yana dizer (aynı kişi: `ALAZ_DEMO_LINEUP_ID`, varsayılan 12) |
| `ALAZ_DEMO_FALL=deer\|wolf\|ox\|horse\|dog` | Kameranın önünde bu türden bir hayvan tekrar tekrar ölüp devrilir (ölüm animasyonu) |
| `ALAZ_DEMO_SURFACE=mud\|puddle\|ice\|fire\|ash` | Zemin durumlarını gösterir: yağmur sonrası çamur, birikinti, buz (`ALAZ_DEMO_HOUR=9` ve `ALAZ_DEMO_WEATHER=snow` ile), yanan çimen, kül |
| `ALAZ_RELAY=host:port` | İnternet aracısının adresi (varsayılan `relay.alazlab.com:443`); test ve kendi aracın için |
| `ALAZ_WINDOW=x,y,w,h` | Pencere konumu ve boyutu |
| `ALAZ_MUTE` | Sesi kapatır |

## Testler

```bash
cargo test --release --workspace
```

Testler simülasyonu, arayüzü, ağı ve çizimi kapsar. Simülasyon testleri çağların akışını, dünya ve mağara üretimini, fizik kurallarını ve eski kayıtların yüklenmesini denetler. Ağ testleri yerel soket açar. `--ignored` ile çalıştırılan ölçüm ve önizleme araçları normal test çalışmasına dahil değildir.

## Durum ve bilinen sorunlar

Bütün çağlar oynanabilir durumda. Ayrıntılı ve güncel liste için [docs/hata-ayiklama-raporu.md](docs/hata-ayiklama-raporu.md) ve [PLAN.md](PLAN.md). Başlıcaları:
- **Doğma araması:** v3 dünyalarda birkaç saniye, kötü tohumlarda daha uzun sürebiliyor.
- **Görüntü:** Gece deniz karadan parlak, kıyıda zikzak desen var, dev ağaçlar 2.5D görünümde ekranı kapatabiliyor.
- **Uzay çağı:** Planlanıyor.
