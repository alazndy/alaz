# alaz-relay sunucu kurulumu (RUNBOOK)

Bu dosya yazıldı, **çalıştırılmadı**. Hiçbir adım Göktuğ onaylamadan sunucuda yapılmaz.

## Açık sorular (Göktuğ cevaplayacak)

1. **DNS sağlayıcısı hangisi?** `alazlab.com` alan adı nerede yönetiliyor? Bilinmiyor.
2. **Bulut hesabı hangisi?** Örnek olarak Oracle Cloud'un ücretsiz VM'i kullanıldı. Henüz hesap yok.
3. **Let's Encrypt için e-posta** verilsin mi? Script şimdilik e-postasız kayıt yapar (`--register-unsafely-without-email`). Süre uyarıları gelmez.

## Varsayımlar

- Sunucu Ubuntu veya Debian. Başka dağıtımda paket adları değişir.
- Sunucuda `systemd` var.
- Domain: `relay.alazlab.com`. Port: 443.
- Sertifika istemcisi: **certbot**. Doğrulama yöntemi: **HTTP-01 standalone**.

## Neden HTTP-01 (80/tcp)?

| Yöntem | Artı | Eksi |
|---|---|---|
| HTTP-01 standalone (seçilen) | Sağlayıcıdan bağımsız. API anahtarı gerekmez. | 80/tcp sürekli açık kalmalı. |
| DNS-01 | 80/tcp gerekmez. | DNS sağlayıcısının API anahtarı sunucuda durur. Sağlayıcıya bağlıdır. |

Sağlayıcı belli olmadığı için HTTP-01 seçildi. DNS sağlayıcısı API verirse DNS-01'e geçmek mümkün.

## Dosyalar

| Dosya | Ne yapar |
|---|---|
| `alaz-relay.service` | systemd servisi. Yetkisiz `alaz-relay` kullanıcısı ile çalışır. |
| `deploy.sh` | Kullanıcıyı, klasörleri, binary'yi ve servisi kurar. Güncellemede de aynı script. |
| `renew-cert.sh` | Sertifikayı alır, yeniler, relay'e SIGHUP gönderir. |

Kullanıcı seçimi: `DynamicUser` yerine sabit `alaz-relay` kullanıcısı. Sertifika dosyaları sabit bir gruba verilmeli. Anahtar dosyası (`privkey.pem`) `root:alaz-relay` ve `0640`. Başkaları okuyamaz.

## Kaynak hesabı (neden bu sayılar)

Relay her bağlantı için bir iş parçacığı (thread) açar. Bu yüzden her kaynak sınırı tek bir sayıdan türer: `max_total_conns = 3000` (`Limits::default()`). Kabul döngüsü soketi alır almaz bu sayıyı ayırır. Sınır aşılırsa soket sessizce kapatılır; iş parçacığı açılmaz, cevap yazılmaz.

