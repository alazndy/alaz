# A.L.A.Z: Nihai Plan

Taş devrinden "bu oyunun kendisini yapmaya" giden yolun ana planı. Kararların ayrıntısı `DESIGN.md`'de, referans analizi `docs/referans-analizi.html`'de. Bu belge **ne, hangi sırayla, kim tarafından** sorusunun cevabıdır. Tarih: 10 Kasım 2026.

## 1. Vizyon

Oyuncu hiçbir şeyi olmayan bir maymun olarak başlar; taşı yontar, ateşi bulur, toprağı eker, metali eritir, makineyi döndürür, elektriği iletir, devreyi kurar ve sonunda bir bilgisayarda **bu oyunun kendi kural dosyasını** yazıp kendi kaydından yeni bir dünya başlatır. Her adım gerçek tarihin ve gerçek fiziğin bir yansımasıdır; sayılar (sıcaklık, oran, verim) doğrudan oyun kuralıdır.

**Final (karar, 10 Kasım 2026): döngü.** Bilgisayar bir *tasarım masası* açar: oyuncu yeni tarif ve makine tanımlar. Sonra kayıt yeni bir dünyanın tohumu olur. Yeni dünyada oyuncu yine bir maymundur (algı sıfır), ama dünya **oyuncunun yazdığı kurallarla** işler ve eski medeniyetin **harabeleriyle** doludur; maymun gözüyle harabeler anlamsız taş yığınıdır, algı arttıkça ne oldukları anlaşılır. Bilgisayara yeniden ulaşınca döngü yeniden başlar; her tur dünya biraz daha oyuncunun olur.

Ana referanslar: **TerraFirmaCraft** ve **Vintage Story** (gerçekçi hayatta kalma ve zanaat), **Create** ve güç eklentileri (görünür mekanik güç, enerji çağları), **Tinkers' Construct** (parçalı alet, döküm), **Modular Machinery** (veriyle tanımlanan çok bloklu makineler), **Create Aeronautics** (fiziksel araçlar), **Minecraft** (kademe = erişim, oyuncunun kendi hedefi).

## 2. Yedi ilke

1. **Her şey dünyada ve görünür.** Güç milden akar, eşya yerde ya da bantta durur, araç fiziksel bir cisimdir; bir şey durunca sebebi gözle görülür.
2. **Parçalardan kur, tarif seçme.** Alet parçalardan, makine yapı deseninden, araç bloklardan kurulur.
3. **Kademe erişim demektir.** Yeni malzeme yalnız hızlandırmaz; yeni bir işi ya da yeri açar.
4. **Cezalandırma, tasarıma davet et.** Alet kaybolmaz körleşir; hatalı metal yeniden erir; başarısız montaj hurda verir.
5. **Gerçek sayılar kuraldır, oyunun içinde öğretilir.** Oyuncu sayıları wikide değil algı, defter ve renkte öğrenir. Tekrar eden işi bir sonraki çağda makine devralır.
6. **Risk bir ritimdir.** İlk gece, kış, fırtına: haber verilen, tekrar eden tehditler; hazırlık oyunun parçasıdır.
7. **Tek güç birimi, kayıplı köprüler.** Dönme merkezdedir; buhar, ısı, elektrik ona bağlanır, her dönüşüm kaybettirir; yeni enerji eski ağı çöpe atmaz.

### İlkelerin uygulanışı (karar, 10 Kasım 2026)

| İlke | Karar |
|---|---|
| 1. Dünyada ve görünür | İstasyon panelleri kalır ama yalnız kumandadır; işin sonucu dünyada görünür (ocakta parlayan metal, tezgahta duran alet). |
| 2. Parçalardan kur | Basit yapılar tek tıkla (kamp ateşi, barınak, ambar, tarla); makineler yapı desenine göre parça parça (bloomery, değirmen, yüksek fırın). |
| 3. Kademe erişimdir | Tam kapı: yanlış aletle hiç çıkmaz, "daha iyi bir alet gerek" ipucu gelir. |
| 4. Cezalandırma | Affedici modda başarısız el işi malzemenin yarısını geri verir; metal her zaman tamamen yeniden erir. Gerçekçi mod bilerek daha serttir. |
| 5. Gerçek sayılar | Çatışmada okunurluk ve eğlence kazanır; gerçekçilik varsayılandır ama kutsal değildir (ör. sıcaklık aralıkları genişletilebilir). |
| 6. Risk ritmi | İki ana ritim: gece (günlük, var) ve kış (yıllık: ekin büyümez, yiyecek bozulur, soğuk artar; sonbaharda stok). Başka sürekli tehdit eklenmez. |
| 7. Tek güç birimi | Hayvan gücü ilk dönme kaynağıdır: hayvan manejı (öküz, at) su çarkından önce gelir. El değirmeni elle, sonra hayvanla, sonra suyla döner. |

## 3. Değişmez teknik sınırlar

| Sınır | Kural |
|---|---|
| Determinizm | Sim sabit adım (60 tps), tam sayı ve sabit nokta. Yeni her sistem (ısı, güç ağı, devre, araç fiziği) kayan nokta kullanmaz; sıralı ve tekrarlanabilir hesaplanır. Lockstep coop buna dayanır. |
| Veri | İçerik `rules.toml`'da. Kodda sabit etki bırakılmaz (su çarkı ×2, saban ×2 veriye taşınacak). |
| Kayıt uyumu | Eşya, keşif, hedef, tarif ve bina **yalnız sona eklenir**. Eski kayıttaki kısa eşya dizileri boş slotla tamamlanır (`items_serde`). Bit alanları büyütülürken postcard varint'inden yararlanılır (u16→u32 yapıldı; sürüm artmadı). |
| Alan sınırları | Keşif 25/32 bit, hedef 18/32 bit, görülen eşya 48/64 bit, tarif 38/256 (u8). **Bakır çağından önce** `known` u32→u64, `seen` u64→u128, `RecipeId` u8→u16 genişletilmeli (hepsi varint, sürüm gerekmez; testle doğrulanır). |
| Zaman ölçeği | 1 oyun günü = 24 gerçek dakika. Referanslardaki süreler (VS çeliği 160 saat) bu ölçeğe göre sıkıştırılır: uzun iş saatler değil, oyun günleri sürer. |
| Performans | Aktif bölge simülasyonu korunur; ağ hesapları (güç, ısı, devre) yalnız değişince yeniden kurulur, her tickte değil. |
| Kural uyumu | **Karar (10 Kasım):** kural dosyası yalnız sona ekleme yapılarak değiştiyse eski kayıt açılır; başka değişiklikte reddedilir. Kayıt, kurallarının her bölümdeki girdi sayısını ve o önekin sağlama toplamını taşır (S0b). |
| Zorluk | **Karar:** ayarlanabilir. Dünya kurulurken seçilir: *affedici* (varsayılan: süreler oyun günlerine sıkışır, hatalı metal yeniden erir, alet körleşir) ya da *gerçekçi* (TFC/VS sertliği: uzun süreler, kalıcı hatalar, ağır bakım). Çarpanlar `rules.toml`'da zorluk başına; seçim dünyaya ve hash'e girer. |

## 4. Çapraz sistemler (altyapı)

Her çağ bunların bir kısmını kullanır. Sıra bağımlılığa göre.

