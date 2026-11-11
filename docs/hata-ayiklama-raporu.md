# A.L.A.Z — debug kapıları, sıfırdan oynanış ve dünya üretimi raporu

11 Kasım 2026. Bu raporda üç şey var:
- oyundaki işlevlere dışarıdan erişen debug kapıları
- bu kapılarla sıfırdan bir oyuncu gibi oynanan oyun
- üç dünya üreticisinin karşılaştırmalı analizi

Bulunan sorunlar, yapılan düzeltmeler ve açık kalanlar aşağıda.

## 1. Debug kapıları (yeni)

**Betik dili** (`crates/sim/src/debug.rs`): satır satır, bir oyuncunun yaptığı gibi. Her komut işi bitene kadar bekler, reddedilirse durur, her şeyi zamanıyla günlüğe yazar.

| Komut | Ne yapar |
|---|---|
| `gather KAYNAK N` | En yakın kaynağa yürür, N tane toplar. Kurallara göre o an toplanamayanları atlar (gevşek taşı bitmiş yığın gibi). |
| `combine A B [C D]` | Eldekileri dener (yontma oyunu dahil, kusursuz oynar). |
| `craft TARİF` | Tarifi yapar; mini oyunu kusursuz oynar. |
| `build BİNA` | Yakında boş yere kurar. |
| `startjob / deposit / collect / refuel BİNA [..]` | Binaya yürür, işi yapar. |
| `eat`, `drink`, `walk dx dy sn` (basılı tuş), `goto`, `sprint`, `jump`, `drop`, `prospect` | Oyuncu eylemleri |
| `state` | Konum, arazi, sağlık, açlık, su, sıcaklık, soluk, çanta, görev |
| `shot AD` | Oyunda ekran görüntüsü işareti |
| `tp`, `give`, `hour`, `learn` | Yalnız debug: dünyayı doğrudan değiştirir, günlükte işaretlenir. |

**Nerede çalışır:**
- **Oyunda:** `ALAZ_SCRIPT=dosya`. Görüntü almak için `runscript.sh`: oyunu açar, `SHOT` işaretlerinde ekran görüntüsü alır.
- **Başsız (ekransız):** `ALAZ_PLAY_SCRIPT=dosya ALAZ_PLAY_GEN=1|2|3 ALAZ_PLAY_SIZE=.. cargo test --release -p alaz-sim -- --ignored play_script --nocapture`
- **Tam tarama:** `sweep_every_building_and_recipe`. Her binayı maliyetinden kurar; her tarifi girdilerinden yapar (istasyonda ya da elle, mini oyunu oynayarak); başarısızları nedeniyle listeler.
- **Dünya üretimi raporu:** `worldgen_report` (`ALAZ_REPORT_GEN=v1|v2|v3`). Doğma noktası ve çevresinin ölçümü.
- **Harita görüntüleri:** `dump_v2_map`, `dump_v3_map` (`ALAZ_MAP_PPM`, `ALAZ_MAP_STEP`).
- **Gösteri kancaları:** `ALAZ_DEMO_IDLE` (gösteride el işi başlatma), `ALAZ_DEMO_WALK=dx,dy` (tuşu basılı tut), `ALAZ_DEMO_AT=swim` (derin suya koy).

## 2. Sıfırdan oynanış (rehberi izleyerek)

**Sıra:** iki taş → keskin taş → dal ve lif → balta (keskin taş + dal + **iki** lif) → 3 odun → çekiç (iki taş + dal + lif) → taş damarından çakmaktaşı → kamp ateşi → barınak → ilk gece.

- İkinci üretici, 4 km: ilk ateşe **90 saniyede**, sorunsuz.
- Ateşi besleyen oyuncu ilk geceyi geçiriyor; gece bir köpek ziyaretçi geliyor.

## 3. Bulunan ve düzeltilen sorunlar

