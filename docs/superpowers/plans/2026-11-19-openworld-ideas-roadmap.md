# Yol haritası: Openworld'den alınan üç fikir

Tarih: 2026-11-19. Kaynak: https://www.play-openworld.com/tr/ (site, wiki "İlerleme" ve "Üretim" sayfaları).
Durum: **taslak plan**. Her proje başlamadan önce kendi kısa tasarımını alır (beyin fırtınası → tasarım → ayrıntılı plan),
sonra ajanlara dağıtılır. Bu dosya sırayı, kapsamı, verilmiş kararları ve açık soruları tutar.

## Sıra ve bağımlılıklar

| Sıra | Proje | Neye bağlı | Boyut |
|---|---|---|---|
| 0 | UI yeniden tasarımı (`feat/ui-redesign`, sürüyor) | — | büyük |
| 1 | **Eşya sayfaları ve tarif kitabı** | 0: bilgi paneli (`inv/info.rs`), Defter penceresi (`book.rs`) | orta |
| 2 | **İlerleme ağacı** (Defter'de "Keşifler" sayfası) | 0 ve 1: Defter, eşya sayfaları (adımlar eşya sayfasına bağlanır) | orta-büyük |
| 3 | **Kodla internetten katılma** | Yok (ağ katmanı ayrı). 0'dan bağımsız başlayabilir | büyük, sunucu gerektirir |

Neden bu sıra: 1 ve 2, 0'ın yaptığı Defter ve bilgi panelinin üstüne oturur; 2'nin her adımı 1'in eşya sayfalarına
bağlanır. 3 bağımsızdır ama bir sunucu kararı ister (aşağıda), o karar gelene kadar bekler.

---

## Proje 1: Eşya sayfaları ve tarif kitabı

**Amaç:** Herhangi bir eşyaya (çanta, makine, Defter, bilgi paneli) tıklayınca o eşyanın sayfası açılsın:
- **Nasıl yapılır:** onu çıkaran tarifler (hangi makinede, girdiler, ısı, süre).
- **Nerede kullanılır:** onu girdi olarak alan tarifler ve binalar (inşaat maliyetleri dahil).
- **Nerede bulunur:** toplanan kaynaklar için (taş yığını, ağaç, cevher damarı, mağara katı).
Sayfalar arasında ileri-geri gezilir (eşya → tarif → başka eşya), tarayıcı gibi "geri" düğmesi olur.

**Verilmiş kararlar (benim):**
- Veri kaynağı tek: `rules.toml` (tarifler, binalar) + kaynak düğümleri. Elle yazılmış yeni metin tablosu yok;
  her şey kurallardan türetilir, kural değişince kitap kendiliğinden güncellenir.
- Bilinmeyen tarif ve görülmemiş eşya "?" olarak görünür (`w.knows`, `w.has_seen`), sır vermez.
- Gösterim algıya göre büyür (UI tasarımındaki kural): taş devrinde resimli, sonra kelimeli, sonra sayılı.
- Sayfa Defter penceresinin içinde açılır (yeni pencere değil). Makine açıkken tarife tıklamak o tarifi makinede seçer
  (UI tasarımında zaten var: `info::set_recipe_pick`).
- Kısayol: bilgi panelinde ya da kutucukta eşyanın üstündeyken **R** (ya da orta tık) o eşyanın sayfasını açar.

**Görev taslağı:**
| # | İş | Model |
|---|---|---|
| 1.1 | `alaz_sim` içinde salt okunur dizin: `made_by(item)`, `used_in(item)`, `found_in(item)` (kurallardan, test edilebilir) | Sonnet |
| 1.2 | Defter'de "Eşya" sayfası + gezinme geçmişi (geri/ileri), bağlantılar | Sonnet |
| 1.3 | Kutucuk/bilgi panelinden sayfa açma (R, orta tık), makinede tarif seçme | Sonnet |
| 1.4 | Algıya göre gösterim, ekran görüntüleri (`--ui-shot` sayfası), elle kontrol | Opus |
| 1.5 | README, test taraması | Haiku |

**Karar (Göktuğ, 2026-11-19):** Kısayol **U tuşu** ("Bu ne?"). R, "Yollar" sekmesinde kalır.

---

## Proje 2: İlerleme ağacı

**Amaç:** Defter'in "Keşifler" sayfası düz liste yerine bir **ağaç** olsun: keşifler kutucuklar, aralarında "önce bunu
bil" okları. Ana yol (taş devri → bilgisayar çağı) altın şeritte, yan dallar (dokuma, tütsü, cam, telgraf…) kendi
şeritlerinde. Bilinenler dolu, sıradakiler parlak çerçeveli, uzaktakiler soluk "?". Kutucuğa tıklayınca o keşfin
sayfası: neyi açtığı (tarifler, binalar), nasıl bulunur ipucu (algıya göre), ilgili eşya sayfalarına bağlantı (Proje 1).

**Verilmiş kararlar (benim):**
- Ağaç **kurallardan türetilir**, elle çizilmez: bir keşif, kendi tariflerinin girdilerini (ve istasyon binasını)
  üreten keşiflere bağlıdır. Sim'e yeni veri eklenmez; döngü/eksik kenar çıkarsa test yakalar.
- Ana yol = çağ ilerlemesini (algı seviyesi) açan keşiflerin zinciri; geri kalan her şey yan dal. Şerit ataması koddan.
- Yerleşim: soldan sağa çağlar (taş, metal, makine, bilgisayar = 4 sütun bandı), yukarıdan aşağı şeritler.
  Yakınlaştır/kaydır (fare tekerleği + sürükle). Piksel stil, çağ malzemesi.
- "Sıradaki" = önkoşulları bilinen ama kendisi bilinmeyen keşif. Hedef şeridindeki (H) öneri de buradan beslenir.
- Openworld'deki gibi tarayıcıda değil, **oyunun içinde**.

**Görev taslağı:**
| # | İş | Model |
|---|---|---|
| 2.1 | `alaz_sim::journal` içinde keşif grafiği: kenarlar, şeritler, "sıradaki" (testler: döngü yok, her keşfe ulaşılır) | Opus |
| 2.2 | Ağaç yerleşimi (çağ bantları × şeritler), kenar yönlendirme, yakınlaştır/kaydır | Opus |
| 2.3 | Keşif kutucuğu + keşif sayfası, eşya sayfalarına bağlantı | Sonnet |
| 2.4 | Hedef şeridi (H) "sıradaki"yi buradan alsın | Sonnet |
| 2.5 | Ekran görüntüleri, README, test taraması | Haiku |

**Karar (Göktuğ, 2026-11-19):** Uzaktaki (önkoşulu bilinmeyen) keşifler **soluk "?"** olarak görünür. Oyuncu yolun
uzunluğunu görür, içeriğini görmez.

---

## Proje 3: Kodla internetten katılma

**Amaç:** Bugün sadece aynı ağda (LAN) oynanabiliyor. Openworld'deki gibi: sunucu kuran oyuncu "Ağa aç" der, 6 haneli
bir kod alır (ör. `K7Q-4XM`); arkadaşı başka bir şehirden "Kodla katıl"a kodu yazar, bağlanır. Port açmak, IP
bilmek gerekmez.

**Nasıl (önerilen yol):** Küçük bir **aracı sunucu** (`alaz-relay`, yeni crate, tek ikili dosya):
- Sunucu kuran oyun aracıya bağlanır, bir kod alır. Katılan oyun kodla aracıya bağlanır. Aracı iki bağlantıyı
  eşler ve baytları olduğu gibi aktarır. Oyunun lockstep protokolü değişmez (aracı içeriği okumaz).
- Her iki taraf da dışarı doğru bağlandığı için NAT/port açma derdi yok.
- Güvenlik: kodlar tahmin edilemez (en az 30 bit), 10 dakikada geçersiz olur, kod başına deneme sınırı, IP başına
  bağlantı sınırı, en fazla 8 oyuncu; aracı TLS ile konuşur; kayıt tutmaz (sadece sayaç).
- LAN yolu aynen kalır; "Ağdaki oyunlar" listesi de.

**Seçenekler (sunucu nerede çalışacak, senin kararın):**
| | Nerede | Artı | Eksi |
|---|---|---|---|
| A | Kendi bulut sunucun (ör. Oracle Cloud'un ücretsiz VM'i) | Ücretsiz, tam kontrol | Kurulum ve bakım sende; sunucu kapanırsa kodla katılma durur |
| B | titan-home + alan adı/port yönlendirme | Ek makine yok | Ev bağlantısına bağlı; titan bugün bir saat ağdan düştü; ev IP'si açığa çıkar |
| C | Doğrudan bağlantı + UPnP ile port açma (aracı yok) | Sunucu gerekmez | Birçok modemde çalışmaz; kod yerine IP paylaşılır |

**Önerim: A.** Aracı küçük ve hafif; ücretsiz bir bulut VM'i yeter. B ev ağını dışarı açar, C güvenilmez.

**Görev taslağı (A seçilirse):**
| # | İş | Model |
|---|---|---|
| 3.1 | `alaz-relay` crate: eşleme, aktarma, kod üretimi, sınırlar, TLS; yerel testler | Opus |
| 3.2 | `alaz-net`: aracı üzerinden `Host`/`Client` (aynı akış arayüzü), testler | Opus |
| 3.3 | Menü: "Ağa aç" (kod göster/kopyala), "Kodla katıl" alanı, hata mesajları | Sonnet |
| 3.4 | Sunucu kurulumu: systemd birimi, güncelleme betiği, izleme | Sonnet (kurulumu sen onaylarsın) |
| 3.5 | Güvenlik incelemesi (kod tahmini, DoS, kaynak tüketimi) | Opus |
| 3.6 | README, sürüm notu | Haiku |

**Kararlar (Göktuğ, 2026-11-19):**
- Alan adı **var**: `alazlab.com`; TLS sertifikası onun üzerinden alınır. Aracı için bir alt alan adı kullanılır (öneri: `relay.alazlab.com`).
- Kodla katılma **ücretsiz**; yukarıdaki sınırlar yeterli.

**Açık soru (senin):** Sunucu nerede: A, B ya da C? **Henüz karar yok.** Proje 3 bu karara kadar bekler; 1 ve 2
etkilenmez.

---

## Kalan işler (bu plan dışında, sırada)
- UI yeniden tasarımını bitirmek (Görev 3–7), elle kontrol, `main`'e birleştirme ve sürüm (onayınla).
- Clippy'nin eski `approx_constant` hataları (`icons.rs:683`, `menu.rs:766`): küçük bir temizlik commit'i.