| Kaynak | Hesap | Birim ayarı |
|---|---|---|
| Soket | en çok 3000. Bir tünel 2 soket, bir oda 1: 1000 tünel x 2 + 500 oda = 2500; 500'ü el sıkışan bağlantılara pay | |
| İş parçacığı | soket başına 1 (tünelde eşleşen taraf 2 iş parçacığı kullanır, host'un kabul soketinin iş parçacığı biter) + 4 servis iş parçacığı = en çok 3004 | `TasksMax=3200` |
| Dosya tanımlayıcı | soket başına 2 (akış + `try_clone`) = 6000, artı dinleyiciler | `LimitNOFILE=16384` |
| Bellek | iş parçacığı başına yaklaşık 110 KiB (128 KiB yığın, TLS durumu, 16 KiB kopya tamponu) x 3000 = yaklaşık 330 MiB | `MemoryHigh=400M`, `MemoryMax=512M` |

Sayılar tahmindir; ilk gerçek kurulumda `systemctl status alaz-relay` ve `systemd-cgtop` ile ölçüp ayarla. Küçük VM'de (1 GB) bu sınırlar relay'i tek başına bellek tüketmekten korur. İş parçacığı açılamazsa ya da sınır aşılırsa relay çökmez, yalnız o bağlantıyı kapatır.

Diğer sınırlar (hepsi `Limits` içinde):

| Sınır | Değer |
|---|---|
| IPv4 adresi / IPv6 /64 başına bağlantı | 64 (bir /48 en çok 4 katı = 256) |
| IP (IPv6'da /48) başına oda | 4 |
| IP (IPv6'da /48) başına katılımcı tüneli | 16 |
| Yanlış kod: pencere ve kilit | 60 sn'de 10 hata (/48'de 40), sonra 5 dk kilit; kilitli IP'nin Join'i reddedilir |
| Genel katılma bütçesi | 20 deneme/sn. Yakın zamanda hata yapmış IP'ler bunun 10'unu kullanabilir; 10'u hata yapmamış IP'lere ayrılmıştır |
| Boşta tünel | 300 sn iki yönde de bayt yoksa kapanır. Uzun tutuldu: küçültülmüş ya da başka pencerenin altında kalan bir oyun turları göndermeyi durdurabilir; host oyuncularını kaybetmemeli |
| Kilitli IP | Kilitli IP kapatılmaz: IP başına 8, toplam 256 soketlik küçük bir havuza girer. Open ve Accept çalışır (aynı NAT'taki host etkilenmez); Join "çok fazla deneme" ile reddedilir. Havuz dolarsa soket cevapsız kapanır |
| Oda / tünel (genel) | 500 / 1000 |

### Tarama maliyeti (bilinen, şimdilik kabul)

Her kontrol bağlantısı saniyede yaklaşık 50 kez, her tünel yönü saniyede yaklaşık 100 kez uyanır (20 ms ve 10 ms'lik okuma aralığı). Dolu bir relay'de (500 oda + 1000 tünel) bu, boştayken bile sürekli küçük bir işlemci yüküdür: kabaca 25000 + 200000 uyanma/sn. Küçük bir VM'de bir çekirdeğin bir kısmını yer. Sınırlar sayesinde üst sınır bellidir, ama ölçülmedi. Yük gerçek kullanımda sorun çıkarırsa (`top` ile `alaz-relay` sürekli yüksekse) sonraki adım: uzun zaman aşımlı bloklayan okuma veya tünel başına yoklama iş parçacığı. Şimdilik yapılmadı.

## Adım 1: VM ve ağ

1. Bulut sağlayıcıda küçük bir Linux VM aç (Oracle ücretsiz VM, Ubuntu veya Debian).
2. VM'e **sabit (reserved) bir genel IPv4 adresi** ver. IP değişirse DNS bozulur.
3. DNS sağlayıcısında bir **A kaydı** oluştur: ad `relay` (yani `relay.alazlab.com`), değer VM'in IPv4 adresi. **AAAA kaydı açma.** Relay şimdilik sadece IPv4'te dinler (`0.0.0.0:443`). IPv6 istemci AAAA görürse bağlanamaz.
4. Bulut ağ kurallarında (güvenlik listesi, security group) **gelen** trafiğe şunları aç:
   - 443/tcp: herkese. Relay burada dinler.
   - 80/tcp: herkese, **sürekli açık**. Yenileme düzenli çalışır. Certbot 80'i sadece yenilerken tutar, diğer zamanlarda orada hiçbir şey dinlemez.
   - 22/tcp: yönetim için, mümkünse sadece senin IP'ne.
   - Başka her şey kapalı. 9100 (istatistik) **hiç açılmaz**; sadece loopback'te dinler.
5. VM'in kendi güvenlik duvarı da aynı kurallara uymalı. Oracle'ın Ubuntu imajında `iptables` kuralları varsayılan olarak 22 dışını keser. Örnek (ufw):

```
sudo ufw allow 22/tcp
sudo ufw allow 443/tcp
sudo ufw allow 80/tcp
sudo ufw enable
```

   Oracle imajında ufw yerine `iptables` kuralı gerekebilir. Önce mevcut kuralları oku (`sudo iptables -S`), sonra karar ver.
6. DNS'in yayıldığını bilgisayarında kontrol et: `dig +short relay.alazlab.com` VM'in IP'sini vermeli.

**Not:** `ALAZ_RELAY` bir IP adresine (ör. `1.2.3.4:443`) işaret ederse TLS doğrulaması çalışmaz; sertifika IP için (IP SAN) düzenlenmemiştir. Her zaman ana makine adı kullan (`relay.alazlab.com:443`).

## Adım 2: Binary'yi derle ve kopyala

Derleme ayrı bir makinede veya VM'de yapılabilir. Dosyalar **sadece senin kullanıcına ait, 0700 modlu** bir klasöre konur (ortak `/tmp` klasörüne düz yazılmaz; başka bir yerel kullanıcı dosyaları sudo çalışmadan önce değiştirebilir). Derlediğin makinede SHA-256 özetini al, VM'de karşılaştır.

```
cargo build --release -p alaz-relay
sha256sum target/release/alaz-relay packaging/relay/{alaz-relay.service,deploy.sh,renew-cert.sh} > relay.sha256
dir=$(ssh USER@relay.alazlab.com 'umask 077; mktemp -d')
scp target/release/alaz-relay packaging/relay/{alaz-relay.service,deploy.sh,renew-cert.sh} relay.sha256 USER@relay.alazlab.com:"$dir"/
echo "$dir"
```

Binary Linux x86_64 veya VM'in mimarisi (Oracle ARM VM'de aarch64) için derlenmeli. Relay sadece Linux'ta çalışır.

## Adım 3: İlk kurulum (VM üzerinde)

```
sudo apt-get update && sudo apt-get install -y certbot
cd DIR          # Adım 2'nin yazdığı klasör
sha256sum --check --strict relay.sha256 || exit 1   # özet tutmuyorsa DUR; sudo çalıştırma
sudo ./deploy.sh ./alaz-relay        # kullanıcı, klasör, servis; sertifika yoksa başlatmaz
sudo ./renew-cert.sh issue           # sertifikayı alır, kopyalar
sudo systemctl restart alaz-relay
```

`renew-cert.sh issue` yenileme kancasını da kurar (aşağıya bak). Sertifika yokken servis kendiliğinden başlamaz (`ConditionPathExists`), yani yeniden başlatma döngüsüne girmez. İlk sertifikadan sonra `restart` onu başlatır. Sırasında 80/tcp boş ve açık olmalı. Başka bir web sunucusu 80'i tutuyorsa dur.

## Adım 4: Otomatik yenileme

Sertifika 90 gün geçerli. Debian ve Ubuntu'daki certbot paketi günde iki kez `certbot renew` çalıştırır ve süre 30 günün altına inince yeniler. Ayrı bir zamanlayıcı kurmana gerek yok.

`renew-cert.sh issue` bir **deploy hook** kurar: `/etc/letsencrypt/renewal-hooks/deploy/alaz-relay`. Certbot her yenilemeden sonra bunu çalıştırır. Hook yeni sertifikayı `/etc/alaz-relay/tls` içine kopyalar ve relay'e SIGHUP gönderir. Yenilemeyi hangi zamanlayıcı yaparsa yapsın bu çalışır.

Relay'in 12 saatlik yeniden okuması sadece bir yedektir. Kopya güncellenmediyse yeni sertifikayı getirmez.

## Güncelleme

Yeni binary'yi kopyala, sonra:

```
sudo ./deploy.sh ./alaz-relay
```

Servis yeniden başlar. O anda açık olan odalar ve tüneller kopar. Oyuncular "bağlantı koptu" görür. Oyun az kullanılan bir saatte güncelle.

## Doğrulama listesi

Hepsi geçmeden "tamam" deme.

- [ ] Servis çalışıyor: `systemctl is-active alaz-relay` çıktısı `active`.
- [ ] Port 443 dışarıdan açık ve sertifika geçerli (kendi bilgisayarından):

```
openssl s_client -connect relay.alazlab.com:443 -servername relay.alazlab.com </dev/null 2>&1 | grep -E 'Verify return code|subject=|notAfter'
```

  Beklenen: `Verify return code: 0 (ok)`. Konu `relay.alazlab.com`.
- [ ] Servis yetkisiz kullanıcıyla çalışıyor: `ps -o user= -C alaz-relay` çıktısı `alaz-relay`.
- [ ] Anahtar dosyası özel: `ls -l /etc/alaz-relay/tls/privkey.pem` çıktısı `-rw-r----- root alaz-relay`.
- [ ] İstatistik uç noktası sadece loopback'te. SSH tüneli ile oku (kendi bilgisayarında):

```
ssh -L 9100:127.0.0.1:9100 USER@relay.alazlab.com
# başka terminalde:
nc 127.0.0.1 9100
```

  Beklenen: `rooms`, `tunnels`, `refusals`, `lockouts`, `abuse_cuts` satırları. Dışarıdan `nc relay.alazlab.com 9100` bağlanamamalı.
- [ ] Oyundan katılma testi: `ALAZ_RELAY` ortam değişkeni **tanımlı olmadan** oyunu aç. Bir oyuncu "Ağa aç" ile kod alsın. İkinci oyuncu "Kodla katıl" ile o kodu girsin. İkisi aynı dünyada görünmeli.
- [ ] Sertifika yenileme testi (sunucuda): `sudo /usr/local/sbin/alaz-relay-renew renew`, hata vermemeli. Ardından `sudo journalctl -u alaz-relay -n 20` ile servisin düşmediğini gör.
- [ ] Yeniden başlatma testi: `sudo systemctl restart alaz-relay` sonrası servis kendiliğinden ayağa kalkar. VM'i yeniden başlatınca da kalkmalı (`systemctl is-enabled alaz-relay` çıktısı `enabled`).
- [ ] Hook kurulu: `ls -l /etc/letsencrypt/renewal-hooks/deploy/alaz-relay` dosyası var ve çalıştırılabilir. `sudo certbot renew --dry-run` hatasız biter.

Not: `shellcheck` bu makinede kurulu değildi, scriptler sadece `bash -n` ile denendi.
