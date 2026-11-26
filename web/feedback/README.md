# ALAZ geri bildirim sitesi

Oyundan gelen hata ve istek raporlarını alır. Sahibi ve davet edilenler e-posta koduyla girip okur.
Tasarım: `docs/superpowers/specs/2026-11-26-feedback-reports-design.md`.

## Geliştirme

```sh
pnpm install
pnpm test        # gerçek bir Postgres testin içinde çalışır (PGlite)
pnpm typecheck
```

Bulut hesabı olmadan rapor kabul eden yerel sunucu:

```sh
cp .env.example .env.local     # IP_HASH_SECRET doldur, FEEDBACK_LOCAL_DIR=.local yap
pnpm dev                       # http://localhost:3000/api/reports
```

Oyunu bu sunucuya yönlendirmek için: `ALAZ_FEEDBACK_URL=http://localhost:3000 cargo run --release -p alaz`.
Gerçek yükleme testi: `ALAZ_FEEDBACK_URL=http://localhost:3000 cargo test -p alaz sends_to_a_local_site -- --ignored`.

Rapor sayfalarını açmak için ayrıca bir Clerk geliştirme anahtarı gerekir. Anahtar yoksa yalnızca yükleme yolu kullanılır.

## Yayına alma (site sahibinin adımları)

1. Vercel'de bu depodan proje aç. **Root Directory:** `web/feedback`.
2. Projeye **Neon** (Marketplace) ve **özel (private) bir Blob deposu** bağla. `DATABASE_URL` ve Blob ayarı kendiliğinden gelir.
3. **Clerk**'i Marketplace'ten bağla. Clerk panelinde giriş yöntemi olarak yalnızca **e-posta doğrulama kodu** açık olsun.
4. Ortam değişkenleri: `IP_HASH_SECRET` (`openssl rand -hex 32`), `ADMIN_EMAIL`, `NEXT_PUBLIC_CLERK_SIGN_IN_URL=/sign-in`.
5. Alan adı: Vercel projesine `feedback.alazlab.com` ekle. Cloudflare'de verilen kaydı **DNS only** olarak aç.
6. Clerk'te üretim (production) örneği oluştur, istediği DNS kayıtlarını Cloudflare'e ekle, üretim anahtarlarını Vercel'e yaz.
7. `ADMIN_EMAIL` ile gir, **Davetliler** sayfasından testçileri ekle.
8. Yeni sürümü yayımlamadan önce Linux ve Windows derlemelerinden birer gerçek rapor gönder.

Vercel'in önizleme adresleri varsayılan olarak korumalıdır; oyun oraya yükleyemez. Oyun yalnızca yayındaki adresi kullanır.

## Sınırlar

| Ne | Sınır |
|---|---|
| Bir rapor | en çok 4 MB (ekran görüntüsü 2,5 · log 0,3 · kayıt 1) |
| Aynı ağ adresi | saatte 5, günde 20 rapor |
| Herkes birlikte | günde 100, 30 günde 500 rapor |
| Toplam dosya | 800 MB; dolunca elle rapor silinir |

Sayılar `src/lib/report.ts` içindedir. Sunucu ağ adresini saklamaz; yalnızca anahtarlı özetini iki gün tutar.

## Yükleme arayüzü

`POST /api/reports`, `multipart/form-data`: `report` (JSON alanları, düz metin), isteğe bağlı `screenshot`, `log`, `save`.
Alanlar ve kurallar: `src/lib/report.ts`.

| Yanıt | Anlamı |
|---|---|
| 201 | kaydedildi |
| 200 | bu kimlik zaten kayıtlı |
| 400 | geçersiz alan ya da dosya |
| 413 | çok büyük |
| 429 | çok sık; `Retry-After` süresi |
| 503 | depo ya da aylık hak dolu, ya da sunucu hazır değil |
