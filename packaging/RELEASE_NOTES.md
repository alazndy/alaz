Taş devrinden bilgisayar çağına uzanan bir hayatta kalma, keşif ve otomasyon oyunu. Ağaçtan yeni inmiş bir maymun-insanla başlarsın; ateşi, çömleği, metali, buharı, elektriği bulur, sonunda kendi kurallarını yazan bir bilgisayar kurarsın.

## Kurulum

### Windows (10 / 11, 64 bit)
- **Kurulum:** `ALAZ-…-windows-setup.exe` dosyasını indirip çalıştır. Oyun Program Files'a kurulur; Başlat menüsüne (istersen masaüstüne) kısayol eklenir. Kaldırmak için Ayarlar → Uygulamalar.
- **Kurmadan:** `ALAZ-…-windows-portable.zip` dosyasını bir klasöre aç, `alaz.exe`'yi çalıştır.
- **SmartScreen uyarısı:** Dosya imzalı olmadığı için Windows "Bilinmeyen yayımcı" uyarısı gösterebilir: **Ek bilgi → Yine de çalıştır**.

Kayıtlar ve tuş ayarları `%APPDATA%\ALAZ` klasöründe durur; oyun kaldırılınca silinmez.

### Linux (x86_64)
```sh
tar -xzf ALAZ-*-linux-x86_64.tar.gz
cd ALAZ-*-linux-x86_64
./install.sh          # ~/.local/bin/alaz ve uygulama menüsü; sudo gerekmez
```

- **Kaldırmak:** `./uninstall.sh` (kayıtlar `~/.local/share/alaz` altında kalır).
- **Kurmadan:** Klasördeki `alaz` dosyasını doğrudan da çalıştırabilirsin.
- **Gerekenler:** Vulkan destekli bir ekran kartı sürücüsü (Mesa ya da NVIDIA), Wayland ya da X11.
- **Ses:** PipeWire (`pw-cat`) ya da ALSA (`aplay`). İkisi de yoksa oyun sessiz çalışır.

## İlk adımlar
- Sağ üstteki "İlk adımlar" listesi ve üstteki sarı hedef şeridi yolu gösterir.
- **Hareket:** WASD ile yürü, Shift ile koş.
- **Fare:** Sağ tıkla topla; imleç neyin üstündeyse adını yazar.
- **Sekmeler:** I çanta, C yap, B inşa, H hedefler, J günlük, F1 yardım.

Ayrıntılar için README'ye bak.

## 0.4.0'da yeni

**Gezilebilir mağaralar:** Mağaralar artık bir panel değil, içine girip yürüdüğün yerler. Elinde meşaleyle ağza yürü (ya da sağ tıkla): her mağara dünyanın tohumundan üretilen dört katlı bir yeraltı; odalar, dolambaçlı tüneller, katlar arası merdivenler.
- WASD ve tıkla-yürü mağarada da çalışır; duvarlardan geçilmez.
- Duvara sağ tıkla kaz (kazma gerekir); derine indikçe başka cevherler.
- Işık yalnız meşaleden gelir, duvarlar onu keser. Meşale bitince karanlık: yavaşça el yordamıyla yürürsün, bir ok çıkışı gösterir.
- Derinde ve kazarken tavan çökebilir; düşen taşları kazıp temizlersin. En dip kat su basmıştır, 15 m içindeki bir pompa onu kurutur.
- Sağ üstte kat, meşale süresi, çıkış oku ve gezdiğin yerlerin haritası.
- Eski kayıtlar açılır; mağaradayken kaydedilmiş bir oyunda mağara ağzının yanında başlarsın.

## 0.3.2'de yeni

**Karakter evrimi:** Maymun'dan Bilge'ye 10 aşama (Maymun, Dik yürüyen, Alet yapan, Ateşçi, Toplayıcı, Avcı, Çiftçi, Zanaatkâr, Usta, Bilge). Duruş, kürk, yüz, boy ve yürüyüş algıyla birlikte azar azar değişiyor; kemer, boya, torba, kolye, hasır şapka, kuşak ve ak sakal gibi ayrıntılar sırayla beliriyor. Yeni bir aşamaya geçince görünüş birkaç saniyede kayarak değişir ve bir kart çıkar. Portre de aynı evrimi gösterir.

