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
| HTTP-01 standalone (seçilen) | Sağlayıcıdan bağımsız. API anahtarı gerekmez. | Yenileme sırasında 80/tcp açık olmalı. |
| DNS-01 | 80/tcp gerekmez. | DNS sağlayıcısının API anahtarı sunucuda durur. Sağlayıcıya bağlıdır. |

Sağlayıcı belli olmadığı için HTTP-01 seçildi. DNS sağlayıcısı API verirse DNS-01'e geçmek mümkün.

## Dosyalar

| Dosya | Ne yapar |
|---|---|
| `alaz-relay.service` | systemd servisi. Yetkisiz `alaz-relay` kullanıcısı ile çalışır. |
| `deploy.sh` | Kullanıcıyı, klasörleri, binary'yi ve servisi kurar. Güncellemede de aynı script. |
| `renew-cert.sh` | Sertifikayı alır, yeniler, relay'e SIGHUP gönderir. |

Kullanıcı seçimi: `DynamicUser` yerine sabit `alaz-relay` kullanıcısı. Sertifika dosyaları sabit bir gruba verilmeli. Anahtar dosyası (`privkey.pem`) `root:alaz-relay` ve `0640`. Başkaları okuyamaz.

`LimitNOFILE=65536`. Hesap: en fazla 4000 tünel x 2 soket + 1000 oda x 1 kontrol soketi = 9000. Geri kalan, el sıkışan ve saldıran bağlantılar için pay.

## Adım 1: VM ve ağ

1. Bulut sağlayıcıda küçük bir Linux VM aç (Oracle ücretsiz VM, Ubuntu veya Debian).
2. VM'e **sabit (reserved) bir genel IPv4 adresi** ver. IP değişirse DNS bozulur.
3. DNS sağlayıcısında bir **A kaydı** oluştur: ad `relay` (yani `relay.alazlab.com`), değer VM'in IPv4 adresi. IPv6 varsa aynı şekilde bir AAAA kaydı.
4. Bulut ağ kurallarında (güvenlik listesi, security group) **gelen** trafiğe şunları aç:
   - 443/tcp: herkese. Relay burada dinler.
   - 80/tcp: sadece sertifika alırken ve yenilerken (HTTP-01).
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

## Adım 2: Binary'yi derle ve kopyala

Derleme ayrı bir makinede veya VM'de yapılabilir.

```
cargo build --release -p alaz-relay
scp target/release/alaz-relay packaging/relay/{alaz-relay.service,deploy.sh,renew-cert.sh} USER@relay.alazlab.com:/tmp/relay/
```

Binary Linux x86_64 veya VM'in mimarisi (Oracle ARM VM'de aarch64) için derlenmeli. Relay sadece Linux'ta çalışır.

## Adım 3: İlk kurulum (VM üzerinde)

```
sudo apt-get update && sudo apt-get install -y certbot
cd /tmp/relay
sudo ./deploy.sh ./alaz-relay        # kullanıcı, klasör, servis; sertifika yoksa başlatmaz
sudo ./renew-cert.sh issue           # sertifikayı alır, kopyalar
sudo systemctl restart alaz-relay
```

`renew-cert.sh issue` sırasında 80/tcp boş ve açık olmalı. Başka bir web sunucusu 80'i tutuyorsa dur.

## Adım 4: Otomatik yenileme

Sertifika 90 gün geçerli. Haftada iki kez yenileme denenir. Yenileme yalnızca süre azaldığında olur.

```
sudo install -m 0755 /tmp/relay/renew-cert.sh /usr/local/sbin/alaz-relay-renew
echo '17 3 * * 1,4 root /usr/local/sbin/alaz-relay-renew renew' | sudo tee /etc/cron.d/alaz-relay-renew
```

Debian ve Ubuntu'daki certbot paketi kendi zamanlayıcısını da kurar. Zararsızdır, ama relay'e SIGHUP göndermez. Relay ayrıca her 12 saatte bir sertifikayı kendisi yeniden okur. Yani en kötü durumda yeni sertifika 12 saat içinde devreye girer.

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
