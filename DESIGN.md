# A.L.A.Z — Ape Learns Another Zing

Taş devrinden uzay çağına kadar kaynak yönetimi, otomasyon ve gerçekçi teknoloji ilerlemesi üzerine kurulu hayatta kalma/fabrika oyunu. Civ (çağ ilerlemesi) + Factorio (otomasyon) + Spore (ölçek) karması. Rust, hızlı ve verimli.

## Amaç

Hiçbir şeyin olmadığı bir yerden başlayıp teknolojiyi sırayla ilerleterek **bu oyunun kendisini yapmak**. Finalde oyuncu kendi kaydından yeni bir dünya başlatır (Inception hissi).

## Kararlar

| Konu | Karar |
|---|---|
| Görsel | Tepeden 2D ↔ döndürülebilir 2.5D, tek ortografik renderer (pitch 90° = 2D, ~45° + yaw = 2.5D) |
| Dünya | Prosedürel, seed'li, deterministik |
| Zaman | Sabit global saat, hızlandırma yok. Bir oyun günü ≈ 24 gerçek dakika. İşler alet/işçi/otomasyonla kendi içinde hızlanır |
| İnşa | Serbest yerleştirme + isteğe bağlı ızgara. Sim'de konumlar sabit nokta (1 birim = 1 mm) |
| Teknoloji | Genelde yaparak öğrenme (fiziksel olarak üretebildiğini keşfedersin). Özel konularda (elektronik, yarı iletken, uzay) araştırma puanı |
| Baskı | Açlık/su, hava/sıcaklık, hastalık/yaralanma. Yaban hayatı seyrek ve hafif, dikkatli olmaya itecek kadar |
| Karakter | Tek maymun/ilk insan, görünür ve doğrudan kontrol edilir. Ölürse son kayıttan devam, hardcore seçeneği dünyayı siler |
| İşçiler | Hayvan → mekanik düzenek → bot. Hepsi aynı `Command` arayüzünde, basit görev uygulayıcı |
| Bakım | Hayvan yem ister, bot enerji/yakıt ister. Bedel oyun içi kaynak |
| Çok oyunculu | Lockstep coop. Gerçek oyuncu, uzak oyuncu, bot, ileride rakip AI aynı ajan arayüzü |
| Rakip | Şimdilik yok, ileride eklenebilir (mimari buna hazır) |
| Açılış | Gerçek insanlığın gelişim yolu, hafif destekle (oyun içi ipuçları, menü yok) |
| Ses | MVP'de sadece temel efektler |
| Motor | Kendi motoru: wgpu + winit + egui (araç UI) + hecs/SoA. Sim ve render ayrı crate |
| Platform | Sadece Linux |
| MVP | Taş devri → Demir çağı, tek harita, rakip yok |
| Final | Oyuncu kendi kaydından yeni bir dünya başlatır |
| Kurallar | Bina ve tarifler `crates/sim/data/rules.toml` içinde (veriye dayalı). Kayıt bu dosyanın sağlama toplamını taşır, farklı kurallarla yapılmış kayıt reddedilir. Tarif sırası kayıtlarda indeks olarak geçer |
| İnşa | Serbest yerleştirme (daire ayak izi, 8 m erişim), `G` ile ızgaraya oturtma. Malzeme anında harcanır. İstasyonlar özerk çalışır: girdileri yükle, iş kendi kendine biter, sonra topla |
| Üretim zinciri (M4) | Odun → kömür (kömür çukuru), kil + taş → taş ocak, cevher + kömür → demir kütle. Kil sadece suya yakın kumda, demir cevheri kayalıkta nadir |
| Hayvanlar | Vahşi köpek (ormanda), öküz ve at (çimende) dolaşır; başlangıç çevresi sakin. Yemişle yaklaşıp 3 yemiş yedirince güven kazanılır ve işçi olur. Hayvanlar toplayamaz, sadece taşır. Öküz 60, at 25 (hızlı), köpek 8 taşır |
| İşçiler | Hayvan ve otomat aynı `Command` arayüzünde. Bakım: hayvan yemiş, otomat kömür tüketir; ambardan kendi kendine yer, elle de beslenir. Bakımı biten işçi hareketsiz kalır (ölmez) |
| Rotalar | Oyuncu "Rota kaydet" ile topla/yükle/al eylemlerini kaydeder, bir işçiye atar. İşçi rotayı döngüde yürütür, yapılamayan adımı atlar. Adımlar: Topla (bölgedeki en yakın kaynak), Yükle, Al, Bekle |
| Ambar ve girdi depoları | Ambar her şeyi saklar (alet hariç). İstasyonların girdi deposu vardır; "Otomatik" açıksa girdi yetince iş kendiliğinden başlar |
| Otomat | Atölyede demir kütle + odun + kömürden üretilir (yer tutucu: ileride daha ileri bir çağa taşınacak). Kapasite 40, toplayabilir |
| Av ve et (M6) | Geyik evcilleşmez, insandan 7 m'de kaçar. Mızrakla (keskin taş + 2 odun) kovalanıp avlanır: 2-4 çiğ et. Çiğ et az doyurur ve sağlığı düşürür (ölümcül değil), ateşte pişen et çok doyurur. Ateş yanmıyorsa pişirme ilerlemez |
| Demir çağı (M6) | Tezgah (odun + taş). Demir balta: ağaç 3 kat hızlı, demir kazma: taş ve cevher 2 kat hızlı. İnsan envanteri 40 |
| Hedefler | 7 hedef (ateş, barınak, evcil hayvan, kömür, demir, demir aletler, otomat). Hepsi bitince "Demir çağı tamamlandı". Sim testi bunu komutlarla baştan sona doğrular |
| Coop (M7) | `crates/net`. Sunucu (host) her tick için numaralı bir "tur" üretir (tüm oyuncuların komutları sıralı), herkese TCP ile yayar; sunucu dahil herkes aynı turları aynı `World`'e uygular. Katılan oyuncu sunucudan kayıt anlık görüntüsü alır, `Join` komutuyla dünyada yeni bir insan doğar. Sunucu her 120 tickte durum hash'i yayar, uyuşmazlık olursa istemci anlık görüntü isteyip kendini onarır. Sahte `Join` komutları yok sayılır. Güven modeli: arkadaşlar tek takım (herkes her ajanı yönetebilir). Teknoloji, hedefler, ambar ortak |
| Coop ölüm | Ölen oyuncu başlangıçta eşyasız yeniden doğar (kayıt yüklenmez, hardcore coop'ta yok sayılır). Katılımcı paylaşılan kayıt dosyasına yazmaz, F9 kapalı |
| Düzenek (M8) | Dişli (tezgah: 1 demir kütle + 2 odun). Su çarkı: akan suyun 3 kare yakınına kurulur, 6 m içindeki istasyonların işini 2 kat hızlandırır. Otomat artık 3 dişli ister, yani hayvan → düzenek → bot sırası korunur. Hedef listesine "su çarkı kur" eklendi |
| Kurt (M8) | Başlangıçtan en az 30 kare uzakta ormanda nadir. 9 m içindeki en yakın insanı kovalar (insandan biraz yavaş), 1,4 m'de 2,5 sn'de bir ısırır: -%12 can ve kanama. Yanan ateşin 7 m yakınına yaklaşmaz, mızrakla öldürülür (et verir) |
| Yaralanma ve hastalık (M8) | Kanama: kalan sürece göre canı yavaşça düşürür, ateş/barınak yanında 3 kat hızlı geçer, geçene kadar can yenilenmez. Çiğ et yemek hastalık verir (açlık ve can yavaş düşer), pişmiş et hastalık süresini yarılar. Hastalık/yaralanma tedavi zinciri (ilaç, bandaj) yok |
| Ölçek (M9) | `World::with_size`, `--size N` (varsayılan 256). Harita çok iş parçacıklı üretilir (satır bantları, sonuç tek iş parçacığıyla aynı). Kaynak düğümleri satır indeksiyle bulunur (kare başına 4 bayt tablo yok). Vahşi hayatın sadece insan/işçi çevresindeki 11x11 parçada (16 kare/parça) simüle edilir, uzaktakiler donar: tick maliyeti harita boyutuna değil ekip işine bağlı. Durum hash'i artımlı düğüm özeti kullanır. Kayıt sadece değişen düğümleri saklar. Render 32x32'lik parçalarla sadece görünen alanı üretir ve çizer (kare başına 2,5 ms mesh bütçesi, 600 parça önbellek) |
| Yürüme (M10) | `nav.rs`. Su ve bina ayak izi (yarıçap + yarım kare) engeldir, ağaç ve kaya değil (aralarından geçilir). Yol açıksa (10 m ileriye kadar) düz yürür, değilse deterministik A* (8 yön, köşe kesme yok, en çok 5000 genişleme, 40 kare marj) planlar, rota düzleştirilir. Arama sınırına takılırsa en yakın noktaya kısmi rota kurar, varınca yeniden planlar. Tick başına en çok 3 arama, fazlası bir sonraki tike bekler. Yol ajanın durumunda saklanır (kayıtta, hash'te). Binanın içinde kalan düz çıkar. Hedefe gidilemezse görev temiz biter |
| El işi mini oyunu (M11) | Anında olan el işleri artık zamanlama oyunu: bir işaretçi çubukta gidip gelir (periyot 2 sn), hedef bölgeye `Vur!` (Boşluk) ile vurulur. Konum sim tick'inden hesaplanır, yani coop ve kayıtta deterministiktir; vuruşun isabeti komutun uygulandığı tickteki konuma bakar. Tüm vuruşlar tutarsa ince iş (+fazladan parça), yarıdan fazlası tutarsa iyi, aksi halde başarısız (malzeme gider). Zar: iyi sonuç %25 ince, başarısız %20 iyi olur. Envanterdeki aletler (tüketilmez) hedef bölgeyi genişletir: örn. keskin taş yontarken taş balta +4, demir kazma +6. Yürüyünce ya da 15 sn bekleyince vazgeçilir, malzeme sadece sonda harcanır. Veri `rules.toml` içindeki `minigame` alanında: keskin taş, taş balta, mızrak, sargı. İstasyon üretimi otomatik kalır |
| Algı ve dil (M14) | Oyuncunun algısı oyunla gelişir: `perception_permille` (0..1000) keşiflerden, hedeflerden ve elde tutulmuş eşyalardan türetilir (kayda girmez, `seen` kümesi kayda girer), 6 seviye. Algı düşükken dünya renksiz, uzak ve sisli görünür (gölgelendiricide doygunluk ve sis yarıçapı algıya bağlı); hiç elle tutulmamış şeyler sıradan benzerleri gibi çizilir: cevher damarları düz taş, kil kum yığını, ot bitkisi çimen tutamı, lif çimen, dal yok sayılır. Elde tutunca gerçek görünümü açılır. Dil: görülmemiş eşyanın adı "???", seviye 0-1'de eşyalara homurtu adları ("Gro!", "Ugat!"), 2'den sonra bilinen adlar. Geliştirici bayrağı: `ALAZ_DEMO_NEWBIE=1`. Kayıt sürüm 12 |
| Görünüm (M14) | Düz kutular gitti: zemin köşe renkleri komşu karoları harmanlayıp gürültüyle bozulur ve hafif eğimlidir (karo kenarı yok); ağaç, taş, çalı, bitkiler küre ve koni gibi yumuşak şekillerle (ışık köşelere pişirilmiş, gölgelendiricide normal yok). Karakter kemikli uzuvlarla: bacak ve kollar dizden, dirsekten bükülür, yürüyüş ve her iş kendi hareketini yapar. Eşya ikonları kodla vektör çizilir (egui painter). Arayüz: sıcak kahverengi tema, yuvarlak paneller. Mini oyunlarda gerçek parçalar çizilir: dal, taş başlığı, lifle sarma. Hayvanlar (köpek, öküz, at, kurt, geyik) ortak `push_quad` ile yuvarlak parçalardan kurulur, bacakları yürüyüşle sallanır; bot ve ateş/kömür ocağı/ocak/barınak yuvarlatıldı. Ambar, atölye, tezgah, su çarkı hâlâ kutu. Dil katmanı (`lang.rs`): algı seviyesi ≤1 iken sekmeler ve hedef başlığı yalın sözcükler (Torba, Kur, Dilek, Yol), ipuçları yalnız ilk cümleye kısalır |
| Çok adımlı el işi (M13+) | Tarifler `steps` listesiyle birkaç iş ardışık oynanır; her adımın kendi mini oyunu var: zamanlama (Boşluk), taş yontma (nişan + güç), yerleştirme (parçayı yeşil halkaya tıkla: `PlaceAt`), sıkıştırma/bağlama (basılı tut, bandın içinde bırak: `Tighten`). Kalite tüm adımların ortalamasından çıkar. Malzeme sadece sonda harcanır. Taş balta: keskin taş + dal + 2 lif; adımlar sapı düzelt, yuva aç, taşı yerleştir, lifle bağla. Mızrak ve sargı da adımlı. Kıskaç (2 dal + 2 lif) yapılan, tüketilmeyen bir alettir ve yerleştirme ile sıkıştırma adımlarında hedefi genişletir: parçaları senin yerine tutar. Yeni eşyalar: dal, lif, kıskaç. Yeni kaynak: yerdeki dal ve uzun otlar (lif). İş animasyonları render tarafında: ağaç kesme ve kazma (alete göre balta ya da kazma sallanır), toplama ve kazı için eğilme, su içme, çekiçleme, mızrak fırlatma, hayvana yem uzatma; vuruş anında talaş, kıvılcım, çamur parçaları saçılır. Kayıt sürüm 11 |
| Kontroller ve arayüz (M13+) | Kamera varsayılan olarak karakteri takip eder (yumuşak), elle kaydırınca serbest kalır; Boşluk ya da F geri takibe alır. Orta tuşla sürükleyerek de kaydırılır. Pencere odağı kaybolunca basılı tuşlar temizlenir. Tüm klavye eylemleri `keys.cfg` ile atanabilir (Menü > Tuşlar; her eylem için 2 tuş; bir tuş başka eylemden alınırsa oradan silinir). Arayüz: sol üstte durum kartı, üstte sıradaki hedef, altta eylem çubuğu (Çanta, Yap, İnşa, Hedefler, Rotalar, Yardım + Su iç, Ye, Sargı, Ot), sekmeler çubuğun üstünde açılır. Karakterin rotası (yol noktaları, akan noktalar, bayrak) çizilir; seçili işçinin rotası da. Yürüyüş animasyonu render tarafında hesaplanır (yön, adım fazı, bacak ve kol sallanması), sim'e dokunmaz |
| Bronz çağı (M13) | Kayalıkta bakır (yaygın) ve kalay (nadir) cevheri. Ocakta 3 bakır + 1 kalay + 2 kömür → 2 bronz. Tezgahta bronz balta (ağaç %40 süre) ve kazma (taş ve cevher %60 süre); demir hâlâ daha hızlı. Keşifler: alaşım (bakır ve kalay elde, ergitme biliniyorsa), bronz işi. Hedefler: bronz üret, bronz alet yap, BRONZ ÇAĞI. Demir çağı hedefleri ayrı kalır. Bronz balta taş yontmada hedefi genişletir. Kayıt sürüm 10, bilinen keşifler u32 |
| Taş yontma (M12) | Keskin taş ve taş balta `knap` türü: taşın kenarına fareyle nişan al, basılı tutarak gücü ayarla (güç çubuğu gidip gelir), bırakınca `StrikeAt{açı,güç}` gider. Sim açı ve güç farkına bakar: tam isabet 100, hedef içinde 60, kıl payı 25 puan; ortalama ≥85 ince, ≥45 iyi. Mızrak ve sargı zamanlama türünde (Boşluk). Yanlış girdi türü yok sayılır. Alet bonusu derece olarak hedefi büyütür |
| Menü ve yuvalar (M12) | Bayraksız açılışta ana menü, Esc ile duraklatma menüsü (solo'da sim durur). 3 kayıt yuvası: yuva 1 eski kayıt dizini, 2-3 `slotN/`. Yuva bilgisi `peek_save` ile okunur. Bayraklar (`--new`, `--seed`, `--demo`, `--host`, `--join`) menüyü atlar |
| Ses (M12) | Kodla sentezlenen efektler (vuruş, ıskalama, keşif, başarı, ısırık...) `pw-cat` ya da `aplay` ile akıtılır. Dosya yok. M tuşu sessize alır, `ALAZ_MUTE=1` kapatır |
| Tedavi (M11) | Şifalı ot (çimen ve ormanda nadir bitki). Çiğnenince hastalığı 3000 tick azaltır. İki otdan sargı (mini oyun, aletler kolaylaştırır). Sargı kanamayı durdurur |
| Kayıt | `ALAZ` + sürüm + sağlama toplamı + postcard. Harita seed'den üretilir, sadece değişiklikler saklanır. Kayıt sim hash'ini de içerir, yüklemede doğrulanır. Atomik yazma, `.bak` yedeği. Otomatik kayıt 2 dk, F5/F9, çıkışta kayıt |
| Ölüm | Varsayılan: son kayıttan devam. `--hardcore`: kayıt silinir, yeni dünya (seed+1) |

## Ana plan

Çağların sırası, çapraz sistemler, yakın vade iş listesi, iş bölümü ve açık kararlar: `PLAN.md`. Referans analizi: `docs/referans-analizi.html`.

## Mimari ilkeleri

1. **Sim render'dan bağımsız.** `alaz-sim` hiçbir grafik/pencere bağımlılığı içermez. Render sadece okur.
2. **Determinizm.** Sabit adım (60 tps), sabit nokta tam sayılar, komutlar sıralı. `World::hash()` ile desync tespiti.
3. **Her şey komutla.** Oyuncu, bot, hayvan, uzak oyuncu `Command` kuyruğuyla eylem yapar.
4. **Veri odaklı gerçekçilik.** Malzeme, enerji, tarif değerleri RON/TOML dosyalarında, koda gömülü değil.
5. **Chunk'lı dünya.** Sadece aktif chunk'lar tam simüle edilir, uzaktakiler toplu hesaplanır.
6. **Kayıt = snapshot.** Seed + durum, finaldeki "kaydı içeride tohum yap" fikrinin temeli. Format sürümü `alaz_sim::save::FORMAT_VERSION` (şu an 8); alan eklenince artırılmalı ve eski sürümler için göç yolu yazılmalı.

## Repo yapısı

```
crates/sim      alaz-sim     deterministik simülasyon çekirdeği
crates/render   alaz-render  kamera ve çizim (wgpu eklenecek)
crates/game     alaz         ikili: sim + render + girdi
```

## Açık kalanlar

1. Mekanik düzenek (su değirmeni vb.) ve hayvan → düzenek → bot hattı. (Bakır/kalay bronz çağı MVP'de atlandı: taş → demir.)
2. Tedavi zinciri sadece ot ve sargı; ilaç, kırık/çıkık, enfeksiyon yok.
2d. Mini oyun sadece el işlerinde; istasyon tarifleri otomatik. Bot/işçi mini oyun oynamaz (becerisiz sayılır). Bitmiş ürünün kalitesi envanterde tutulmaz, sadece miktara yansır.
2b. Köpek hâlâ yalnızca yemişle evcilleşiyor. Kurt tek yırtıcı, sürü davranışı ve işçilere saldırı yok (sadece insanlara). Mekanik güç sadece su çarkı; rüzgar/buhar ve güç iletimi yok.
2c. Ajanlar birbirini engellemez (üst üste binebilir), ağaç/kaya engel değil, vahşi hayvanlar (geyik, kurt) bina ve yol bulmayı dikkate almaz. Bina yıkma/taşıma ve rotasyon yok.
3. Hayvan evcilleştirme türleri (köpek, öküz, at) ve yem döngüsü.
4. Mevsim uzunluğu (1 gün = 24 dk ile yıl kaç gün olacak, ayarlanabilir sabit olsun).
5. Araştırma puanı neyle üretilir (özel konular).
6. Final sahnesinin ayrıntısı ve kayıt dosyasının içeride nasıl tohumlandığı.
7. Ağ katmanı: lockstep komut dağıtımı, gecikme tamponu, kayan nokta politikası.
8. Ses ve müzik planı (MVP sonrası).
9. Çoklu kayıt yuvaları ve ana menü (şimdilik tek yuva, `--new`/`--seed`/`--hardcore` bayrakları).
10. Otomatik kayıt tehlikede (tokluk/su/sıcaklık < %20 ya da sağlık < %30) atlanır; elle kayıt ve çıkış kaydı atlamaz.
12. Coop: istemci komutları sunucu turuna girene kadar gecikmeyle uygulanır (yerel tahmin yok); sunucu yavaşlarsa herkes yavaşlar; bağlantı koparsa istemci yerel kopya olarak kalır; NAT/port yönlendirme elle; şifreleme ve oyuncu kimlik doğrulaması yok.
11. Yabani hayvan güveni zamanla azalır (5 sn'de 1) ama dengesi test edilmedi. Hastalık ve yırtıcı tehdidi hâlâ yok.

## Yol haritası

1. **M0 (şimdi):** Workspace, deterministik tick, komut arayüzü, testler. ✔
2. **M1:** winit + wgpu pencere, tek kamera (2D/2.5D geçişi), ajan çizimi. ✔
3. **M2:** Prosedürel harita, kaynak düğümleri, toplama ve envanter. ✔
4. **M3:** İhtiyaçlar (açlık/su/sıcaklık), ateş, ilk aletler, yaparak öğrenme sistemi. ✔
5. **Kayıt sistemi (M3.5):** sürümlü ikili format, atomik yazma, yedek, otomatik kayıt, ölüm kuralları. ✔
6. **M4:** İnşa (serbest + ızgara), tarif sistemi, ilk üretim zinciri. ✔
6. **M5:** Hayvanlar, işçiler, rotalar, ambar, otomat. ✔ (mekanik düzenek ertelendi)
7. **M6:** Demir çağı içeriği (tezgah, demir alet, avcılık, pişirme, hedefler), MVP cilası. ✔
14. **M13:** Bronz çağı (bakır, kalay, bronz, bronz aletler). ✔
13. **M12:** Taş yontma, menü, yuvalar, ses. ✔
12. **M11:** El işi mini oyunları ve tedavi zinciri (şifalı ot, sargı). ✔
11. **M10:** Çarpışma ve yol bulma: göl ve binaların etrafından dolaşma. ✔
10. **M9:** Ölçek ve performans: ayarlanabilir harita boyutu (32 - 16384 kare), parçalı çizim, aktif bölge simülasyonu, ölçüm araçları. ✔
9. **M8:** Düzenek (dişli, su çarkı) ve tehditler (kurt, yaralanma, hastalık). ✔
8. **M7:** Lockstep coop. ✔ (TCP, sunucu tur dağıtır; internet/NAT, kopma sonrası geri katılma ve sesli/yazılı sohbet yok)

## Geliştirici bayrakları

- `--new`, `--seed N`, `--hardcore`: yeni dünya.
- `--demo`: bol malzemeli, tüm binaların kurulu olduğu ve kömür çukurunun çalıştığı test dünyası (görsel doğrulama için).
- `--host [--port N]`: coop sunucusu (varsayılan 7777, tüm arayüzler); `--join ADRES[:port]`: sunucuya katıl.
- `--size N`: yeni dünyanın kenar uzunluğu (kare). Büyük haritalar için aşağıdaki Performans bölümüne bak.
- `ALAZ_PERF=1`: 5 sn'de bir kare istatistiği. `ALAZ_BENCH=<sn>`: kamerayı haritada uçurur, özet yazar, çıkar (kayıt yazmaz). `ALAZ_NOVSYNC=1`: vsync kapat.
- `cargo run --release -p alaz-sim --example bench -- 256 4096 8192`: sim ölçeği (üretim, tick, hash, kayıt, yükleme, bellek).
- `ALAZ_SAVE_DIR`: kayıt klasörünü değiştirir. `ALAZ_AUTOSAVE_TICKS`: otomatik kayıt aralığı.

## Performans (M9 ölçümleri)

Makine: 8 çekirdek, 15 GB RAM, tümleşik ekran kartı, Wayland (vsync'i kapatmak compositor yüzünden 60 fps sınırını kaldırmıyor, bu yüzden render CPU süresi ve p99 kare süresine bakılır). Uçuş testi: kamera hızlı gezer, döner, yakınlaşıp uzaklaşır; sim'de 40 işçi çalışır.

| Kenar (kare) | Kare sayısı | Açılış/yükleme | Bellek (tepe) | Tick ort. | Hash | Kayıt | Render CPU ort. | Kare p99 |
|---|---|---|---|---|---|---|---|---|
| 256 | 0,07 M | 5 ms | ~190 MB* | 0,02 ms | 0,01 ms | <1 ms | 0,6 ms | 17,3 ms |
| 1024 | 1 M | 25 ms | | 0,10 ms | 0,3 ms | 1 ms | 0,6 ms | 17,5 ms |
| 4096 | 17 M | 0,4 s | 233 MB* | 0,03 ms | 0,5 ms | 10 ms (1,2 MB) | 0,65 ms | 17,2 ms |
| 8192 | 67 M | 1,6 s | 407 MB (sim) | 0,03 ms | 2 ms | 30 ms (5 MB) | 0,7 ms | 17,2 ms |
| 16384 | 268 M | 7-8 s | 1,3 GB (sim) | 0,04 ms | 8,5 ms | 131 ms (21 MB) | 0,65 ms | 17,1 ms |

\* oyun süreci (GPU tamponları dahil). Hepsi 60 fps.

Ölçüm öncesi ilk sürümde: tick maliyeti harita boyutuyla doğrusal artıyordu (8192'de 6 ms), tüm harita tek tampondaydı (4096'da ~2,8 GB), düğüm tablosu kare başına 4 bayttı (16384'te 1 GB). Hepsi M9'da giderildi.

Kalan sınırlar: 16384'te yükleme 8 s (harita her yüklemede yeniden üretilir), kayıt/coop katılım anlık görüntüsü 21 MB (vahşi hayvan listesi tam saklanıyor) ve ana iş parçacığında 131 ms'lik takılma yapar, coop hash'i 120 tickte bir 8,5 ms sürer. Rahat sınır: 4096-8192.

## Yapımcı gözüyle yeniden tasarım (öneri, henüz uygulanmadı)

### Teşhis
Sistem çok, oyun az. Elimizde taş devrinden bronza zincir, algı, mini oyunlar, rotalar var; ama oyuncunun saniye saniye yaptığı şey "tıkla, panel aç, tarif seç, mini oyun penceresi". Bu yüzden çocuksu ve kopuk hissettiriyor.
1. **Dünyadan kopuk arayüz:** Mini oyunlar ekranın ortasında modal pencere. Oyuncu karakteri ve taşı dünyada görmüyor.
2. **Tarif listesi keşfi öldürüyor:** Listeden seçmek "yaparak öğrenme"nin tersi. Algı sistemi bile menüde "???" gösteriyor.
3. **İlk 10 dakikanın yönü yok:** Baskı (açlık, soğuk, kurt) var ama kamp, yön ve küçük zaferler yok.
4. **Derinlik yerine genişlik:** Bronz çağı yazıldı ama taş devri henüz eğlenceli değil.

### Yeni çekirdek döngü: Gözle, dene, öğren
- **Elde birleştir, listeden seçme:** Eline iki şey al (taş + taş, dal + taş + lif); sonuç dünyada, karakterin elinde ve yerde olur. Tarif listesi sadece öğrenilenlerin defteri.
- **Mini oyun dünyada:** Kamera yakınlaşır, karakter çömelir, vuruş fareyle taşın üstünde yapılır, panel yok. İş hissi ve algı aynı sahnede.
- **Algı = bilgi:** Algı ilerledikçe dünyada yeni şeyler fark edilir (iz, damar, ot); defter dolar; dil gelişir. Menü yerine dünya öğretir.
- **Kamp merkezi:** Ateş etrafında topluluk büyür: ilk yoldaş, ilk iş bölümü, sonra rotalar ve botlar. Rotalar artık bir "büyük oyun" aşaması.

### Yol haritası
- **R1 (çekirdek):** Elde birleştirme + dünyada yontma/bağlama, panellerin kaldırılması. En riskli ve en değerli adım.
- **R2:** Deftere dayalı keşif, algıya bağlı dünya detayları.
- **R3:** Kamp ve yoldaşlar; ilk 10 dakika senaryosu.
- **R4:** Mevcut bronz/demir zincirini bu döngüye taşı.

Sim çekirdeği (deterministik, kayıt, koop) korunur; değişen şey komutlar ve sunum.

### R1 durumu (uygulandı)
- **Elde birleştirme:** `CommandKind::Combine{items:[u8;4]}` (255 = boş el). Elde tutulanlar bir elle yapılan tarifin girdileriyle birebir uyuşursa tarif öğrenilir ve işlem başlar; uyuşmazsa `Reject::NoIdea`. Yapabildiklerin sekmesi artık "Ellerin" (4 yuva, çantadan tıkla, Dene) + altta Defter.
- **Mini oyun dünyada:** Panel karakterin kafasının üstüne çıkar (`Camera::project`), kamera işin süresince yakınlaşır ve sonra eski haline döner. Mini oyun sürerken bina/işçi panelleri gizlenir.
- **Eksik:** Panel hâlâ büyük ve kutu gibi; işlenen taş ve sap karakterin elinde dünyada çizilmiyor; istasyon işleri (ocak, tezgah) elde birleştirmeye taşınmadı.

### R2 durumu (ilk dilim)
- **Defter ipuçları:** Elle yapılan bir tarifin tüm girdileri görüldü ama tarif öğrenilmediyse, defterde adsız ve adımsız soluk bir girdi çıkar (girdi ikonları + "Bunlar bir arada işe yarar gibi. Elinde dene."). Liste yok, sadece dürtme.
- **Algıya bağlı iz:** Algı seviyesi 2 ve üstünde vahşi hayvanların arkasında pati izleri çizilir (kurtlarınki daha koyu). Yön hayvan kimliğinden türer, durum sim'de saklanmaz.
- **Gidiş yönü:** Yürüyen hayvanın izi hedefinin tersine, arkasına düşer (`Wild::heading_mm`); duran hayvanda kimlikten türeyen sabit yön.
- **Fark edilen ayrıntılar:** Algı 3+ iken henüz tutulmamış cevher kayasında soluk bir parıltı, algı 2+ iken tutulmamış ot kümesinde soluk bir benek görünür (mesh önbelleği algı seviyesine de bağlı).
- **Eksik:** İz ve parıltının ekran görüntüsüyle doğrulanması (pencere başka ekranlarla örtüşüp yakalama güvenilmez oldu), kil/odun gibi diğer ayrıntılar.

### R3 durumu (ilk dilim: kamp çekim merkezi)
- **Ateşe gelen köpek:** Yanan bir ateşin 6 m yakınında biri oyalanırsa, saniyede bir tohum+zamandan türeyen bir zarla (1/40 şans, yani ortalama kırk saniye başında kalınca) karanlıktan bir köpek ateşin 11 m uzağında belirir. Yalnızca bir kez olur (`Milestone::Visitor`, hedef listesinde görünmez, sadece "ziyaret oldu" bitidir; ek durum kaydı yok, FORMAT_VERSION değişmedi). Köpek zaten evcilleştirme sistemine bağlı: böğürtlen ver, güven kazan, yoldaş olsun. Companion hedefi yapılmışsa ziyaret olmaz.
- **İpucu:** "Karanlıktan bir köpek ateşinin ışığına yaklaştı. Yanına git, böğürtlen ver..."
- **Testler:** 3 tohumda köpek gelir, ateşe 12 m'den yakındır, ikinci ziyaret olmaz; boş kampa ziyaret olmaz.
- **Eksik:** İş bölümü (köpeğe/öküze kamp görevi), insan yoldaş, ilk 10 dakika senaryosunun yönlendirmesi (hedef sırası, ilk gece koruması), kamp ateşi etrafında oturma/uyku, ziyaretin görsel/ses vurgusu.

- **İlk 10 dakika rehberi:** Üstteki hedef bandı artık oyuncunun elindekine göre sıradaki somut adımı söyler (`hud::guide_text`): taş topla → iki taşı ellerinde dene → dal ve lif → baltayla odun → İnşa'dan ateş. Susuzluk/açlık/üşüme acilleri öne geçer. Ateş kurulunca sıradaki hedef metnine döner. Sim'e dokunmaz, her şey eldeki eşyadan türer. Testi: `the_guide_walks_a_newcomer_from_stones_to_the_first_fire`.
- **İlk gece:** İlk gün (24 dk, 08:00'den ertesi sabaha) kurt saldırganlığı zaten kapalı; yani ilk gece korumalı. Gerçek tehlike 2. akşam başlıyor; ateş kurtları uzak tutuyor.

- **Köpek nöbeti:** Evcilleştirilmiş köpek durduğu yerde kurtları ateş gibi (7 m) korkutur. Kamp köpeği ve yoldaş olarak ilk gerçek faydası bu. Test: `a_tamed_dog_keeps_wolves_off_its_master`. Emri (rotası) olmayan köpek, en yakın insan 8 m'den uzaklaşınca ona doğru koşar ve yanında durur (`DOG_FOLLOW_MM`), böylece koruma yanında gelir. Rotası olan köpek yerinde kalır. Test: `a_dog_without_orders_follows_its_person_but_one_with_a_route_stays`.

- **Ziyaret vurgusu:** Köpek geldiğinde hedef fanfarı yerine iki kısa havlama (`Sfx::Bark`) çalar; evcilleştirilene kadar köpeğin çevresinde nabız gibi atan soluk altın bir halka durur (`Visitor` var, `Companion` yok iken). Ses ve görünüm doğrulanmadı, sadece derlendi.

### Oyun testi turları (bot ile)
Sim üstünde iki bot oynuyor (`tests.rs`): baştan hiçbir şey bilmeyen bir yeni gelen (taş, dal, lif toplar; ellerde birleştirir; mini oyunları oynar) ve temkinli bir hayatta kalan (su, yemek, ateş, odun stoku).
- **Bulunan ve düzeltilen:** (1) Ölüp doğduğun yerde kurtlar bekliyordu: 2 günde 42 ölüm döngüsü. Artık doğuşta yakındaki kurtlar uzaklaştırılıyor. (2) Kurtlar gündüz de 9 m'den saldırıyordu: 385 ısırık. Artık gündüz 3,5 m, akşam ve gece 9 m. (3) Menüden başlayan yeni oyunda eski ipucu vardı ve kısaltma "Sol tıkla" kısmını kesiyordu; ipuçları yeniden yazıldı, Yap sekmesini (elde birleştirme) gösteriyor. (4) Dolu çanta testi ölümle temizleniyordu; test düzeltildi.
- **Kalan:** 4 tohumdan 1'inde (seed 4) akşam kurtlarıyla temkinli bot yine ölüyor; ilk gece korumasız oyuncu için sert. Test "en fazla 1 ölüm" diyor. Gerçek oyuncu tepkisi ve arayüz girdisi bot ile test edilemiyor.

### Arayüz testi (egui_kittest)
Gerçek ekranlar başsız çalıştırılıp sahte fare girdisiyle oynanıyor (`hud.rs` `ui_tests`): Yap sekmesine tıkla, çantadan taşı iki kez al, Dene; yontmada fareyi taşın kenarına götür, bas, tut, bırak (vuruş açısı ve gücü doğrulanıyor); el yuvasına tıklayıp boşalt; tam bir taş balta baştan sona (yontma, düzeltme, yuva, yerleştirme, sıkıştırma) fareyle yapılıyor. Erişilebilirlik etiketleri (yuvalar, "Yontma taşı", "Yerleştirme alanı", "Sıkıştırma alanı") bu testlerin tuttuğu yerler.
- **Kurtlar:** İlk gün (24 dk) kurtlar sadece 3,5 m'den saldırır; ondan sonra gündüz yine 3,5 m, akşam ve gece 9 m. Temkinli bot 4 tohumda hâlâ bir tanesinde ölüyor (kurda yürüyerek).

- **Güncelleme (son tur):** Dolu çantayla toplama ve ulaşılamayan hedef artık sessiz değil (`Reject::PackFull`, `Reject::Unreachable`, ekranda ipucu). Bot testinde bulunan ve düzeltilen gerçek oyun sorunu bu. Temkinli bot dört tohumda da iki gün ölmeden yaşıyor; test artık sıfır ölüm istiyor. Kalan ölümlerin hepsi botun kendi hatasıydı (kurt avlanma saatinde dışarı çıkmak, dolu çantayla yemiş toplamaya devam etmek).
- **Ekran görüntüsüyle doğrulanan düzen:** Mini oyun paneli çalışanın yanında (sağda, yer yoksa solda), 390 px'e kadar genişler, uzun yazı satır atlar; karakter her zaman görünür. Mini oyun sürerken hedef şeridi ve ipuçları gizlenir.

### Zorluk ölçümü (insan gibi hata yapan oyuncu)
`minigame_difficulty_is_reasonable_for_human_like_players`: nişan, güç ve yerleştirmede hata payı olan, tepkisi gecikmeli bir "sıradan" ve bir "çalışkan" oyuncu her tarifi 80 kez oynuyor. İlk ölçüm çok kolaydı (sıradan oyuncu neredeyse hiç başarısız olmuyor, çoğu "ince iş" çıkarıyordu). Kalite eşikleri yükseltildi: ince iş ortalama 92+, iyi 60+. Şimdi sıradan oyuncu yontmada %15 başarısız, %20-45 ince iş; çalışkan oyuncu %85-95 ince iş. Hata payları tahmin; gerçek oyuncuyla doğrulanmadı.

### Oyun kaydı (gerçek veriyle ayar)
Tek kişilik oyunda her vuruş sonucu, yapım kalitesi, ret nedeni, ısırık, ölüm, keşif ve hedef `playlog.txt` dosyasına (kayıt klasöründe, 1 MB'a kadar) yazılır. Zorluk ayarı bundan sonra bu dosyadaki gerçek oyuna göre yapılmalı; bot testindeki hata payları tahmindir.

### Piksel görünümü (Dead Cells yaklaşımı)
- **Karar:** Döndürülebilir yörünge kamerası (2D ↔ 2.5D, serbest yaw) korunur. Dünya alçak poligonlu 3B modellerden kurulur ve ekranın 1/scale boyutundaki bir resme çizilip tam sayı katıyla büyütülür (`Camera::scale()` 2..6, 1 texel = 1/16 m). Ara çözünürlük ve sert büyütme pikseli, cel gölgeleme ve kontur görünümü verir.
- **Modeller** (`shapes.rs`, `lib.rs`): düz yüzlü `limb`/`sphere`/`rock`/`push_box`; her yüzün albedo rengi köşe renginde, ışık seviyesi `uv.x`'te. Gölgelendirici ışığı 4 banda keser (parlak/sıcak, normal, gölge/soğuk, koyu): `shader.wgsl::band`.
- **Zemin:** Atlas dokusundan (16x16 boyalı karolar, `sprites.rs`), texel dünya konumundan okunur, süzme yok. Gölge ve ateş ışığı yere yatan atlas decal'ları.
- **Kontur** (`outline.wgsl`): küçük resim çözünürlüğünde, arkasındaki komşu belirgin uzaksa o texel mürekkep olur; yakın nesne bir texel ile çevrilir. Sonra `pixel.wgsl` büyütür ve hafif renk tonu/vinyet uygular (`ALAZ_NOCINEMA=1` kapatır).
- **Piksel kayması:** Kamera hedefi görünüm eksenlerinde texel ızgarasına oturtulur (`Camera::snapped`), kalan kesir büyütmede ekran pikseli olarak uygulanır; kayarken pikseller sabit kalır. Dönerken yüzeylerin texel hizasından kayması kaçınılmaz.
- **Dev aracı:** `ALAZ_ATLAS_PNG=yol.png cargo test -p alaz-render dump_atlas` atlası PNG yazar. Not: `sprites.rs` içindeki insan/hayvan/bina sprite boyaları (önceki 2D denemeden) artık kullanılmıyor, zemin, gölge ve ışık havuzu dışında.
- **Model dokuları:** Modellerin yüzeyleri dört boyalı 16x16 malzeme karosundan texel texel okunur (`sprites.rs::mat_*`, atlasın altında sabit şerit, `MAT_Y`): kereste/kabuk, yaprak, taş, deri-kumaş. Malzeme köşe renginden seçilir (`lib.rs::material_of`), doku yüzün baskın eksenine göre üç düzlemli yansıtılır, nesnenin kendi çıpasına göre hesaplanır (nesneyle birlikte durur; yürüyen uzuvlarda hafif kayabilir). Doku bir çarpandır (128 = 1.0), cel bantları üstüne biner. Parlayan yüzeyler dokusuzdur.

## Çağ içeriği (Kasım 2026)

- **Sıra tarihe göre:** Taş → Cilalı taş (tarım) → Bronz → Demir. Bakır ve kalay ocakla açılır (`Alloying` artık `Furnace` ister). Demir cevheri bronz işi bilinmeden bir şey öğretmez (`Smelting` artık `Bronzework` ister). `Milestone::ALL` bu sırada; kayıtlar hedefleri enum bitiyle tuttuğu için sıra serbestçe değişebilir.
- **Cilalı taş, ilk dilim (tarım ve ekmek):** Uzun otun son tutamı tohum verir (`FiberPlant`, kalan 1). Tohum elde ve taş balta biliniyorsa *Tarım* keşfi gelir: çapa (keskin taş + 2 dal + lif, 3 adımlı el işi), tarla (2 odun, çapa elde olmalı, harcanmaz). Tarlada "Ek ve büyüt": 2 tohum → 14400 tick (4 dk) sonra 6 buğday + 3 tohum; tarla kendini besler. Buğday elde ve ateş biliniyorsa *Öğütme*: öğütme taşı (3 taş, yontma), un (3 buğday → 2 un, öğütme taşı elde), ateşte ekmek (1 un → 1 ekmek). Ekmek 5000 doyurur (pişmiş et 4500), hasta etmez.
- **Hedefler:** İlk hasat, Ekmek pişir, CİLALI TAŞ ÇAĞI. Milestone bitleri u16 içinde 16'nın 16'sı doldu: sonraki çağdan önce `milestones` u32'ye çıkmalı (kayıt sürümü ile).
- **Kayıt uyumu:** Eşyalar sadece sona eklenir; eski kayıttaki kısa eşya dizileri boş slotla tamamlanır (`items_serde`). Tarifler de sona eklenir. Ancak `rules.toml` sağlama toplamı değiştiği için eski kayıtlar hâlâ reddedilir (mevcut karar).
- **Arayüz:** Rehber (`guide_text`) tarım ve ekmek adımlarını sırayla söyler. Defterde tarım ipuçları tohum, öğütme ipuçları buğday görülene kadar çıkmaz.
- **Çanak çömlek:** Kil elde (2+), ateş ve tarım biliniyorsa *Çömlekçilik*. Ham çömlek: 3 kil, 3 adım (yoğur, kil halkaları üst üste koy = yığma oyunu, yüzeyi düzle). Kamp ateşinde 900 tickte pişer. Su içerken susuzluk dolunca çantadaki boş çömlekler dolar; dolu çömlekten her yerde içilir (+3500), çömlek boş geri gelir. Arayüzde düğme "Su iç, çömlek doldur" / "Çömlekten iç". Hedef biti kalmadığı için ayrı hedef yok: CİLALI TAŞ ÇAĞI artık hasat + ekmek + pişmiş çömlek ister; rehber çömlek adımlarını ekmekten sonra söyler.
- **Dokuma:** 6 lif elde, giysi ve tarım biliniyorsa *Dokuma*. Dokuma tezgahı (4 odun + 2 kil ağırlık) 4 lifi 1200 tickte kumaşa çevirir (istasyon, otomatik çalışabilir). Elde: dokuma giysi (2 kumaş + lif, sıcaklık +20, lif örtü +10) ve dokuma torba (kumaş + 2 lif). Torba insanın taşıma sınırını 15 artırır (`BAG_EXTRA`, `Agent::capacity()`); torba ve giysi alettir, ambara girmez. Not: fizik görevleri `Split`, `Pick`, `seek_piece` hâlâ `a.kind.capacity()` okuyor, torba bonusu orada geçmiyor (fizik oturumunda düzeltilecek).
- **Ağıl (hayvancılık):** Evcil hayvan varsa, tarım biliniyor ve 4 odun eldeyse *Hayvancılık*. Ağıl 8 odun; ortada yemlik, çevresinde 3,5 m'lik çitle çevrili alan (`PEN_RADIUS_MM`). Tarifler istasyon işi olarak: öküz yavrusu ve tay 4 buğday / 7200 tick, köpek yavrusu 2 çiğ et / 5400 tick (`spawns`). İş ancak o türden iki hayvan ağılın içindeyken başlar (elle başlatmada `Reject::NeedsPair`, otomatikte bekler). İşçi panelindeki "Ağıla götür" hayvana [Yükle → ağıl, Bekle 10 dk] rotası verir: içeri girer, taşıdığı yemi yemliğe koyar, orada kalır. Otomatik açıksa rota ile gelen buğdayla sürü kendiliğinden büyür. Tarım → hayvancılık bağı: buğday yemdir.
- **Hedef alanı u32:** `milestones` 16 bitten 32 bite çıktı. Postcard u16 ve u32'yi aynı varint olarak yazar, durum hash'i alanı zaten u64'e çevirir: kayıt sürümü artmadı, eski kayıtlar hedefleriyle açılır (`save.rs` tek tür değişikliği). Test: `milestones_from_a_16_bit_save_read_as_32_bit`.
- **Bronz çağı, çeki gücü:** Bronz işi biliniyor ve öküz ya da at varsa *Çeki gücü*. Tezgahta tekerlek (4 odun + 1 bronz), araba (2 tekerlek + 6 odun), saban (3 odun + 1 bronz). Yeni komut `CommandKind::Hitch{worker,item}`: çantadaki araba ya da sabanı 3 m içindeki öküz/ata koşar (köpeğe ve ikinci kez aynı şeye `Reject::CannotHitch`). Arabalı hayvan +60 taşır (`CART_EXTRA`). Sabanlı hayvan tarlanın 3,5 m içindeyse ekin iki kat hızlı büyür (su çarkıyla çarpılır). İşçi panelinde "Arabayı koş", "Sabanı koş", sabanlıya "Tarlaya götür". Hedefler: araba yap ve koş, saban yap; BRONZ ÇAĞI artık bronz + bronz aletler + araba + saban ister.
- **Denge notu:** Algı puanı sabit bir tam puana (`PERCEPTION_FULL_SCORE`) göre ölçülüyor; yeni keşif, eşya ve hedefler algıyı biraz daha erken dolduruyor. Oyun testinden sonra ayarlanmalı.
- **Sırada:** bronzda kalıba döküm (yeni mini oyun) ve arabanın dünyada çizimi; bronzda kalıba döküm ve tekerlek/araba; demirde körük, örs, değirmen; otomatın sonraki çağa taşınması.
