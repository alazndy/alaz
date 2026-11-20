# Kodla internetten katılma: tasarım

Tarih: 2026-11-19. Bağlı plan: `2026-11-19-openworld-ideas-roadmap.md`, Proje 3.

## Amaç ve neden
Bugün çok oyunculu yalnızca aynı ağda (LAN) çalışıyor. Başka şehirdeki bir arkadaşın katılması için sunucu kuranın
modemde port açması ve IP'sini söylemesi gerekiyor. Hedef: sunucu kuran menüde "Ağa aç" der, kısa bir kod alır
(ör. `K7Q-4XM`); arkadaşı "Kodla katıl"a kodu yazar ve bağlanır. Port açmak, IP bilmek yok.
Kod yalnızca sahibinin doğrudan verdiği arkadaşlara gidecek (Discord'da herkese açık paylaşılmayacak).

## Hedefler
- İki oyuncu, hiçbiri port açmadan, kodla bağlanır.
- LAN ve internet aynı lockstep oyun protokolünü kullanır. Aracı bu mesajları yorumlamadan iletir.
- Sunucu her oyuncu komutunun bağlantıya atanmış karaktere ait olduğunu doğrular. Oyuncu ortak işçi komutlarında işçiyi hedef olarak seçebilir.
- LAN yolu ve "Ağdaki oyunlar" listesi aynen çalışmaya devam eder.
- Aracı sunucu küçük, tek ikili dosya, ücretsiz bir bulut VM'inde `relay.alazlab.com` adresinde çalışır.

## Hedef olmayanlar
- Hesap, arkadaş listesi, davet bildirimi (Essential gibi). Yerine kod.
- Doğrudan bağlantı / NAT delme (ICE). Aracı her durumda çalışır; doğrudan bağlantı ileride eklenebilir.
- Sohbet, sunucu listesi, herkese açık eşleme.

## Kaçınılacak sonuçlar (çalışsa bile başarısızlık sayılır)
- Aracının oyun içeriğini okuyup kaydetmesi ya da bir yere yazması.
- Kodun tahmin edilip yabancının oyuna girmesi.
- Aracının tek bir kötü niyetli bağlantıyla kapanması ya da VM'in kaynağını tüketmesi.
- Oyunun menüde ya da bağlanırken donması (mevcut menü davranışı korunur: bağlanma arka planda, hata menüde).

## Kısıtlar
- Ağ kodu senkron ve bilerek asenkron çalışma zamanı yok (`crates/net`, `Conn` bir `TcpStream` sarar). Oyun tarafı bunu korur.
- Alan adı: `alazlab.com`; aracı için `relay.alazlab.com`. TLS sertifikası bu ad için alınır.
- Sunucuya kurulum ve kullanıcı hesabı işleri her adımda Göktuğ'un onayıyla yapılır.
- Aynı çalışma klasöründe başka bir oturum (fizik/oynanış) çalışıyor: `crates/sim` ve `main.rs`'e yalnızca gerektiği kadar dokunulur.

## Yaklaşımlar (üç tane değerlendirildi; Yol 1 seçildi)
1. **Aracı iki soketi birleştirir (seçilen).** Host aracıya bir kontrol bağlantısı açar ve kod alır. Katılan kodla aracıya
   bağlanınca aracı kontrol bağlantısından host'a "gelen: id" der. Host her gelen için aracıya yeni bir bağlantı açıp
   "kabul: id" der; aracı bu iki soketi uç uca bağlar. `Host` bunu `accept()` ile gelmiş bir akış gibi görür.
   Artı: protokol ve oyun değişmez, aracı basit. Eksi: oyuncu başına bir bağlantı (en çok 8).
2. Tek bağlantı, aracı oyuncuları numarayla ayırır. Eksi: `Host` ve çerçeveleme baştan yazılır, aracı protokolü bilmek zorunda kalır.
3. Hazır QUIC kütüphanesi (`quinn`). Eksi: asenkron çalışma zamanı (`tokio`) getirir, senkron ağ kodu büyür.

## Tasarım

### Parçalar
- **`alaz-relay` (yeni crate, tek ikili dosya).** Kontrol bağlantısı karşılar, kod üretir, katılanı eşler, iki soketi birleştirir.
  Birleştikten sonra baytlara bakmaz. Kayıt tutmaz, yalnızca sayaçlar (açık oda, aktif tünel, reddedilen deneme).
- **`alaz-net` değişiklikleri.** `Conn` düz soket ya da TLS soket tutabilir. `Host` için `open_relay` (kod döner) ve `poll`
  içinde gelen tünelleri kabul etme. `Client` için `connect_code(code)`. Mevcut `Host::bind` / `Client::connect` aynen kalır.
- **Menü.** Çok oyunculu ekranında "Ağa aç" (kodu gösterir ve panoya kopyalar) ve "Kodla katıl" alanı. Hatalar mevcut
  `mp_error` ile kırmızı gösterilir.

### Aracı protokolü
Kendi sürüm numarası var (`RELAY_PROTOCOL`), oyunun `PROTOCOL`'ünden ayrı; oyun protokolü değişmediği için o artmaz.
Mesajlar (postcard, aynı uzunluk-önekli çerçeve): kuran tarafı `Open → Opened{code}`, katılan tarafı `Join{code} → Paired | NoSuchCode | Full | Refused`,
kontrol bağlantısında `Incoming{id}`, kuranın yeni bağlantısında `Accept{id}`. Eşleşme bekleyen bağlantılar 10 sn içinde
kapanır. Birleştikten sonra akış yalnızca ham bayttır.

### Kod ve sınırlar
- Kod **40 bit** rastgele (işletim sisteminin güvenli rastgele kaynağından): 32 harflik okunaklı alfabe (0/O, 1/I yok) ile 8 karakter, `XXXX-XXXX` biçimi. *(Builder kontrolünden sonra benim kararım: 6 karakter tam 30 bit idi ve oturum boyu geçerli bir kod için dar.)*
- Kod, kuran oyuncunun kontrol bağlantısı açık olduğu sürece geçerli; kuran menüden "yeni kod" alabilir (eskisi hemen geçersiz).
  **Bu benim kararım:** yol haritasındaki "10 dakikada geçersiz" arkadaşlar bir saat sonra katılınca işe yaramaz. Kodun
  ömrü oturum, katılma denemesi ise sınırlı (aşağıda). Menüde kuran **"Ağı kapat"** diyebilir: kod hemen geçersiz olur, bağlı oyuncular kalır.
  Kuran, menüde bağlı oyuncu sayısını görür. Atma/onaylama yok (hedef olmayan); kuran gerekirse ağı kapatıp yeni kod alır.
- Sınırlar: IP başına dakikada başarısız deneme sayısı (aşılınca o IP geçici engellenir) ve genel (tüm IP'ler) deneme hızı üst sınırı,
  IP başına eşzamanlı bağlantı, oda başına en çok 8 oyuncu, toplam oda ve tünel sayısı, kontrol bağlantısında hareketsizlik zaman aşımı.
- **Tünel hız politikası (benim kararım):** tünel başına toplam bayt üst sınırı **yok** (anlık görüntü ve yeniden eşitleme meşru olarak büyük
  mesajlar gönderir; kesmek host kopması gibi görünür). Bunun yerine hız **kısılır** (jeton kovası, büyük patlama payıyla; sürekli hız lockstep
  trafiğinin çok üstünde). Kesme yalnızca aşırı kötüye kullanımda (ör. bir tünelde günlük çok yüksek toplam) ve aracı bunu sayaçta işaretler.
  Sayılar ayarlanabilir; builder ölçümden sonra belirler.
- Başarısız denemeler kodun var olup olmadığını sızdırmaz: yanıt ve süre aynı.

### Güvenlik
- Tüm bağlantı TLS (rustls; sertifika `relay.alazlab.com` için). **Benim kararım:** oyun Windows ve Linux için yayınlanıyor, bu yüzden
  rustls'in `ring` sağlayıcısı (cmake/C araç zinciri gerektirmez) ve paketlenmiş web kök listesi (`webpki-roots`) kullanılır; sistem kök deposu değil.
  Aracı TLS'i sonlandırdığı için baytları teknik olarak görebilir; bu yüzden kaçınılacaklar listesinde "okuma, saklama yok" yer alıyor.
- `MAX_FRAME` ve mevcut çerçeve sınırları aracı protokolünde de uygulanır (aracı ham akışı çözmez, ama eşleşme mesajlarını çözer).
- Aracı yalnızca eşleşme mesajı için çerçeve çözer; tünel bağlandıktan sonra düz aktarım. Kod ve sır logda yer almaz.
- Aracıya bağlanma hedefi sabit (`relay.alazlab.com`, `ALAZ_RELAY` ile değiştirilebilir, yalnızca test/kendi barındırma için); kullanıcıdan gelen serbest URL kabul edilmez (SSRF).
- Bağımlılıklar eklenmeden önce `cargo audit`.
- **Kaynak sınırları (güvenlik incelemesi sonrası):** toplam soket sınırı 3000, kabul döngüsünde, iş parçacığı açılmadan ayrılır; aşan soket
  sessizce kapanır (cevap yok). İş parçacıkları küçük yığınla açılır ve açılamazsa bağlantı bırakılır (panik yok). IP başına
  bağlantı sınırı TLS el sıkışmasından önce, kabulde uygulanır. Kilitli IP kapatılmaz: IP başına 8, toplam 256 soketlik havuza girer;
  Open ve Accept çalışır (aynı NAT'taki host etkilenmez), Join "çok fazla deneme" ile reddedilir; havuz dolarsa soket cevapsız kapanır. IPv6 iki düzeyde sayılır (/64 ve /48, sıkı olan geçerli). IP (IPv6'da /48)
  başına en çok 4 oda ve 16 katılımcı tüneli. Katılma bütçesi 20/sn; bunun yarısı yakın zamanda hata yapmamış IP'lere ayrılmıştır.
  Eşleşmiş tünel 300 sn iki yönde de bayt görmezse kapanır (küçültülmüş pencerede oyun döngüsü durabilir; host oyuncularını kaybetmemeli). Oyun tarafı: kimliği doğrulanmamış (Hello göndermemiş) bağlantı en çok 4 KiB
  tamponlar; Hello'dan sonra istemciden hosta çerçeve en çok 1 MiB (hosttan istemciye 64 MiB). İstemci en çok 16 eşzamanlı tünel açar,
  işlenmemiş tur birikimi 10 000'i geçerse bağlantıyı keser; uzaktan gelen hata metni menüde 200 karaktere kısaltılır.
  "Ağa aç" yalnızca `127.0.0.1` (rastgele port) dinler ve LAN keşfini açmaz; LAN sunucusu ("Sunucu kur") değişmedi.

### Hata durumları (kullanıcı ne görür)
Aracıya ulaşılamıyor; kod yok ya da süresi bitti; oda dolu; sürüm uyuşmuyor; host koptu; aşırı deneme. Her biri menüde
kısa Türkçe bir cümle. Bağlantı koparsa oyun donmaz; mevcut "bağlantı koptu" davranışı geçerli.

### Bilinen risk: dünya anlık görüntüsü
Katılan oyuncuya tüm dünya tek mesajla gider (`Welcome.snapshot`). İnternette büyük bir dünya yavaş açılabilir.
Builder ilk iş tipik bir dünyanın boyutunu ölçer; büyükse sıkıştırma ekler (protokol sürümü artırılır).

**Ölçüm (2026-11-19):** `World::to_bytes()`, varsayılan harita 256x256, seed 5, 3 oyuncu, release derleme
(`cargo test -p alaz-net --release --test snapshot_size -- --ignored --nocapture`; test süresi yaklaşık 3 sn):
tick 0 = 7745 bayt, tick 1000 = 7912 bayt, tick 20000 = 7958 bayt (yaklaşık 8 KB).
Bu ölçümdeki dünya 2 MiB sınırının altında kaldı. Oyun protokolü 7, bağlantıya atanan oyuncuyu ve dünya değiştiğinde yeni oturumu taşır.

### Test
- Aracı için yerel testler: eşleme, tünel aktarımı, yanlış kod, dolu oda, süre aşımı, hız ve bağlantı sınırları (kendinden imzalı sertifikayla).
- Uçtan uca: host + client aracı üzerinden, mevcut `run` yardımcısıyla birkaç yüz tick, hash eşit.
- Menü testleri (mevcut egui_kittest kalıbı): "Ağa aç" kod gösterir, "Kodla katıl" hatalı kodu gösterir.
- Elle: iki gerçek ağdan bir deneme (Göktuğ'la).

## Sunucu kurulumu (ayrı adım, onaylı)
Ücretsiz bulut VM'i, systemd birimi, güncelleme betiği, sertifika yenileme, izleme. Ben hazırlarım; sunucuda çalıştırmadan önce
her adım için Göktuğ'a sorulur. DNS'in nereden yönetildiği ve bulut hesabı **açık soru** (yalnızca bu adımı etkiler).

## Builder'a bırakılanlar
Aracının iş parçacığı düzeni (iş parçacığı mı, `mio` gibi olay döngüsü mü; `tokio` hariç), rustls sarmalayıcısı ve
`Conn`'un genelleştirilmesi, sertifika alma aracı (ACME), kod alfabesi ayrıntısı, sıkıştırma kütüphanesi, sınır sayıları (makul varsayılanlar, ayarlanabilir).

## İşler (roadmap 3.1-3.6)
3.1 `alaz-relay` · 3.2 `alaz-net` aracı desteği + ölçüm · 3.3 menü · 3.4 sunucu kurulumu · 3.5 güvenlik incelemesi · 3.6 README ve sürüm notu.