| # | Önem | Sorun | Düzeltme |
|---|---|---|---|
| 1 | **Kritik** | Yeni üreticilerde oyuncu kumsalda doğabiliyordu: 120 m içinde hiç ağaç, kuru dal, lif ya da yemiş yoktu. İlk alet yapılamıyor, oyun başlamadan tıkanıyordu. | Doğma araması: çayır ya da orman; 20 m içinde ≥4 ağaç, ≥3 taş yığını (her yığında yalnız 2 gevşek taş var, ilk aletler 4 taş istiyor), dal, 2 lif, yemiş; 30 m içinde su; 150 m içinde kil. |
| 2 | **Kritik** | Üçüncü üreticide 6 tohumun 4'ünde başlangıç küçük bir adaydı: 600 m çevrenin %81-98'i su, hiç bakır, kalay, demir ya da mağara yok. Bakır çağına geçilemiyordu. | İki aşamalı arama: önce 160 m'lik hücrelerde ucuz bir "ülke" denetimi (250 m içinde ≥%50 kara, 150 m içinde kayalık; önbelleksiz ham yükseklikten), sonra hücre içinde kaynak araması. Şimdi çevrede su %3-60, madenler 8-208 m. |
| 3 | Yüksek | 2 numaralı düzeltmenin ilk hali bir dünyayı kurmak için dakikalar harcıyordu; oyun yeni dünya açarken donardı. | Aynı iki aşamalı arama: kurulum 0,1-5,4 sn. |
| 4 | Orta | Parça parça üretilen dünyalarda açılıştaki ilk chunk'lar doğma noktasına göre üretiliyordu. Doğma kuralı değişince kayıttaki kaynak sırası tutmaz, kayıt bozulurdu. | Yüklerken ilk chunk'lar üretilmiyor, kayıttaki sıra baştan oynatılıyor (`World::for_loading`). |
| 5 | Orta | Sağ alttaki pusula ihtiyaç çubuklarının üstüne biniyordu. | Çubukların üstüne taşındı. |
| 6 | Orta | "İlk adımlar" listesi bina ve işçi panelinin üstüne biniyordu. | Panel açıkken liste gizleniyor. |
| 7 | Orta | Bina ve işçi paneli ekranın yarısına yayılıp hedef şeridini ve ipuçlarını kesiyordu. | Genişlik 280-330 piksel. |
| 8 | Orta | Derin suda yüzen karakter dalga tepelerinin altında kalıyordu. | Baş ve omuzlar dalganın üstünde. |
| 9 | Orta | Denizde büyük, yapay köpük lekeleri vardı (tepe ölçeği yanlıştı). | Köpük gerçek en büyük dalga yüksekliğine göre, yalnız rüzgârlıyken ve seyrek. |
| 10 | Orta | Ekranın çoğu su olunca kare hızı 23 fps'e düşüyordu: dalgalar her piksel için iki kez hesaplanıyordu. | Tek hesap; 60 fps. |

Tam tarama sonucu:
- **Çalışan:** 115 işlev, ilk denemede.
- **Başarısız (12):** Hepsi kurulumdan:
  - kuru yere su çarkı
  - ekinin büyüme süresi
  - körüksüz fırın
  - güçsüz makine
  - çukur fırının 4,5 dakikalık ısınması

  Gerçek bir tarif ya da bina hatası çıkmadı.

## 4. Dünya üreticileri: analiz

32 km'lik dünyalar, tohum başına. Ölçülenler:
- doğma noktası
- 30 m içindeki kaynaklar
- en yakın su, kil, maden ve mağara
- 600 m çevrenin arazi dağılımı

**v1 (eski dünyalar, değiştirilmedi):**
- Arazi her yerde benzer (%20 su, %30 çayır, %20 orman, %18 kaya).
- Madenler 5-48 m.
- 6 tohumun 3'ünde başlangıç kumsal. Orman ve ağaç yine 30 m içinde olduğu için oynanabilir.

