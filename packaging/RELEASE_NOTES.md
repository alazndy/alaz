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
- **Sekmeler:** I çanta, C yap, B inşa, H hedefler, F1 yardım.

Ayrıntılar için README'ye bak.

## 0.2.0'da yeni
- Açılış ekranı ve dünyaya giriş animasyonu (ALAZ_NOFX=1 ile kapatılır).
- Sol tık binaları ve işçileri açar; binalar "Taşı" ile yer değiştirir.
- Yapım defteri resimli kartlar; arayüz boyutu Yardım (F1) sekmesinden ayarlanır.
- Z tuşu eşya bırakır; köpeğine sağ tıkla, sev.
- Balta sesi, yumuşak dönüş ve eğilme, toplanacak şey parlar, zamanlama oyununda isabet efektleri.
- "İlk adımlar" listesi hedef şeridiyle aynı sırada.