| Kod | Sistem | İlham | İçerik | Bağımlı |
|---|---|---|---|---|
| S0 | Alan genişletme | — | `known` u64, `seen` u128, `RecipeId` u16 + uyumluluk testleri | — |
| S0b | Eklemeli kural uyumu | — | Kural sağlama toplamı girdi başına; kayıt bölüm sayılarını ve önek toplamını taşır; yükleme yeni kuralların öneki eşleşiyorsa kabul eder. Kayıt sürümü artar: fizik oturumuyla birlikte | — |
| S16 | Dünyaya özgü kurallar | Final | Kurallar global sabit değil, dünyanın verisi: `World` kendi kurallarını taşır, kayıt onları içerir (temel kurallara göre fark olarak), coop anlık görüntüsü gönderir. `rules()` çağrıları dünyanın kurallarına taşınır. S0b bunun üstüne kurulur | — |
| S17 | Miras ve harabeler | Final | Eski kayıttan miras kaydı (bina türü ve konumu, birkaç eşya); yeni dünya üretiminde tohumdan deterministik yerleştirme; algıya göre görünüş | S16 |
| S0c | Zorluk ayarı | — | Dünya ayarı (affedici/gerçekçi), menüde seçim; süre, hata ve aşınma çarpanları veride | — |
| S1 | Kademe = erişim | Minecraft | Toplama ve madencilikte alet kademesi şartı (veri: `node.min_tier`); mağara odaları kademe ister | — |
| S2 | Isı | TFC, VS | Eşya ve istasyon sıcaklığı (tam sayı °C); yakıt tavanı (odun < odun kömürü 750 < kok 800); ısınma/soğuma; algı düşükken renk, yüksekken derece | — |
| S3 | Metal ve döküm | TFC, Tinkers | Kap/pota içinde metal miktarı (birim), alaşım = oran aralığı (bronz %88–92 bakır); kil kalıp (geçici), bronz kalıp (kalıcı); çukur fırın | S2 |
| S4 | Dövme ve örs | VS, TFC | 2D ızgara külçe, hedef şekil; hamleler: vur, şişir, kes; hata yeniden erir; örs kademeleri; su verme | S2, S3 |
| S5 | Aşınma ve parçalı alet | Tinkers, Minecraft | Kafa + sap + bağ; malzeme özellikleri (az ve okunur); körleşme, onarım; dövme bonusu dayanıklılığa | S4 |
| S6 | Mekanik güç ağı | VS → Create | v1: tork ≥ direnç toplamı, mil ve dişli; v2: hız ve stres ayrımı, dişli oranı. El değirmeni elle de döner | — |
| S7 | Yapı desenli binalar | Modular Machinery | `rules.toml`'da `structure`; parça parça kurulur; değiştirici yuvaları (körük → süre ×0,5) | S6 |
| S8 | Jeoloji ve arama | TFC, VS | Gizli damarlar (seed'den), kaya türüne bağlı cevher, yüzeyde küçük parçalar, arama aleti (yoğunluk + düğüm), destek kirişi | S1 |
| S9 | Beden ve mevsim | TFC, VS | Besin grupları (en yüksek can), bozulma ve koruma (çömlek, kiler, tuzlama), ekin mevsim/nem, toprak yorgunluğu, hayvan nesilleri, ağırlık | — |
| S10 | Lojistik | Create | Oluk ve bant: fizik parçalarını taşır | S6, fizik |
| S11 | Fiziksel araçlar | Aeronautics | Parçalardan araba, sal; ağırlık hızı düşürür; sonra balon | fizik |
| S12 | Isı ağı ve buhar | Steam Powered, New Age | Isı taşınır; kazan + silindir; makine malzemesi basıncı belirler | S2, S6 |
| S13 | Elektrik | Crafts & Additions, New Age | Dinamo/motor köprüsü (kayıplı), tel menzili, mıknatıs verimi | S6 |
| S14 | Devreler | Power Grid | Küçük devreler gerçek yasalarla (Ohm, Kirchhoff), sabit noktalı çözüm | S13 |
| S15 | Programlanabilir makine | Final | Mantık → bellek → komut; oyuncu makine/kural tanımlar | S14 |

## 5. Çağlar

Her çağın bir **imza malzemesi**, bir **imza makinesi**, bir **yeni fiili** (erişim) ve bir **çağ hedefi** vardır. Durum: ✔ var, ◐ kısmen, ○ yok.

| # | Çağ | İmza malzeme / makine | Yeni fiil | İçerik | Sistemler | Durum |
|---|---|---|---|---|---|---|
| 0 | Hayatta kalma | — | yaşamak | Açlık, su, sıcaklık, kurt, yara, ilk gece | — | ✔ |
| 1 | Taş devri | çakmaktaşı / kamp ateşi | yontmak | Yontma, taş balta, mızrak, kıskaç, giysi, meşale, mağara. Eksik: bıçak, kürek, çekiç kafaları | S1 | ◐ |
| 2 | Cilalı taş | kil / tarla, dokuma tezgahı | ekmek, pişirmek | Tarım, ekmek, çömlek, dokuma, ağıl ✔. Eksik: çukur fırın, kiler, ekin mevsimi | S3 (çukur fırın), S9 | ◐ |
| 3 | **Bakır çağı** (yeni) | bakır / çukur fırın, kil kalıp | eritmek, dökmek | Yüzeyde bakır parçaları, kapta eritme, kil kalıba döküm, bakır kazma ve balta; bakır kazma derin damara erişir | S0, S1, S2, S3, S8 başlangıç | ○ |
| 4 | Bronz çağı | bronz / ocak, örs | alaşım, dövmek | Oranla alaşım, örs II, dövme oyunu, bronz kalıp; araba ve saban ✔; el değirmeni | S3, S4, S6 v1 başlangıç | ◐ |
| 5 | Demir çağı | demir / bloomery, körük, şahmerdan | körüklemek, su vermek | Yapı desenli bloomery, süngerimsi demir örste arınır, örs III, su verme, su değirmeni ve şahmerdan; aşınma | S4, S5, S6 v1, S7 | ◐ (eski demir zinciri var) |
| 6 | Çelik ve mekanik çağ | çelik / yel değirmeni, yüksek fırın | ağ kurmak | Sementasyon ya da yüksek fırın, güç ağı v2, bant ve oluk, fiziksel araba ve sal, yapı desenli makineler; **otomat buraya taşınır** | S6 v2, S7, S10, S11 | ○ |
| 7 | Buhar çağı | dökme demir, çelik / kazan, silindir | basınç | Isı ağı, kazan, buhar makinesi kademeleri, buharlı otomat ve taşıma; derin kaynak (kömür) | S12, S8 | ○ |
| 8 | Elektrik çağı | bakır tel, mıknatıs / dinamo, motor | iletmek | Dinamo, motor, tel menzili, ışık; telgraf | S13 | ○ |
| 9 | Elektronik çağı | silisyum / devre kartı | hesaplamak | Gerçek devre yasaları, mantık kapıları; yarı iletken için araştırma puanı | S14 | ○ |
| 10 | Bilgisayar ve final | — / bilgisayar, tasarım masası | tasarlamak | Programlanabilir makine; tasarım masasında yeni tarif ve makine; kayıt tohum olur, yeni dünyaya kurallar ve harabeler taşınır, oyuncu yine maymun (döngü) | S15, S16, S17 | ○ |
| (11) | Uzay (isteğe bağlı) | — / roket | uçmak | Balon → hava gemisi → roket; vizyondaki "uzay çağı" | S11 | ○ |

### Çağ kararları

**Çağ 0–1 (10 Kasım 2026):**
- *Tam kapı:* elle yalnız yerdeki gevşek taş (yeni kaynak), dal, lif, yemiş, ot. Kaya damarı taş çekiç ya da kazma, ağaç balta ister; baltasız ağaçtan yalnız dal düşer. Çakmaktaşı damardan, nadiren gevşek taştan.
- *Yeni taş kafalar (yontma + dal):* çekiç (damar kazar, yontmayı kolaylaştırır, ilk taş örs), bıçak (avı yüzer, lif ve otu hızlı keser), kürek (kil ve kumu hızlı kazar, çukur fırını kazar).
- *Av iki adım:* mızrakla düşen hayvan yerde kalır, bıçakla yüzülür (et + post); bıçaksız az et, post yok.
- *Hayatta kalmaya yeni şey yok:* beslenme, bozulma, sert kış S9 ile gelir.
- *İlk 10 dakika:* taş, dal, lif → keskin taş → taş balta (ağaç) → taş çekiç (damar, çakmaktaşı) → ateş. Rehber bu sırayı söyler.

**Çağ 2, cilalı taş (10 Kasım 2026):**
- *Çağın kalbi kış ritmidir;* S9'un çekirdeği öne alınır (Dilim A2): 6 günlük mevsim, ekin 1–1,5 gün ve kışın büyümez, yiyecek bozulur (ekmek ve tahıl uzun, et kısa), büyük çömlek (depolama kabı) bozulmayı yavaşlatır. Yeni hedef: "İlk kışı aç kalmadan geç". Beş besin grubu ve kiler S9'un geri kalanında.
- *Çukur fırın:* kürekle kazılır; en çok 4 ham eşya, üstüne lif (saman) ve odun; yaklaşık yarım gün. Çömlek artık kamp ateşinde değil burada pişer; kamp ateşi yalnız yemek pişirir. Isı sistemi (S2) burada doğar.
- *Üç kurucu ekin:* buğday (tahıl; çimenlik), mercimek (sebze/protein, çömlekte çorba; kurak bozkır), keten (lif, tezgahta kumaş; nemli yerler). Yabani hâlleri biyomlara dağılır.
- *Toprak yorgunluğu:* tarla başına tek verim değeri; hasat düşürür, dinlenme toparlar; ağıl gübre üretir, gübre verimi artırır.
- *Hayvan nesilleri (VS):* yavru anneden +1 nesil; nesil başına öküz kapasitesi ve at hızı +%5, 10. nesilde durur. Cinsiyet ve yaşlanma yok.
- *Değişmeyen:* ekmek zinciri, dokuma, torba, giysi. Öğütme taşı elle kalır; el değirmeni bronz çağında.
- *Çağ hedefi:* CİLALI TAŞ ÇAĞI = ilk hasat + ekmek + pişmiş çömlek + ilk kışı aç kalmadan geçmek. Affedici modda kış serin: hata yapan aç kalır, ölmez.

**Çağ 3, bakır (10 Kasım 2026):**
- *Cevher:* yüzeyde bakır parçaları (yeni kaynak, elle; yakında damar olduğunu gösterir: S8'in ilk adımı); bakır damarı taş çekiç ya da kazmayla.
- *Kademe:* taş 0, bakır 1, bronz 2, demir 3. Kalay damarı bakır kazma, demir damarı bronz kazma ister; mağara odaları 2: bakır, 3: bronz, 4: demir.
- *Isı tavanları:* kamp ateşi 600 °C (kalay erir, bakır erimez), çukur fırın 1200 °C (bakır 1085 °C erir).
- *Pota ve döküm:* kilden pota, çukur fırında pişer. Eritme istasyonda olur: fırın sıvı metal miktarını tutar (cevher 25 birim, külçe/kafa 100). Bronzda aynı pota alaşım içindir.
- *Kil kalıplar:* külçe, balta, kazma, bıçak, çekiç kafası; kilden, çukur fırında pişer. Affedici modda 3 dökümde, gerçekçi modda 1 dökümde kırılır. Kalıcı bronz kalıp bronz çağında.
- *Döküm mini oyunu (yeni):* basılı tut, pota eğilir; akışı yeşil bantta tut. Hızlı: taşar (taşan metal bakır parçası olur, yeniden erir). Yavaş: soğur (yeniden ısıtılır).
- *Bakır aletler:* dökülen kafa + dal: balta, kazma, bıçak, çekiç. Bakır çekiç ilk metal örse giden yol.
- *Keşif ve hedefler:* "Döküm" (bakır elde, ateş ve çömlekçilik biliniyor); "İlk metal", "Bakır alet", BAKIR ÇAĞI.
- *Teknik:* binaya sıvı metal ve sıcaklık alanları eklenir; kayıt değişikliği S16/S0b'deki tek sürüm artışına katılır. Yaklaşık 20 yeni eşya (S0 yeri açar).

**Çağ 4, bronz (10 Kasım 2026):**
- *Oranla alaşım:* bakır ve kalay aynı potada; bronz affedici %85–95, gerçekçi %88–92 bakır. Oran tutmazsa "bozuk bronz" (kırılgan, yeniden erir). Pota panelinde oran çubuğu, algı arttıkça yüzde. Yalnız kalay bronzu.
- *Taş ocak:* bronz çağının fırını; kömürle 1300 °C, 400 birimlik pota, sürekli ve otomatik çalışabilir; kömür çukuruna gerçek iş.
- *Dövme başlar (S4):* taş örs, sonra bronz örs. VS tarzı 2D ızgara: vur, şişir, kes; hata yeniden erir. Kafalar dökülebilir; dövülen kafa kalite bonusu alır (iyi, usta, kusursuz). Yalnız dövülerek: testere, levha, kalıcı bronz kalıp.
- *Aşınma öne alınır (S5 çekirdeği):* taş ve bakır dahil tüm aletlere dayanıklılık; alet kaybolmaz, körleşir, tezgahta onarılır; dövme bonusu dayanıklılığı artırır. Malzeme özellikleri ve kafa/sap/bağ ayrımı demir çağında.
- *Testere ve tahta:* testere odunu tahtaya çevirir; tekerlek, araba, el değirmeni, sonra makineler ve destek kirişleri tahta ister.
- *İlk mekanik güç:* el değirmeni (elle çevrilir; elle öğütme mini oyununun yerini alır) ve hayvan manejı (öküz ya da at değirmeni çevirir). Güç ağı v1: kaynak torku ≥ direnç toplamı; bu çağda manej + mil + değirmen.
- *Çeki gücü kalır;* tekerlek ve saban tahta ister. Fiziksel araba çelik çağında (S11).
- *Hedefler:* bronz, bronz alet, araba, saban, "Hayvan gücüyle öğüt"; BRONZ ÇAĞI hepsini ister.
- *Eski tarifler:* sabit bronz ve tezgah bronz alet tarifleri kurallarda kalır (kayıt uyumu), artık öğretilmez.

**Çağ 5, demir (10 Kasım 2026):**
- *Bloomery, ilk yapı desenli bina (S7):* "iskelet ve doldurma": iskelet tek tıkla konur, parçalar (kil ve taş baca katmanları, körük yuvası) sırayla taşınır, bina görünür biçimde yükselir, tamamlanınca çalışır. Değirmen ve yüksek fırın aynı yolla; desen `rules.toml`'da.
- *Körük:* olmadan bloomery yeterince ısınmaz. Önce elle (ritim oyunu: doğru anda bas, sıcaklık yükselir), güç ağına bağlanınca kendiliğinden.
- *Demir dökülmez, dövülür:* bloomery cüruflu süngerimsi demir verir; bronz örste dövülerek (ızgarada cüruf hücreleri "kes" ile) dövme demir külçe olur. Demir aletler yalnız dövülür. Demir örs (kademe 3) dövme demirden, bronz örste.
- *Su verme:* isteğe bağlı zamanlama oyunu; başarı bonus (dayanıklılık), başarısızlık affedici modda çatlak (yeniden dövülür), gerçekçi modda kırık.
- *S5'in geri kalanı:* kafa (hız, kademe), sap (dal/tahta: dayanıklılık), bağ (lif/deri/demir perçin: dayanıklılık); az ve okunur özellik.
- *Su çarkı güç ağına girer:* alan etkisi tamamen kaldırılır (eski kayıtlarda da); mil ve dişliyle değirmen, körük, şahmerdana bağlanır; eski çark sahiplerine ipucu. Şahmerdan (yeni) basit şekilleri dövme kurallarına uyarak otomatik döver. Dişli dövme demirden.
- *Otomat ve atölye çelik ve mekanik çağa taşınır;* tarifler kurallarda kalır (eski kayıtlar), Otomat hedefi DEMİR ÇAĞI'ndan çıkar.
- *Demirin açtığı kapılar:* mağaranın 4. odası, derin damarlar (kömür, S8), sert kaya.
- *Hedefler:* "Süngerimsi demiri dövme demire çevir", demir aletler, "Su gücüyle dövme" (şahmerdan), demir örs; DEMİR ÇAĞI hepsini ister.

**Çağ 6, çelik ve mekanik (10 Kasım 2026):**
- *Yüksek fırın (yapı desenli):* ateş tuğlası (ateş kili: yalnız belirli yerlerde) ve güçle çalışan körük şart. Girdi: demir cevheri, kömür, kireçtaşı (eritken; yeni kaya kaynağı, S8). Çıktı pik demir: dökülürse dökme demir (kazan, silindir, tencere: döküm geri döner); örste iki kez dövülürse yüksek karbonlu çelik, sonra çelik. Çelik örs kademe 4.
- *Güç ağı v2 (Create):* hız (RPM) ve yük kapasitesi ayrı; dişli oranları; yel değirmeni (yükseklik ve havaya bağlı) ve büyük su çarkı; aşırı yük ağı görünür biçimde durdurur.
- *Lojistik (S10):* oluk ve bant fizik parçalarını taşır, huni ve filtre ayırır; malzeme ilk kez ajansız akar. Taşıma fiziği fizik oturumunda, kurallar ve ayrıştırma oynanışta.
- *Yapı desenli makineler:* testere değirmeni (kereste otomatik), büyük değirmen, pres (levha), haddehane (çubuk, tel: elektriğe köprü).
- *Fiziksel araçlar (S11, fizik oturumu):* araba çerçeve + 2 tekerlek + aks, yük ağırlaştıkça öküz yavaşlar; sal ve kayık. Koşulan araba eşyası buna dönüşür.
- *Otomat: kurgulu düzenek + delikli kart.* Yay ve dişliyle çalışır, kömür yemez; elle ya da güçlü bir kurma istasyonunda kurulur. Ne yapacağını elle delinen, çoğaltılabilen bir kart söyler: bugünkü Rotalar sistemi karta dönüşür. Programlama çizgisi: kart (sıralı komut) → röle ve devre (koşul, mantık) → tasarım masası (kuralın kendisi). Yan ürün: kartlı dokuma tezgahı (desenli kumaş).
- *Çeliğin açtığı kapılar:* sert kayalarda derin damarlar, büyük yapılar, çelik yay ve dişliler.
- *Hedefler:* çelik, "Rüzgârla çalışan ağ" (yel değirmeni + en az iki makine), "İlk bant hattı" (ajansız iki istasyon arası), "Kartlı otomat", fiziksel araba; ÇELİK VE MEKANİK ÇAĞI hepsini ister.

**Çağ 7, buhar (10 Kasım 2026):**
- *Isı ağı (S12):* ısı üretilip taşınabilen bir kaynak; tam sayı, ağ değişince hesaplanır.
- *Kazan ve buhar makinesi:* ocak → kazandaki su → buhar → silindir → volan → güç ağı (buhar da dönmeye bağlanır). Malzeme kademesi: dökme demir (düşük basınç, orta güç), çelik (yüksek basınç, yüksek güç). Aşırı basınç: affedici modda emniyet supabı verim kaybettirir; gerçekçi modda supapsız ya da yanlış ayarlı kazan patlar (oyuncunun hatası, dış tehdit değil).
- *Sıvılar ve borular:* boru ağı ve pistonlu pompa (güçle); yalnız su ve buhar (petrol 13. adımda); tam sayı.
- *Kömür ve kok:* maden kömürü derin damarlarda (S8); kok fırını daha sıcak yanan kok verir.
- *Kademe = erişim:* derin katmanlar su basmış; buharlı pompa olmadan inilemez.
- *Demiryolu (fizik oturumuyla ortak), sade ilk sürüm:* ray, buharlı lokomotif, vagon; tek hat, iki istasyon, delikli kartla sefer; sinyal ve kavşak yok (ileride).
- *Fabrika:* tek büyük makine kayış ve millerle binayı çalıştırır; mekanik dokuma, un fabrikası.
- *Buharlı otomat:* kurulmaz, kömür ve suyla çalışır; aynı kartlar.
- *Cam:* kum + kül, yüksek ısı; basınç göstergesi, pencere; ileride ampul, tüp, güneş paneli.
- *Hedefler:* maden kömürü, "Buharla çalışan ağ", "Su basmış madeni boşalt", "İlk demiryolu", cam; BUHAR ÇAĞI hepsini ister.

Not: Bakır çağı, TFC ve Vintage Story'deki gibi taş ile bronz arasına yeni bir çağ olarak giriyor; bugün bakır ancak bronzun hammaddesi.

## 6. Yakın vade: sıralı iş listesi

Her madde bitince: sim testi (baştan sona, komutlarla), arayüz testi (gerekirse), rehber metni, `DESIGN.md` güncel, tüm testler yeşil.

### Dilim A: Bakır çağının zemini
1. **S0 Alan genişletme.** `known` u64, `seen` u128, `RecipeId` u16; eski kayıt verisini okuyan testler.
2. **S16 Dünyaya özgü kurallar.** `rules()` her yerde kullanılmadan önce taşınmalı; her yeni çağla maliyeti büyür. Kayıt sürümü S0b ile aynı seferde artar.
3. **S0b Eklemeli kural uyumu.** Fizik oturumuna haber verilip kayıt sürümü bir kez artırılır; bundan sonra çağ eklemek kayıtları bozmaz.
4. **S0c Zorluk ayarı.** Menüde seçim, dünyaya kayıt; ilk etapta yalnız altyapı (çarpanlar 1), sistemler geldikçe bağlanır.
5. **S1 Kademe = erişim.** Düğümlere ve mağara odalarına en düşük alet kademesi; yanlış aletle `Reject::NeedsTier` ve açıklayıcı ipucu. Kademe: taş 0, bakır 1, bronz 2, demir 3. Kalay damarı bakır kazma, demir damarı bronz kazma ister; mağara odaları 2: bakır, 3: bronz, 4: demir. Taş devrinde kaya damarı çekiç/kazma, ağaç balta ister; elle yalnız gevşek taş ve dal.
6. **Taş devri tamamlama.** Bıçak, kürek, çekiç kafaları (yontma) + dal.
7. **S2 Isı çekirdeği.** Eşya sıcaklığı alanı; kamp ateşi ve çukur fırın ısıtır; yakıt tavanı; renk/derece gösterimi algıya bağlı.
8. **Çukur fırın.** Lif (saman) + odun; 4 eşyaya kadar; çömleği pişirir.

**Dilim A durumu (10 Kasım 2026):**
- ✔ S0: `known` u64, `seen` u128, `RecipeId` u16; sürüm artmadı (postcard varint; tarif kimlikleri 128'in altında kaldıkça u8 kayıtlar okunur, test bunu bekçi olarak tutar). Hash eski durumlar için değişmedi.
- ✔ S1: alet ve kapı tabloları `rules.toml`'da (`[[tool]]`, `[[gate]]`, `cave_tiers`); `gate_check` hem toplama komutunda hem görevde (rotalar da); otomat demir kademesinde sayılır. Uyarı: `Reject::NeedsTool`.
  - *Plandan sapma:* "gevşek taş" yeni bir kaynak türü değil, kayalığın üstteki ilk iki birimi (`loose = 2`). Yeni düğüm türü dünya üretimini değiştirip bütün kayıtları bozardı. Çakmaktaşı damardan (kalan 2. birim), gevşek taşlarda 1/8 şans.
  - Bakır aletler gelene kadar bakır gerektiren kapılar (kalay, 2. oda) taş kademesinde; bakır çağında yalnız veri değişir.
- ✔ Taş çekiç, taş bıçak, taş kürek (yontma + bağlama). Bıçak lif ve otu, kürek kili iki kat hızlı alır.
  - *Plandan sapma:* av iki ayrı eylem olmadı; avın sonunda bıçak varsa tam et + post, yoksa 1 et ve post yok. Yere düşen leş gibi yeni bir varlık gerekmedi.
- Botlar (yeni oyuncu, temkinli oyuncu) yeni ilk 10 dakikayı oynuyor ve geçiyor.
- ✔ Çukur fırın: kürekle kazılır (kürek harcanmaz, 1 lif); çömlek artık yalnız burada pişer: 1 çömlek + 2 lif + 2 odun ya da 4 çömlek + 4 lif + 4 odun, yarım gün (43 200 tick). Kamp ateşi çömlek pişirmez. Rehber kürek → çukur → pişirme sırasını söyler.
- ✔ Kayıt sürümü 21 (fizik oturumuna haber verildi): eski yapı (`SaveCore`) değişmedi, sona bir ek bölüm (`SaveExt`) geldi. Sürüm 20 ve öncesi yükseltilerek açılır (affedici, 3 günlük mevsim).
- ✔ S0b: kural kontrolü artık sağlama toplamı değil, **tarif sırası**: kaydın tarif kimlikleri mevcut kuralların öneki olmalı. Sona eklemek ve tarif içeriğini ayarlamak eski kayıtları bozmaz; tarif sırasını değiştirmek bozar.
- ✔ S0c: `WorldSettings` (zorluk: affedici/gerçekçi; mevsim günü: yeni dünya 6, eski 3). Menüde zorluk ve mevsim (3/6/9 gün) seçicileri; hardcore ölümü aynı ayarlarla yeni dünya kurar. Takvim fonksiyonları mevsim gününü parametre alır. Zorluk çarpanları henüz bağlı değil (sistemler geldikçe).
- ✔ S16: dünya kendi kurallarını taşır (`World::rules()`, `rules_arc()`, `set_custom_rules`), kayda ve hash'e girer; dünya yöntemleri kendi kurallarını okur. Serbest yardımcılar (`gate_check`) temel kuralları, `_in` sürümleri dünyanınkini kullanır.
- ✔ Yeni alanlar hazır (henüz kullanılmıyor): bina sıcaklığı (S2), tarla verimi (A2), hayvan nesli (A2).
- ✔ S2 ısı çekirdeği: bina sıcaklığı (°C, tam sayı, saniyede bir); ısı kaynakları `heat_max`/`heat_rate` ile (kamp ateşi 600 °C hızlı, çukur fırın 1200 °C yavaş, taş ocak 1300 °C); yanarken ya da iş yaparken tavana çıkar, sönünce havaya doğru soğur. Tariflerde `heat`: iş ancak o sıcaklıkta ilerler (et 150, ekmek 200, çömlek 900, bronz 950, demir 1150). Bina panelinde sıcaklık çubuğu: algı < 3 renk adı (TFC ölçeği), ≥ 3 derece; "Isınıyor: iş … olunca başlar". Eşya sıcaklığı Dilim B'de (döküm, dövme).
- **Dilim A tamamlandı.**

**Dilim A2 durumu (10 Kasım 2026):**
- ✔ Buğday ekimi ≈1,25 gün (108 000 tick). Kışın tarlada hiçbir şey büyümez; gerçekçi modda kışa kalan ekin ölür (`Event::CropFrozen`).
- ✔ Yabani ekin iklime göre (yeni düğüm türü olmadan, uzun otun son tutamı): nemli yerde keten, kurak ılık yerde mercimek, başka yerde buğday tohumu. Tarlada mercimek (2 → 7) ve keten (2 → 6) ekilir. Tezgahta 3 keten → 1 kumaş. Kamp ateşinde 2 mercimek + dolu çömlek → çorba (4000 doyurur), çömlek geri gelir.
- ✔ Toprak: ilk ürün verime göre çıkar (tohum payı tam), her hasat verimi 150‰ düşürür (en az 300‰), boş tarla 10 sn'de 1‰ toparlanır. Ağıldaki hayvanlar 100 sn'de bir gübre bırakır; tarlaya yüklenen gübre verimi 250‰ artırır. Tarla panelinde verim çubuğu ve kış uyarısı.
- ✔ Nesil: yavru, ağıldaki çiftin en yüksek neslinden bir fazla (en çok 10); hayvanın taşıma gücü nesil başına %5. Atın hızına henüz bağlanmadı.
- ✔ Hedef "İlk kışı aç kalmadan geç" (ilk yılın sonunda tok bir insan); CİLALI TAŞ ÇAĞI bunu da ister.
- ✔ Bozulma, **kayıt sürümü artmadan**: her oyun saatinde her yiyecek yığınının bir payı bozulur; kesirli kısım tohum ve zamandan türeyen bir hash ile yuvarlanır (coop'ta herkes aynı sonucu görür, yığın başına durum saklanmaz). Hızlar `rules.toml`'da (`[[perish]]`, günlük binde): çiğ et 500, yemiş 350, pişmiş et 250, çorba 200, ot 100, ekmek 50, un 20, tahıl 10, mercimek 10. Bozulan yiyecek gübreye döner. Kış yarıya indirir, gerçekçi mod iki katına çıkarır.
  - *Plandan sapma:* bozulma için kayıt sürümü 22 gerekmedi.
- ✔ Büyük çömlek: 5 kil (yığma oyunu), çukur fırında pişer. Ambardaki her büyük çömlek (en çok 4) oradaki bozulmayı yavaşlatır (1 + çömlek kat).
- ✔ Rehber: güzde stok azsa "kışın ekin büyümez, biriktir" der; çantadaki yiyecek bozulunca uyarı.
- **Dilim A2 tamamlandı.** Sırada Dilim B (bakır çağı). Beş besin grubu ve kiler S9'un geri kalanında.

**Dilim B durumu (10 Kasım 2026):**
- ✔ Döküm keşfi: elde bakır taşı + çömlekçilik + ateş.
- ✔ Pota (2 kil + lif, "kile lif kat"), kil kalıplar: balta (taş balta model), kazma (çekiç model), bıçak (taş bıçak model), külçe (taş); hepsi çukur fırında 900 °C'de pişer.
- ✔ Bakır eritme çukur fırında: 4 bakır taşı + pota + lif + odun, 1085 °C → "eriyik dolu pota". Dökme elde (şimdilik sıkıştırma mekaniği: akışı bantta tut); başarıda kafa + pota geri, kalıp affedici modda 1/3, gerçekçi modda her dökümde kırılır; taşan döküm 3 bakır taşı + pota + kalıbı geri verir.
  - *Plandan sapma:* sıvı metal binada değil bir eşyada (eriyik dolu pota); eşya sıcaklığı ve potanın soğuması yok; kalıp kırılması şansla (sayaçsız). Kayıt sürümü gerekmedi.
- ✔ Bakır balta, kazma, bıçak (kafa + dal); kademe 1. Kalay damarı ve mağaranın 2. odası artık bakır ister (`cave_tiers = [0, 1, 2, 3]`).
- ✔ Affedici modda başarısız el işi malzemenin yarısını geri verir (`fail_returns` olan tarifler kendi listesini verir); gerçekçi modda hepsi gider.
- ✔ Hedefler: İlk metali dök, Bakır balta ve kazma, BAKIR ÇAĞI. Rehber adım adım söyler; çağ adı "Bakır Çağı".
- Bekliyor: döküm oyununun kendi görseli; Dilim C (oranla alaşım, dövme, aşınma, testere, el değirmeni ve manej).

**Dilim C durumu (10 Kasım 2026), birinci parça:**
- ✔ İstasyonlar işi başlatırken önce girdi deposundakini, sonra çantadakini kullanır (yükle, sonra başlat).
- ✔ Oranla bronz: taş ocağa istenen kadar bakır ve kalay taşı yüklenir, "Alaşım erit" (2 kömür, 950 °C). Bütün yüklü taş birlikte erir; 4 taş bir külçe; bakır payı affedici %85–95, gerçekçi %88–92 ise bronz, değilse bozuk bronz. Bozuk bronz ocakta 3 bakır + 1 kalay taşına çözülür. Ocak panelinde oran çubuğu (algı < 3 "iyi görünüyor / tutmuyor", ≥ 3 yüzde).
- ✔ Bronz döküm: bronz külçe çukur fırında potada erir (950 °C), bakır çağının balta ve kazma kalıplarına dökülür, kafa dala bağlanır. Eski sabit bronz tarifi ve tezgah bronz aletleri `retired` keşfine alındı: kurallarda duruyor (eski kayıtlar), öğretilmiyor, panellerde görünmüyor.
- ✔ Örs (6 taş, çekiç elde): "near" alanlı tarifler yalnız örsün başında yapılır (`Reject::NeedsStation`). Örste dövme: testere (2 bronz + kömür) ve bronz balta/kazma kafası (döküme alternatif, iyi işte fazladan kafa).
  - *Plandan sapma:* dövme oyunu henüz 2D ızgara değil; mevcut zamanlama + yontma adımlarıyla. Örs kademeleri (taş/bronz/demir örs) demir çağında.
- ✔ Testere ve tahta: testereyle 1 odun → 2 tahta. Tekerlek, araba, saban artık tahta ister; el değirmeni ve manej de.
- ✔ El değirmeni (4 taş + 2 tahta): 3 buğday → 3 un. Güç: başında duran her insan 1, 6 m içindeki manejde dönen her öküz/at 3; en çok 3 (iş o kadar hızlı ilerler); güç yoksa durur. Hayvan manejı (4 tahta + 4 odun), işçi panelinde "Maneje götür". Hedef "Hayvan gücüyle öğüt"; BRONZ ÇAĞI bunu da ister.
- ✔ Aşınma (kayıt sürümü 22): aletlerde dayanıklılık (taş 60, bakır 150, bronz 250, demir 650; affedici ×2). İş yapan alet (gevşek taşlarda değil) her birimde aşınır; kör alet kaybolmaz, 3 kat yavaş. `Repair` komutu bir parça malzeme harcar (taş, bakır taşı, bronz, demir kütle). Çanta sekmesinde aşınma çubukları ve "Onar"; körleşince uyarı. Sürüm 21 kayıtlar aletler aşınmamış olarak açılır.
  - *Plandan sapma:* dövme bonusu dayanıklılığa bağlanmadı (alet başına kalite yok).
- **Dilim C tamamlandı.**

**Dilim D, demir (10 Kasım 2026):** ✔
- Bloomery (6 taş + 6 kil, 1300 °C) ancak 3 m içinde çalışan bir körük varken 1000 °C'yi geçer; körük (2 post + 2 tahta) başında duran biri, 6 m içinde manejde bir hayvan ya da 8 m içinde su çarkı/yel değirmeni/buhar ile çalışır. 2 demir cevheri + 3 kömür → süngerimsi demir (1150 °C).
- Süngerimsi demir örste dövülür (çekiç, "Cürufu döverek çıkar") ya da su gücüyle çalışan şahmerdanda kendiliğinden → dövme demir ("Demir kütle"nin yeni adı). Demir örs (4 dövme demir); demir balta ve kazma yalnız demir örste dövülür. Eski ocak demir tarifi ve tezgah demir aletleri `retired`.
- Su çarkının hız alanı kaldırıldı; güç kaynağıdır (`World::powered_at`). Hedef "Su gücüyle dövme" (şahmerdan); otomat DEMİR ÇAĞI'ndan çıktı.
- *Plandan sapma:* yapı deseni "iskelet ve doldurma" değil, yakındaki parçalar (bloomery + körük); su verme yok.

**Çağ 6, çelik ve mekanik:** ✔
- Hedef alanı u64 (varint, sürümsüz). Kireçtaşı (taş damarı, kalan 3. birimde 1/4) ve ateş kili (kil yatağının son birimi, 1/3): dünya üretimi değişmeden.
- Ateş tuğlası (çukur fırın, 1100 °C); yüksek fırın (8 ateş tuğlası + 4 taş + 2 dövme demir, körüklü, 1550 °C): 3 demir cevheri + 3 kömür + 1 kireçtaşı → 2 pik demir (1450 °C). Demir örste iki dövme: yüksek karbonlu çelik → çelik. Çelik örs.
- Yel değirmeni (8 tahta + 4 kumaş + 2 dişli): su çarkı gibi güç kaynağı. Taşıyıcı bant (güçle): 3 m içindeki istasyonların ürününü 12 m içindeki en yakın ambara taşır.
- Otomat artık çelik ister (2 çelik + 3 dişli + 4 tahta + 2 kömür). Hedefler: çelik, rüzgârla çalışan makine, ilk bant hattı, otomat, kartlı otomat (rotası olan otomat), ÇELİK VE MEKANİK ÇAĞ.
- *Plandan sapma:* güç ağı hız/stres ayrımı yok (tek "güç" sayısı); otomat kurgulu yay değil, kömürle; "kart" = mevcut rota; fiziksel araba, sal (fizik oturumunun alanı) yapılmadı.

**Çağ 7, buhar:** ✔
- Maden kömürü mağaranın derin odalarında; taş ocakta kok. Yüksek fırında dökme demir parça. Buhar makinesi (3 dökme demir parça + 2 çelik + 4 ateş tuğlası): kömür/kok/petrol + dolu çömlek yakarken kazan 120 °C'yi geçince 8 m içine güç verir. Pompa: güç varken 15 m içindeki mağaranın en dip odasını açar (`Reject::Flooded`). Kum (kil yatağından), taş ocakta cam.
- Hedefler: maden kömürü, buharla çalışan makine, su basmış madeni boşalt, cam, BUHAR ÇAĞI.
- *Plandan sapma:* ısı ağı ve boru/sıvı sistemi yok (buhar makinesi kendi kazanıyla); kazan patlaması yok; demiryolu (fizik) yapılmadı.

**Çağ 8, elektrik:** ✔
- Haddehane (güçle): bakır külçe → 4 tel. Mıknatıs demir örste. Dinamo: mekanik güçle dönünce ağı besler; direk, motor, lamba, bilgisayar 12 m içinde zincirlenir (`World::electric_live`). Ağa bağlı motor 8 m içinde güç kaynağıdır. Ampul (cam + tel + kok), lamba.
- Hedefler: elektrik, motorla çalışan makine, lamba, ELEKTRİK ÇAĞI.
- *Plandan sapma:* kayıplı dönüşüm ve mıknatıs verimi yok; ağ ya canlı ya değil.

**Çağ 9, elektronik:** ✔
- Silisyum (yüksek fırın, kum + kok, 1500 °C), transistör ve devre kartı (tezgah). Hedefler: silisyum, transistör, devre kartı, ELEKTRONİK ÇAĞI.
- *Plandan sapma:* gerçek devre yasaları (Ohm, Kirchhoff) ve araştırma puanı yok; mantık kapıları yok.

**Çağ 10, bilgisayar ve final:** ✔
- Bilgisayar (4 devre kartı + 2 cam + 2 çelik + 4 tel) ağa bağlıyken açılır. Tasarım masası (bilgisayar paneli): en çok 3 girdi ve 1 çıktıdan yeni el tarifi; eşya değerleri tariflerden hesaplanır (`World::item_values`), çıktı girdiden değerli olamaz (`Reject::Unbalanced`), yalnız görülmüş eşyalar. Tarif dünyanın kendi kurallarına eklenir (`CommandKind::Design`), kayda ve hash'e girer.
- Döngü: "Döngüyü başlat" → `World::next_cycle()`: tohum = bu dünyanın hash'i; ayarlar ve kendi kurallar taşınır; binalar yeni doğuş noktası çevresinde harabe olur (eski tür harabenin `fuel` alanında; algı ≥ 3 ne olduğunu söyler); oyuncu eşyasız, hiçbir şey bilmeden başlar. Yalnız tek oyuncu ya da sunucu başlatır (katılanlar eşitlemeyle yeni dünyayı alır).
- Hedefler: bilgisayar çalıştır, kendi tarifini yaz, BİLGİSAYAR ÇAĞI.
- *Plandan sapma:* tasarım masası makine (yapı deseni) tanımlamaz, yalnız tarif; döngü sayacı tutulmaz.

**S9 geri kalanı, jeoloji, petrol:** ✔
- Beslenme (kayıt sürümü 23): tahıl, protein, meyve-sebze; yavaşça azalır, yemekle dolar; dolu grup sayısı en yüksek canı belirler (gerçekçi %70–100, affedici %85–100). Durum panelinde (algı ≥ 2) üç çubuk.
- Kiler (8 taş + 4 odun, kürek): ambar gibi, yiyecek 5 kat yavaş bozulur.
- Gizli yataklar tohumdan (24 karelik hücrede en çok bir; demir, bakır, kalay, kömür, petrol; zenginlik 1–3), dünya düğümlerine dokunmaz. Arama kazması (bronz, örste): "Ara" çevredeki türleri ve en yakının yönünü söyler. Maden kuyusu yalnız yatağın üstüne (`Reject::NoDeposit`), "Kuyudan çıkar" (1 tahta) yatağı çıkarır. Petrol buhar makinesinde yakıt.

**Kayıt sürümleri bu oturumda:** 20 (fizik) → 21 (ek bölüm: ayarlar, tarif imzası, kendi kurallar, bina sıcaklık/verim, nesil) → 22 (alet aşınması) → 23 (beslenme). Her biri bir öncekinden yükseltilir; fizik oturumunun 17–20 yollarına dokunulmadı.

**Fizik oturumuna devredilenler (yapılmadı):** fiziksel araba/sal/balon (S11), demiryolu, bant üzerinde görünen parçalar; `Split`/`Pick`/`seek_piece` kapasitesi `a.capacity()` ve baltanın aşınması (`add_wear`).

**Açık kalanlar:** uzay çağı (karar bekliyor); döküm ve dövme oyunlarının kendi görselleri; güç ağı v2 (hız/stres); devre yasaları; kazan patlaması; hayvan neslinin at hızına etkisi.

### Dilim A2: Kış ritmi (S9 çekirdeği)
- Mevsim uzunluğu dünya ayarı (6 gün; eski kayıt 3); ekin süresi 1–1,5 gün, kışın büyümez.
- Yiyecek bozulması (tick ile son kullanma), büyük çömlek (depolama kabı).
- Mercimek ve keten, toprak verimi ve gübre, hayvan nesilleri.
- Hedef: "İlk kışı aç kalmadan geç"; CİLALI TAŞ ÇAĞI koşulu güncellenir.

### Dilim B: Bakır çağı
9. **S3 Kap ve döküm.** Kil kap içinde cevher erir, metal miktarı tutulur; kil kalıp (külçe, kazma, balta kafası); döküm adımı (kalıba dök, soğut, çıkar).
10. **Yüzeyde cevher parçaları ve ilk damarlar (S8 başlangıcı).**
11. **Bakır aletler** ve çağ hedefleri: "İlk metal", "Bakır alet", BAKIR ÇAĞI.

### Dilim C: Bronz ve dövme
12. **Alaşım oranla.** Bronz tarifinin yerine oran aralığı; eski tarif kayıt uyumu için yerinde kalır, artık öğretilmez.
13. **S4 Dövme oyunu.** 2D ızgara külçe, hedef şekil, vur/şişir/kes; hata yeniden erir; örs kademeleri.
14. **Aşınma ve onarım (S5 çekirdeği).** Tüm aletlere dayanıklılık, körleşme, tezgahta onarım; dövme bonusu dayanıklılığa.
15. **Testere ve tahta.**
16. **El değirmeni ve hayvan manejı (S6 v1).** Elle döner; manejde öküz ya da at çevirir; tork ≥ direnç.

### Dilim D: Demir
17. **S5'in geri kalanı:** malzeme özellikleri, kafa/sap/bağ ayrı malzemeler.
18. **S7 Yapı desenli bloomery + körük;** süngerimsi demir örste arınır.
19. **Su değirmeni ve şahmerdan;** su çarkının alan etkisi kaldırılır, mil ağına geçilir.

Bunlardan sonra: S8 jeoloji tam, S9 beden ve mevsim, çelik ve mekanik çağ.

## 7. İş bölümü

| Alan | Sahibi | Not |
|---|---|---|
| Fizik: gövdeler, parçalar, düşme, çökme, araç fiziği, ağırlığın harekete etkisi | Fizik oturumu | `physics.rs`, `world.rs` içindeki fizik görevleri (`Split`, `Pick`) |
| Oynanış: keşif, hedef, eşya, tarif, mini oyunlar, ısı, metal, güç ağı kuralları, rehber | Oynanış oturumu | `rules.toml`, `world.rs` içindeki keşif/hedef/yiyecek/toplama bölümleri, `crates/game` |
| Kayıt formatı ve sürüm | Ortak | Sürüm artıran değişiklik öncesi haber verilir; varint ile sürümsüz genişletme tercih edilir |
| Render | Ortak | Her oturum kendi eklediği bina/eşya modelini çizer |

Fizik oturumuna açık istekler:
- `Split`, `Pick`, `seek_piece` kapasiteyi `a.capacity()` ile okusun (torba ve araba bonusu için).
- Çökme (S8) ve parçalar için destek kirişi ve lojistik (S10) ortak tasarlansın.

## 8. Kararlar

Verilen kararlar (10 Kasım 2026):

| Konu | Karar |
|---|---|
| Gerçekçilik ve süre | **Ayarlanabilir.** Varsayılan affedici, isteyen için gerçekçi (S0c). |
| Bakır çağı | **Ayrı çağ**, taş ile bronz arasında (Çağ 3). |
| Kural değişince eski kayıtlar | **Eklemeli değişiklik kabul edilir** (S0b). |
| Gizem katmanı | **Şimdilik hayır.** Elektronik çağına yaklaşınca yeniden açılır. |
| Final | **C: döngü.** Tasarım masası (tarif ve makine tanımı) + kayıt tohum olur; yeni dünyaya **kurallar ve harabeler** taşınır; oyuncu yine maymun başlar (S16, S17). |
| Final zorunlu mu | **Hayır, isteğe bağlı.** Bilgisayara ulaşınca döngüye geçmek oyuncunun kararı; aynı dünyada sonsuza dek oynanabilir. |
| Tasarım masasının kapsamı | **Tarif ve makine** (mevcut parçalardan yapı deseni + tarif). Yeni eşya yok. Denge sınırı: bir tarif girdisinin değerinden fazla çıktı veremez (eşya değeri kurallarda tanımlanır). |
| Coop'ta final | **Döngüyü sunucu başlatır;** herkes birlikte yeni dünyaya geçer, kurallar ve harabeler ortaktır. |
| Takvim | **Mevsim 6 gün** (yıl 24 gün ≈ 9,6 saat, kış ≈ 2,4 saat). Mevsim uzunluğu dünya ayarı; eski kayıtlar 3 günde kalır. Ekin 1–1,5 gün sürer: mevsimde birkaç hasat, kışı stoksuz geçirmek olmaz. |
| Zorluk farkları | Affedici / gerçekçi: uzun işler kısa / ≈3× uzun; başarısız el işi yarısı geri / hepsi gider; aşınma ve bozulma yavaş / hızlı; kış serin / sert (donar, ekin ölür); kurt yalnız akşam-gece / gündüz de; ölüm her ikisinde son kayıttan (hardcore ayrı seçenek). |
| Zorluk değişebilir mi | **Hayır.** Dünya kurulurken seçilir, dünyanın hash'ine girer; coop'ta herkes aynı kurallarla oynar. |

Hâlâ açık:
- **Uzay çağı** finalden önce mi, sonra mı, hiç mi? Çelik ve mekanik çağdan önce karar gerekmiyor.

## 9. "Tamam" tanımı (her çağ için)

- Çağ zinciri sim testinde komutlarla baştan sona oynanır.
- Yeni mini oyun varsa arayüz testinde fareyle oynanır.
- Rehber bandı çağın her adımını söyler; defter ipuçları çağ görülmeden çıkmaz.
- Kayıt: eski kayıt açılır (test), yeni alanlar boş başlar.
- Determinizm: aynı tohum ve komutlarla iki koşu aynı hash'i verir.
- `DESIGN.md` ve bu plan güncellenir; gerçek oyunla bir oyun testi yapılır ve `playlog.txt` incelenir.

## Su ve büyük dünya (10 Kasım 2026, istek: gerçekçi dalga, akarsu/nehir/göl, büyük dünya, chunk yükleme)

**A. Suyun görünüşü:** ✔ Gerstner dalgaları (5 dalga, 1,9–14 m boy): tepeler sivri, çukurlar geniş; açık denizde daha büyük, rüzgârla kabarır, kıyıda söner. Kumsala koşan kırılan dalga köpüğü. Yalnız görüntü (shader); sim'e dokunmaz.

**B. Akarsu, nehir, göl (dünya üretimi v2):** ✔ (nehir ve göl; tatlı su ayrımı yapılmadı) Kararlar:
- Eski dünyalar değişmez. *Uygulamada:* üretim sürümü kayda değil tohuma yazıldı (üst 16 bit 0xA1A2, `map::gen_v2`); kayıt biçimi değişmedi. Saat tabanlı ya da elle yazılan tohumlar bu işareti taşımaz.
- Nehirler yerel gürültü kanallarından (|gürültü| küçük olan şeritler) oyulur: dağdan denize kesintisiz uzanır, chunk'larla uyumludur (bütün haritayı bilmeyi gerektirmez). Genişlik akış aşağı artar; dere → nehir.
- Göller çanaklarda (yükseklik gürültüsünün yerel çukurları) düz su yüzeyiyle.
- Yeni arazi türü: tatlı su (içilir; deniz suyu içilmez olabilir — ayrı karar).

**C. Suyun su gibi davranması:** ✔ (11 Kasım 2026) Nehirde akıntı yönü; akıntı yüzeni ve yerdeki parçaları sürükler; parçalar suda batmaz, yüzüp sürüklenir; sığda yürüme, derinde yüzme. Fizik oturumunun alanına değiyor (sal, yüzen cisimler S11): onunla paylaşılacak.

**D. Chunk'lı büyük dünya:** ✔ (11 Kasım 2026) Arazi ve kaynaklar zaten (tohum, x, y)'den hesaplanabiliyor. Yapılacak: 64 m'lik chunk'lar oyuncular yaklaştıkça üretilir, uzaklaşınca bellekten düşer; kayıt yalnız değişmiş chunk'ları (toplanan kaynak, bina) tutar; boyut sınırı kalkar ("Sonsuz" seçeneği). Sim, kaynakları dünya listesi yerine chunk haritasında tutar; deterministik kalır (chunk üretimi saf fonksiyon). En büyük iş; B ve C'den sonra.

**Ağaç boyları ve yan düzeltmeler (11 Kasım 2026):** ağaçlar 1,1–15 m (beş sınıf, `map::tree_height_mm` tek kaynak). Uzun gövdenin tepe ucu suya düşerse karadaki ucundan parçalanır; suya düşen odun parçası artık yok olmaz, kıyıda durur (kaya batar). Açık: `a_careful_newcomer_survives_two_days` 4. tohumda düşüyor — ağaç boyları zamanlamayı kaydırdı, bot 40 m uzaktaki suya giderken kurda yakalanıyor (oyun hatası değil, test botunun dikkat sınırı).

**C uygulaması:** `map::water_depth` (0 kara, 1 sığ: kıyıya bir kare, 2 derin) ve `map::current_mm` (yalnız v2 nehirleri; nehir çizgisi boyunca, arazinin alçaldığı yöne; dar dereler hızlı). Oyuncu yalnız elle yürürken suya girer (tıkla-git ve rotalar kıyıda durur, sudaki biri çıkabilir); hayvan ve otomat girmez. Sığda %55, derinde %35 hız; derinde soluk azalır, sıcaklık düşer, soluk bitince boğulma. Nehir yüzeni sürükler (sığda yarısı). Suya düşen kaya batar, diğer her şey yüzer, akıntıyla sürüklenir. Çizimde beden suya gömülür (bel/omuz), yüzen eşya sallanır; durum panelinde "Yüzüyorsun / Boğuluyorsun / Sığ suda".
*Sapma:* gerçek akışkan benzetimi (su yayılması, seviye) yok; akıntı gürültü alanından türetilir. Kayıt biçimi değişmedi.

**D uygulaması:** 4096×4096'ya kadar dünyalar eskisi gibi bir kerede üretilir (kaynak sırası aynı; eski kayıtlar bozulmaz). Daha büyükleri ("Devasa 32768": 32×32 km, 1024 km²) tembel: arazi tablosu tutulmaz, her kare tohumdan hesaplanır (`map::terrain_at`); kaynaklar ve hayvanlar 64 m'lik chunk'larda, her yaşayan insanın 3 chunk (~200 m) çevresinde, sabit sırayla üretilir (`World::make_chunk`). Kaynak araması satır dizisinden chunk dizinine geçti (`NodeChunks`). Kayıt sürümü 24: üretilen chunk'ların sırası ek bölümde; yüklerken aynı sırayla yeniden üretilir, böylece kaynak kimlikleri tutar (23'ten yükseltme yolu var). Ölçüm: 32 km'lik dünya 1 saniyenin altında açılıyor, oyun 73 MB bellekle normal boyuttakiyle aynı işlemci yükünde.
*Sapma:* gerçekten sonsuz değil (32 km sınırı: kare indeksleri 32 bit). Uzaklaşınca bellekten düşürme yok (gezilen yerlerin kaynakları bellekte kalır; arazi zaten tutulmuyor). Görüntü zaten görüş penceresine göre chunk'lı çiziliyordu.

## Hikâye, öğretici, sınırsız dünya, gerçekçi üretim (11 Kasım 2026)

**Üçüncü dünya üreticisi (yeni 4096+ dünyalar; tohum etiketi 0xA1A3, `map::gen_v3`):** kıta ölçeğinde (9 km) kara ve okyanus, iki ölçekte bükülmüş kıyılar, tepeler, dağ kuşaklarında uzun sırtlı sıradağlar (zirveler kar), enleme göre iklim kuşakları (24 km'lik döngü; en büyük dünyanın ortası ılıman), yükseldikçe soğuma, sıcak-kuru yerlerde çöl (kum), nemli ve soğuk-nemli yerlerde orman. Nehirler bu üreticide çok daha geniş ölçekli: az ama uzun, kıta boyunca kıvrılan nehirler; göller seyrek. Ölçüm (32 km'lik görüntü): %23 su, %5 kum, %15 kaya, %31 orman, %26 çayır. 4096'dan küçük dünyalar ikinci üreticide kalır (kıtalar küçük dünyayı tek parçaya düşürürdü). Eski üreticiler ve kayıtlar değişmedi.

**Sınırsız dünya:** menüde "Sınırsız" = 262.144 m'lik (262 km) chunk'lı dünya. Yol bulma engel kümesi 64 bit anahtara geçti. *Sapma:* matematiksel olarak sonsuz değil; görüntü 32 bit kayan sayılarla çizildiği için bu boyutta ortada 1,5 cm hassasiyet kalıyor, daha büyüğü titreme yapardı (kamera-göreli çizim gerekir).

**Hikâye ve öğretici (`game/src/story.rs`):** yeni dünyada dört sayfalık açılış (geçilebilir); her çağ tamamlanınca bir kez gösterilen çağ sayfası (yüklenen dünyada geçmiş çağlar yeniden anlatılmaz); sağ üstte "İlk adımlar" listesi (yürü, taş, dal ve lif, ilk yapım, ye ve iç, ateş, barınak), dünyadaki gerçek duruma göre kendiliğinden işaretlenir, gizlenebilir, bitince kaybolur. Kayda bir şey yazılmaz.

**Akıcılık, gölge, rüzgâr (11 Kasım 2026):**
- *Titreme:* çizim, iki sim adımı arasındaki oranı duvar saatinden tahmin ediyordu; 60 Hz ekran ile 60 adım/sn sim kayınca bazı karelerde 0, bazılarında 2 adım düşüyor, oran şaşıyordu. Artık oyun biriktiricideki kesin oranı veriyor (`Renderer::sim_alpha`; tek oyuncu ve sunucu; katılan oyuncuda eski tahmin).
- *Gölge titreşimi:* gölge kutusu 8 m basamaklarla büyüyüp küçülür (her karede kare boyu değişmez); gölge yönü güneşi her karede değil ~0,6°'lik adımlarla izler (aydınlatma sürekli).
- *Ortak rüzgâr:* `time::wind(seed, tick)` → yön (çeyrek günde bir yavaşça döner, sıçramaz) ve güç (sakin–esinti, fırtınada güçlü, ani sağanaklar). Otlar ve yapraklar rüzgâr yönüne yatar, esinti dalgaları rüzgâr yönünde ilerler; deniz dalgaları rüzgârdan gelir; bulutlar ve su yüzeyi rüzgârla sürüklenir; duman rüzgâr altına yatar; yağmur ve kar ekranda rüzgâra göre eğik yağar; durgun sudaki yüzen eşyalar rüzgârla yavaşça kayar (sim, deterministik).

**Performans incelemesi (11 Kasım 2026):** ölçüm `ALAZ_BENCH=30 ALAZ_NOVSYNC=1` (kamera uçarak gezer; bu ekranda 60 Hz tavanı kalkmıyor). Makinede aynı anda başka yük vardı (yük ortalaması 7–13), sayılar gürültülü.
- *Bulgu 1:* üçüncü üreticide bir arazi sorgusu ~0,5 µs; çizim aynı kareleri defalarca soruyordu. *Düzeltme:* yükseklik, arazi türü ve iklim 64×64'lük bloklar halinde birlikte hesaplanıp hatırlanır (`map::land_block`; saf fonksiyonların önbelleği, sim'i değiştirmez), her iş parçacığı son bloğu kilitsiz tutar; dolunca en uzak çeyrek atılır. Sorgu ~10 ns.
- *Bulgu 2:* çizimdeki her chunk tek bir genel kaynak sayacına bakıyordu: bir kaynak tükenince ya da (benim eklediğim) yeni sim chunk'ı üretilince ekrandaki her şey yeniden kuruluyordu. *Düzeltme:* 64 karelik bölge başına damga (`World::node_stamp`); yalnız değişen bölge yeniden kurulur.
- *Sonuç (sınırsız dünya, uçan kamera):* çizim ortalaması 37 → 2 ms, en kötü %1 375 → 16 ms, chunk başına hazırlama ~15 → ~1 ms, kare hızı 18 → 57 fps (ekran tavanı). 1 km'lik dünyada çizim 2–4 → 1,3–2 ms. Sim adımı 0,03–0,2 ms (sorun değil).
- *Açık:* 1 km'lik (ikinci üretici) dünyada ekranda 1,7 milyon köşe var (ağaçlar: tepe başına 7–9 küre); bu makinede 60 fps'i bozmuyor ama zayıf ekran kartında ilk darboğaz bu olur → uzak chunk'larda ağaçlar için düşük ayrıntı (LOD) önerilir.
- *Rüzgâr düzeltmesi:* hareket açıları dünya başlangıcına göre hesaplanıyordu (oyuncu 131 km uzakta): yön binde bir derece dönünce açı yüzlerce radyan sıçrıyor, 32 bit hassasiyet de kesik kesik hareket veriyordu. Dalgalar artık dönmeyen altı sabit dalga dizisi; rüzgâr hangilerinin büyüyeceğini seçer (yön değişimi yumuşak geçiş), her dizinin evresi işlemcide 64 bitle pencere köşesine göre hesaplanır. Ot ve yaprak salınımı, çayır dalgaları ve su yüzeyi kayması, kare kare biriken rüzgâr yer değiştirmesiyle taşınan gürültüden gelir. Fırtına rüzgârı hava dönünce 30 saniyede yükselir/iner (önceden tek adımda).

## Debug kapıları ve sıfırdan oynanış (11 Kasım 2026)
Ayrıntılı rapor: `docs/hata-ayiklama-raporu.md`. Özet: betik dili (`sim/src/debug.rs`; oyunda `ALAZ_SCRIPT`, başsız `play_script`), tam tarama (115/127 işlev ilk denemede; kalan 12 kurulum kaynaklı), dünya üretimi raporu. Düzeltilen kritikler: yeni üreticilerde kaynaksız kumsal ve maden-sız adacık başlangıçları (iki aşamalı doğma araması, 0,1–5 sn), kayıtların doğma kuralından bağımsız yüklenmesi. Açık: kurt dengesi, ateş besleme, barınak keşif eşiği, yemiş bozulma ipucu.
İkinci tur (aynı gün): her komutun kapısı ve testi (`every_command_has_a_working_gate`); tarama 127/127, artık her koşuda; denge düzeltmeleri — aletler çanta yüküne sayılmaz, pota+kalıp tek pişirme (`fire_casting_kit`), çukur fırın 8 °C/sn, kurt ısırınca 6 sn çekilir ve ilk iki gün yarı hasar, ateş dolana kadar odun alır, barınak 10 odunla keşfedilir; dünya: üçüncü üretici yalnız >4 km, küçük yeni dünyalarda çayırda maden damarları, doğmaya 200 m içinde bakır şartı. Sıfırdan bakır baltaya ~23 dk.