**v2 (nehirli, 4 km'den küçük yeni dünyalar):**
- Dengeli; nehirler düzgün.
- Yeni doğma kuralıyla hep çayır ya da orman.
- Madenler genelde 10-30 m. Bir tohumda 72-93 m.

**v3 (kıtalar, 4 km ve üstü yeni dünyalar):**
- Kıta ölçeği gerçekçi: okyanus havzaları, uzun sıradağlar, iklim kuşakları.
- Asıl risk başlangıç yeriydi (madde 1-3); düzeltildi.
- Kalan farklar:
  - Yoğun ormanlarda (30 m içinde 250-390 ağaç) yürümek ve görmek zor.
  - Kil 150-170 m'ye kadar uzak olabiliyor (dar kumsallar).
  - Madenler bazı tohumlarda 120-210 m.

## 5. İkinci tur: kapsam, doğrulama ve düzeltmeler

### Kapsam: her komutun kapısı
Betik diline eklenen komutlar:
- `pick`, `split`, `hunt`, `tame`, `feed`, `hitch`, `route`, `auto`
- `cave enter|mine|deeper|leave`, `repair`, `design`
- debug: `worker`, `cycle`, `tp cave`

Oyuncunun verebileceği her komut artık bir kapıdan geçiyor. `every_command_has_a_working_gate` testi hepsini sürüp etkisini doğruluyor:
- **Hareket:** gitme, yürüme ve koşma, zıplama.
- **Toplama:** toplama, ağaç devirme ve parçalama, bırakma ve yerden alma.
- **Bakım:** yeme, içme.
- **Yapım:** birleştirme, mini oyunlu yapım.
- **Binalar:** inşa, ateş besleme, depoya koyma, alma, iş başlatma, kendiliğinden iş.
- **İşçiler:** otomat rotası ve besleme, öküze araba.
- **Hayvanlar:** evcilleştirme, avlanma.
- **Aletler ve mağara:** onarım, arama, mağaraya girme, kazma, çıkma.
- **Son oyun:** tasarım masası, döngü.

Yontma, yerleştirme, gerdirme, zamanlama ve dizme mini oyunları yapım içinde oynanıyor.

### Tarama: 127/127
İlk turdaki 12 başarısızlık doğru kurulumla tek tek doğrulandı. Hepsi kurulum eksikliğiymiş, oyun hatası değil:
- **Su çarkı:** Kıyıya kuruldu.
- **Fırınlar:** Bloomery ve yüksek fırın körük ve rüzgâr değirmeniyle çalıştı.
- **Makineler:** Şahmerdan ve haddehane rüzgâr gücüyle çalıştı.
- **Süreler:** Tarlalar ve çukur fırın ısınma ve büyüme süresi kadar beklendi.

Tarama artık her test koşusunda çalışıyor; tek bir işlev bozulursa test düşüyor. Betiğin yer araması da oyunun kendi yerleşim kuralını (`World::can_place`) kullanıyor.

### Sıfırdan oyuncu: bakır baltaya kadar
Bulunan oyun sorunları ve düzeltmeleri:

| Sorun | Düzeltme |
|---|---|
| Çanta 40 yer; aletler (keskin taş, balta, çekiç, kürek) de yer kaplıyordu. Depo ilk dolu çantada keşfediliyor ama 16 yerlik malzeme (12 odun + 4 taş) istiyor; oyuncu depoyu kuracak malzemeyi taşıyamıyordu. | Aletler, kalıplar ve pota çanta yüküne sayılmıyor (`Inventory::load`); çanta göstergesi "aletler sayılmaz" diyor. |
| Bakır balta için pota, kalıp ve eritme üç ayrı çukur fırın işiydi. Her biri soğuktan ~3,7 dk ısınma ve 6 dk iş; ~27 dk bekleme. | Pota ve balta kalıbı tek işte pişiyor (`fire_casting_kit`); çukur fırının ısınma hızı 4 → 8 °C/sn. Sıfırdan bakır baltaya ~23 dk (betikli oyuncu, cömert beklemelerle). |
| Kurtlar: 20 sn'de 9 ısırık, oyuncunun kaçacak vakti yok. | Kurt her ısırıktan sonra 6 sn geri çekiliyor; ilk iki gün ısırık hasarı yarı. Uzun süredir kırık olan `a_careful_newcomer_survives_two_days` bununla geçiyor. |
| Ateş: tıklama başına 1 odun (100 sn yanma). | Ateş besleme dolana kadar en çok 6 odun atıyor. |
| Barınak 6 odunla keşfediliyor ama 10 odun istiyordu. | Keşif eşiği maliyetle aynı: 10 odun. |
| Yemiş çürümesi | Zaten bir uyarı var: ilk bozulmada ne olduğunu ve neyin korunduğunu söylüyor. Değişiklik gerekmedi. |

Tasarım gereği olan (hata değil) zincirler:
- Çömlekçilik için önce tarım (lif otlarından arada düşen tohum) gerekiyor.
- Döküm için elde bakır cevheri ve çömlekçilik gerekiyor.
- Çekiç ilk kazmadır, bakır damarını kazar.

Rehber bunları söylüyor.

### Dünya üreticileri: genişletilmiş analiz
Her üretici için 20 tohum, beş boyutta ölçüldü: 128 m, 1 km, 4 km, 32 km, 262 km.

| Bulgu | Düzeltme |
|---|---|
| 128 m'lik yeni dünyalarda bazı tohumlarda hiç bakır, kalay ya da demir yoktu (kaya azlığı). | ≤512 m'lik yeni dünyalarda çayırda küçük maden damarları çıkıyor; 20 tohumun hepsinde üç maden var (`small_new_worlds_have_ores`). |
| "Dev 4096" yeni üreticiyi kullanıyordu: 20 tohumun 10'unda zayıf başlangıç, 6'sında ada, 400 m içinde maden yok. 4 km'lik harita 9 km'lik kıtaların içinde kalıyor. | Yeni üretici yalnız 4 km'den büyük dünyalarda; 4 km ve altı ikinci üreticide. |
| 32 km ve sınırsızda "üç maden birden" ortanca 157 m; bazı tohumlarda 400 m içinde bakır yok. | Yeni üreticide doğma noktası 200 m içinde bakır istiyor. Kalay sonra gerektiği için uzakta olabilir. |
| İkinci üretici 1 km: madenler ortanca 35 m (en kötü 82), kil en kötü 39 m. | Değişiklik gerekmedi. |
| **Son ölçüm** (düzeltmelerden sonra, 20'şer tohum) | 4 km (artık ikinci üretici): zayıf başlangıç 0, ada 0, madenler en kötü 76 m, kil en kötü 31 m, kurulum en kötü 7,9 sn. 32 km (üçüncü): zayıf başlangıç 0, ada 0, kil en kötü 181 m; bakır 200 m içinde (doğma testi doğruluyor); "üç maden birden" ortanca 157 m (kalay uzak olabiliyor); kurulum en kötü 14,4 sn. |
| Eski üretici: 20 tohumun 3'ünde zayıf başlangıç, çoğu kumsal. | Kayıtlı dünyalar bozulmasın diye değiştirilmedi. |

## 6. Açık kalanlar

1. **Görüntü (düşük):**
   - Gece deniz karadan parlak.
   - Kıyı çizgisinde zikzak desen.
   - Dev ağaçlar ekranı kapatabiliyor.
2. **Doğma araması (orta):** Üçüncü üreticide bakır şartıyla en kötü 14 saniye. Ölçüm sırasında makine meşguldü; tek başına birkaç saniye. Öneri: yükleme ekranında arka iş parçacığında yapmak.
3. **Kalay (düşük):** Üçüncü üreticide kalay doğmadan 400 m'den uzak olabiliyor. Tunç çağında oyuncu zaten keşfe çıktığı ve arama kazması yatak gösterdiği için bilerek bırakıldı.
4. **Eski üretici (bilgi):** Başlangıç kumsala düşebiliyor. Kayıtlar için bilerek bırakıldı.