(0.3.0 ve 0.3.1 yayınlanmadı; aşağıdakiler de bu sürümle geliyor.)

**53 yeni makine**, üç dalgada:
- **Sanayi:** kok fırını, kireç ve çimento fırınları, tuğla fırını, pudlaj fırını, Bessemer konvertörü, döküm ocağı ve kum kalıp, torna, matkap/freze, pres, testere değirmeni, çıkrık, mekanik dokuma tezgahı, kazan, buhar çekici, haddeleme hattı.
- **Yaşam ve zanaat:** çömlekçi çarkı, yağ presi, tabakhane, kurutma rafı, tüttürme ocağı, kuyu, dibek, su değirmeni, dövme çekici, keçe tokmağı, halat bükme, vinç, cam üfleme, kâğıt ve matbaa, çırçır (pamukla), iplik eğirme makinesi.
- **Elektrik ve ileri dönem:** ark fırını, rafineri, damıtma kazanı, elektroliz (boksit ve alüminyum), kompresör, demiryolu, buharlı pompa, transformatör, akümülatör, kaynak, kimya reaktörü, soğutucu, telgraf, radyo, vakum pompası, kristal çekme, fotolitografi, CNC, montaj robotu.
- Eski üretim yolları açık kalıyor; yeni makineler daha hızlı ya da daha verimli yollar ekliyor.

**Kamp ve günlük:** Akşam yanan bir ateşin başına otur: günün özeti, dinlenme ve ısınma, yarın için bir hedef. Günlük (J) gün gün olayları, keşifleri ve bütün bilinen tarifleri tutar; elde kâğıt varsa yerleri de not eder.

**Elementler:** Islak zemin, çamur, su birikintisi, buz; kuru otta rüzgârla yayılan yangın, kül ve yeniden yeşerme. Kamp ateşinin kıvılcımı ve yaz yıldırımı yangın başlatabilir.

**Giysiler artık işe yarıyor:** ısı, yağmur, ısırık, darbe, düşme ve ateş koruması; yük ve aşınma. Giyim sıcaklığı durum kutusunda görünür.

**Görüntü ve performans:**
- Daha ince pikseller, yumuşak zemin geçişleri, yeni karakter tasarımı, çağa göre kıyafetler ve maymunsu portre.
- Grafik ayarları (menü → Grafik): piksel inceliği, gölge kalitesi, uzak süsler.
- Akıcı yürüyüş: dünya parçaları arka planda kuruluyor. Kalabalıkta uzak insanlar sadeleşiyor.
- Ctrl + / Ctrl - / Ctrl 0 arayüz boyutunu ayarlar ve hatırlar.

**Modlama:** `assets/` klasörüne MagicaVoxel (`.vox`) ya da glTF (`.glb`) model, PNG ikon, arayüz parçası ve zemin dokusu konabilir; kodla çizilenin yerine geçer. `alaz --export-assets klasör` mevcut modelleri ve ikonları düzenlenmek üzere dışa aktarır.

**Kayıtlar:** 0.2.0 kayıtları açılır ve yükseltilir.

## 0.2.0'da yeni
- Açılış ekranı ve dünyaya giriş animasyonu (ALAZ_NOFX=1 ile kapatılır).
- Sol tık binaları ve işçileri açar; binalar "Taşı" ile yer değiştirir.
- Yapım defteri resimli kartlar; arayüz boyutu Yardım (F1) sekmesinden ayarlanır.
- Z tuşu eşya bırakır; köpeğine sağ tıkla, sev.
- Balta sesi, yumuşak dönüş ve eğilme, toplanacak şey parlar, zamanlama oyununda isabet efektleri.
- "İlk adımlar" listesi hedef şeridiyle aynı sırada.
