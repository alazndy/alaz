# assets/ — oyuna dosyadan varlık eklemek

Bu klasör **boş olabilir**: oyun her şeyi hâlâ kodla çiziyor. Buraya bir dosya bıraktığında, o dosyanın karşılığı olan
şey kodla çizilen yerine **dosyadan** gelir. Koda dokunmana gerek yok. Dosya yoksa kod çizer, yani her şey her zaman
çalışır.

Derleme sırasında bu klasör `build.rs` ile binary'nin içine gömülür; yayınlanan oyunun yanında ayrı dosya taşımak
gerekmez.

```
assets/
  models/   3D modeller: <ad>.vox, <ad>.glb, <ad>.toml (isteğe bağlı yan dosya)
  icons/    eşya ikonları: <eşya_id>.png
  ui/       arayüz resimleri: panel.png, button.png, logo.png ...
  tiles/    zemin dokuları: <zemin>_<n>.png
```

## Hızlı başlangıç

```sh
# 1. Oyunun kodla çizdiği HER şeyi dosya olarak dök (pencere açmaz):
alaz --export-assets /tmp/alaz-assets

# 2. İstediğini aç, değiştir (Blender / MagicaVoxel / piksel editörü), assets/ altına koy:
cp /tmp/alaz-assets/models/campfire.glb assets/models/
cp /tmp/alaz-assets/icons/stone.png assets/icons/

# 3. Oyunu diskten okutarak, dosya kaydedilince canlı yenilenerek çalıştır:
ALAZ_ASSETS=$PWD/assets ALAZ_HOTRELOAD=1 alaz --demo

# 4. Tek modele bak (dönen kamera, dinlenme/çalışma sırayla):
ALAZ_ASSETS=$PWD/assets ALAZ_HOTRELOAD=1 ALAZ_DEMO_VIEW=campfire alaz --demo
```

Geliştirme ortam değişkenleri:

| Değişken | Anlamı |
|---|---|
| `ALAZ_ASSETS=<klasör>` | varlıkları gömülü kopya yerine bu klasörden oku |
| `ALAZ_HOTRELOAD=1` | `ALAZ_ASSETS` verildiyse dosya kaydedilince yeniden yükle (debug derlemede varsayılan açık) |
| `ALAZ_NOASSETS=1` | hiçbir varlık dosyasını kullanma (kodla çizilen görünümle karşılaştırmak için) |
| `ALAZ_DEMO_VIEW=<ad>` | önizleme: yalnız o model (ya da yapı adı), kamera dönerek; eşya adı verirsen o ikon |

Yükleme hatası olursa (bozuk dosya vb.) terminale `[assets] ...` satırı basılır ve o şey için kod çizimi kullanılır.

---

## 3D modeller (`assets/models/`)

İki biçim: **MagicaVoxel `.vox`** ve **glTF 2.0 `.glb`**. Aynı ad için ikisi de varsa `.glb` kazanır.

### Adlar

Dosya adı = oyundaki şeyin adı (küçük harf, `_` ile):

* **Yapılar** (`BuildingKind`): `campfire`, `shelter`, `charcoal_kiln`, `furnace`, `depot`, `workshop`, `workbench`,
  `water_wheel`, `field`, `loom`, `pen`, `pit_kiln`, `anvil`, `hand_mill`, `animal_gin`, `bloomery`, `bellows`,
  `trip_hammer`, `iron_anvil`, `blast_furnace`, `steel_anvil`, `windmill`, `conveyor`, `steam_engine`, `pump`,
  `rolling_mill`, `dynamo`, `pole`, `electric_motor`, `electric_lamp`, `computer`, `ruin`, `cellar`, `mine_shaft`.
  * **Sanayi makineleri** (1. dalga): `coke_oven`, `lime_kiln`, `cement_kiln`, `brick_kiln`, `puddling_furnace`,
    `bessemer`, `cupola`, `molding_bench`, `lathe`, `drill_mill`, `press`, `sawmill`, `spinning_wheel`, `power_loom`,
    `boiler`, `steam_hammer`, `rolling_line`. Bunların modelleri `crates/render/src/mesh/machines.rs`'te kodla yazılır
    ama diğer yapılarla aynı kancadan geçer: `models/<ad>.vox|.glb` varsa o kullanılır. Dönen ve inip kalkan parçalar
    (tornanın aynası, presin kolu, çekicin başı, haddelerin silindirleri, çarklar) dışa aktarmada `spin_` / `spinx_` /
    `spinz_` / `piston_` adlı düğümler olarak yazılır (hız ve genlik `extras` içinde), yani dışa aktarılan dosya
    aynı kuralla oynar; kendi modelinde de aynı adları ver. Çalışırken parlayan ağızlar `lit_extra` içindedir.
  * **Yaşam ve zanaat makineleri** (2. dalga): `potter_wheel`, `oil_press`, `tannery`, `drying_rack`, `smokehouse`, `well`,
    `stone_mortar`, `water_mill`, `forge_hammer`, `felt_mallet`, `ropewalk`, `crane`, `glass_furnace`, `paper_vat`,
    `printing_press`, `cotton_gin`, `spinning_jenny`. Bunlar da `crates/render/src/mesh/machines.rs`'te kodla yazılır ve
    aynı kancadan geçer. Hareketli parçalar dışa aktarmada şu düğümler olur: `spin_kickwheel`, `spin_head` (çömlekçi
    çarkı), `spin_lever` + `piston_platen` (yağ presi), `piston_paddle` (tabakhane), `spinx_windlass` + `piston_bucket`
    (kuyu), `piston_pestle` (dibek), `spinz_wheel` (su değirmeni), `piston_head` + `spinx_cam` + `spinx_flywheel`
    (dövme çekici), `piston_mallet` (keçe tokmağı), `spinx_hookwheel` (halat bükme), `spinz_pulley` + `piston_load` +
    `spinz_windlass` (vinç), `piston_mould` (kâğıt havuzu), `spin_bar` + `piston_platen` (matbaa), `spinz_roller_a/b` +
    `spinx_pulley` (çırçır), `spin_spindle_0..5` + `spinz_drive` (iplik eğirme makinesi). Kurutma rafı çalışırken
    şeritlerin renk değiştirmesiyle, tüttürme ocağı dumanla, cam ocağı ağız parıltısıyla gösterir (`lit_extra`/`idle_extra`).
  * **Elektrik ve ileri dönem makineleri** (3. dalga): `arc_furnace`, `refinery`, `still`, `electrolysis`, `compressor`, `railway`,
    `steam_pump`, `transformer`, `accumulator`, `welder`, `reactor`, `cooler`, `telegraph`, `radio`, `vacuum_pump`,
    `crystal_puller`, `photolith`, `cnc_machine`, `assembly_robot`. Bunlar da `crates/render/src/mesh/machines.rs`'te kodla yazılır
    ve aynı kancadan geçer. Hareketli parçalar dışa aktarmada şu düğümler olur: `piston_electrode_0..2` (ark fırınının elektrotları),
    `spinz_flywheel` (rafineri, kompresör, buharlı pompa), `piston_head` (kompresör, CNC), `spinx_drum` + `spinz_wheel_*`
    (ray ve vagonun makarası ve tekerlekleri), `piston_rod` (buharlı pompa), `spin_stirrer` (reaktör), `spin_fan` (soğutucu),
    `piston_key` (telgraf), `spin_dish` (radyo), `piston_piston` (vakum pompası), `spin_rod` (kristal çekme), `spin_wafer`
    (fotolitografi), `spin_chuck` (CNC), `spin_turret` (montaj robotu kolu). Elektriksiz (şebeke ölü) makine dinlenme hâliyle
    kalır: çalışan parça (`spin_`/`piston_`/`lit_`) yalnız şebeke canlıyken oynar.
* **Doğa** (`NodeKind`): `stone_outcrop`, `iron_outcrop`, `copper_outcrop`, `tin_outcrop`, `berry_bush`,
  `clay_deposit`, `herb_patch`, `cave_mouth`, `fiber_plant`, `deadwood`, `tree`.
  * Ağaçlar için tür adları da var, bulunursa `tree`'den önce onlar kullanılır:
    `tree_oak`, `tree_birch`, `tree_pine`, `tree_acacia`, `tree_jungle`, `tree_autumn`, `tree_willow`, `tree_cactus`.
  * Maden damarı henüz tanınmamışken (`iron_outcrop` vb.) `<ad>_unknown` aranır, yoksa `<ad>` kullanılır.
* **Zemin süsü** (`decor`, karo başına küçük şeyler): `grass`, `flower`, `fern`, `mushroom`, `reeds`, `pebbles`,
  `dry_grass`, `rubble`.
* **Elde tutulanlar** (isteğe bağlı override): `tool_stone_axe`, `tool_metal_axe`, `tool_metal_pick`, `tool_spear`;
  **giysiler**: `outfit_wrap`, `outfit_cloak`, `outfit_boots`, `outfit_tunic`, `outfit_hood`, `outfit_plate`,
  `outfit_helm`, `outfit_apron`, `outfit_felt_cloak` (keçe pelerin), `outfit_leather_armor` (deri zırh). Giysi dosyası varsa o
  giysinin kodla çizilen hâli çizilmez.
* **Çeşitlilik**: `<ad>.vox`, `<ad>_1.vox`, `<ad>_2.vox` ... Birden çok dosya varsa konuma göre (seed ile, her
  seferinde aynı) biri seçilir. `.glb` ile `.vox` karışık olabilir.

### Eksenler, ölçek, pivot

Oyun: **+x doğu, +y yukarı, -z kuzey (+z güney)**, birim **metre**. Kamera güneyden baktığı için modelin "önü" **+z**
tarafıdır (ekrana bakan yüz). Bir insan ~1.8 m boyunda.

* **.vox** (MagicaVoxel: z yukarı, ön yüz −y tarafı): oyunun x = vox x, y = vox z, z = −vox y.
  * Ölçek: **1 voxel = 1/16 m** (6,25 cm; eski sprite çözünürlüğüyle uyumlu). `<ad>.toml` içinde `scale = 0.0625`
    ile değiştirilir.
  * Pivot: modelin **alt orta noktası** (yerdeki ağırlık merkezi); yani model yere oturur, ortalanır.
  * Gizli yüzler atılır, aynı renkli komşu yüzler tek dikdörtgene birleştirilir (greedy meshing).
  * Renk = paletteki renk **olduğu gibi** (bayt / 255). Kendinden ışıyan voxel için MagicaVoxel'de malzemeyi
    **Emit** yap; ya da `.toml`'da `glow = ["#ff7a1a"]` ile renge göre belirt.
  * Sahne grafı düzleştirilir; **adı olan dönüşüm düğümleri** (`spin_tekerlek` gibi, aşağıya bak) ayrı parça olur.
* **.glb** (glTF 2.0 ikili): y yukarı, birim metre, **origin sende** (ayakların bastığı noktaya koy). `scale` bir
  çarpandır (varsayılan 1).
  * Yalnız üçgen mesh. Düğüm hiyerarşisi düzleştirilir (dönüşümler uygulanır).
  * Renk: vertex rengi (`COLOR_0`) × malzemenin `baseColorFactor` × (varsa gömülü **PNG** doku, köşelerde örneklenir
    ve vertex rengine pişirilir). JPEG doku desteklenmez. Malzemenin `emissiveFactor`'ı sıfırdan büyükse parça
    kendinden ışır. Değerler olduğu gibi kullanılır (sRGB bayt/255 gibi), oyunun kodla çizdiği renklerle aynı uzay.
  * Işık seviyesi yüzün yönünden hesaplanır (oyunun kutularındaki gibi: üst 1.0, güney 0.7, doğu 0.64, kuzey 0.5,
    batı 0.48, alt 0.3). Oyunun dışa aktardığı dosyalarda bu, köşe başına `_LIGHT`, `_GLOW`, `_MAT` ek nitelikleriyle
    birebir saklanır (başka programlar bunları yok sayar).

### Hareketli ve koşullu parçalar (adlandırma kuralı)

Bir **düğümün adı** şu önekle başlıyorsa o düğüm (ve çocukları) özel bir parçadır. glb'de düğüm adı, vox'ta
MagicaVoxel sahnesindeki dönüşüm düğümünün adıdır (`_name`).

| Önek | Ne yapar |
|---|---|
| `spin_`, `spiny_` | **makine çalışırken** düğümün orijini (vox'ta parçanın ortası) etrafında y ekseninde döner |
| `spinx_`, `spinz_` | aynısı x / z ekseninde (su çarkı gibi yatay eksenli şeyler) |
| `piston_` | çalışırken kendi y'si boyunca aşağı yukarı gider (`\|sin\|`, 0.15 m) |
| `lit_` | **yalnız çalışırken** çizilir (ateş, kor, parlayan ağız) |
| `idle_` | **yalnız dururken** çizilir |
| `foliage_` | parça yaprak malzemesidir (rüzgârda sallanır) |

glb düğümüne `extras` ile ayar verilebilir: `{"speed": 0.05}` (spin için radyan/tick, piston için döngü/tick),
`{"amp": 0.3}` (piston için metre). Vox'ta varsayılanlar kullanılır.

"Çalışıyor" = oyundaki yapının çalışma durumu (kodla çizilen modellerdeki `active` ile aynı kanca). Campfire için
"çalışıyor" = yakıt var.

### `<ad>.toml` (isteğe bağlı yan dosya)

Aynı klasörde, aynı adla (`tree_pine.toml`; `tree_pine_1.vox` için önce `tree_pine_1.toml`, yoksa `tree_pine.toml`).

```toml
scale = 0.0625          # vox: voxel başına metre; glb: çarpan
foliage = true          # tüm model yaprak malzemesi (rüzgârda sallanır)
rotate = false          # doğa nesneleri rastgele döndürülür; kapatmak için false
fit_height = true       # yalnız ağaç: modeli dünyadaki ağacın boyuna ölçekle (ağaçlar için varsayılan true)
glow = ["#ff7a1a"]      # bu renkler kendinden ışır
leaf = ["#3a7a2a"]      # bu renkler yaprak malzemesi
offset = [0.0, 0.0, 0.0]  # metre, modeli kaydır
```

### Elde tutulan alet ve giysi

* `tool_*`: **kavrama noktası origin**, sap **+y** yönünde başa doğru gider, bıçağın yüzü **+z**. Oyun modeli, elin
  olduğu yere ve sapın o anki yönüne göre yerleştirir.
* `outfit_*`: kişinin **ayaklarına** konur, yüzü gittiği yöne bakar (ön +z), gövdeyle birlikte dönmez bükülmez; her
  kişi için seçilen çeşide göre `outfit_cloak_1` ... çeşitlenebilir. Uzak (LOD) insanlarda kullanılmaz.

---

## İkonlar (`assets/icons/<eşya_id>.png`)

`<eşya_id>` eşyanın snake_case adı: `stone`, `sharp_stone`, `stone_axe`, `iron_ore` ... Tam liste için
`alaz --export-assets` çıktısındaki `icons/` klasörüne bak (her eşyaya bir dosya). Sanayi eşyaları da dahil:
`tar`, `lime`, `mortar`, `cement`, `concrete`, `brick`, `pipe`, `iron_cylinder`, `iron_wheel`, `sand_mold`, `shaft`,
`bolt_nut`, `piston`, `precision_gear`, `cylinder_block`, `sheet`, `sheet_part`, `thread`, `heavy_shaft`, `rail`, `beam`.
Yaşam ve zanaat eşyaları da: `plant_oil`, `linseed`, `leather`, `belt`, `leather_armor`, `rope`, `felt`, `felt_cloak`, `cotton`,
`cotton_fiber`, `paper`, `book`, `bottle`, `lens`, `window_pane`, `oil_lamp`, `oil_flask`, `dried_berries`, `jerky`,
`smoked_meat`, `smoked_fish`, `bulgur`.
Elektrik ve ileri dönem eşyaları da: `bauxite`, `aluminium`, `pure_copper`, `kerosene`, `lubricant`, `alcohol`, `fertilizer`, `dye`,
`soap`, `welded_frame`, `vacuum_tube`, `pure_silicon`, `chip`.

* Biçim: PNG, RGBA (şeffaflık olur). Boyut serbest ama **kare**; 32×32 ya da 64×64 öneriyoruz.
* Çizim: **nearest** (piksel piksel, yumuşatma yok), yuvaya sığacak kare boyutuna ölçeklenir.
* Dosya yoksa mevcut kodla çizilen ikon kullanılır. Bilinmeyen (henüz görülmemiş) eşyalar yine "?" gösterir.

## Arayüz (`assets/ui/`)

Hepsi isteğe bağlı. Çağ numarası 0–5 (`ERA`): önce `<ad>_<çağ>.png`, yoksa `<ad>.png`, o da yoksa kodla çizilen.

| Dosya | Nerede |
|---|---|
| `panel.png`, `panel_<çağ>.png` | tüm panellerin çerçevesi (9 parçalı: kenar = kısa kenarın üçte biri) |
| `button.png`, `button_hover.png`, `button_down.png`, `button_off.png` | düğmeler (9 parçalı); yalnız `button.png` varsa diğer durumlar renk tonuyla türetilir |
| `logo.png` | ana menü başlığı (animasyonlu harflerin yerine) |

9 parçalı resimde köşeler boyutunu korur (resim pikseli = arayüz pikseli), kenarlar ve orta gerilir. Örnek: 24×24 bir
resimde 8 piksellik köşeler.

## Zemin dokuları (`assets/tiles/<zemin>_<n>.png`)

`<zemin>` = `grass`, `forest`, `sand`, `rock`, `water`; `<n>` = 1…4 (her zeminin dört çeşidi var, konuma göre karışır).

* Biçim: PNG, kare. Oyunun zemin dokusu **metre başına 27 texel** (düşük piksel inceliğinde 21); yani 1 karo = 27×27
  (ya da 21×21). Başka boyutta kare verirsen en yakın komşu ile ölçeklenir; en temizi 27×27 (`--export-assets` o
  boyutta döker).
* Opak olmalı (alfa yok sayılır). Zeminler arası kenar geçişi (karışma) senin dokundan türetilir.
* `<zemin>_1.png` kenar geçişlerinin de tabanıdır.

---

## Dışa aktarma ve geri yükleme (round-trip)

`alaz --export-assets <klasör>`:

* `models/<ad>.glb` — her yapının ve doğa nesnesinin kodla çizilen hâli (vertex renkli). Yapılar için: hep görünen
  üçgenler `model` düğümünde, **yalnız çalışırken** olanlar `lit_extra`, **yalnız dururken** olanlar `idle_extra`
  düğümünde. (Ağaçlar tür başına `tree_pine.glb` vb.; maden damarları tanınmamış hâliyle `_unknown.glb`.)
* `icons/<id>.png` (64×64) ve `icons_32/<id>.png` (32×32) — her eşya ikonu.
* `tiles/<zemin>_<n>.png` — zemin dokuları.

Makinelerin hareketli parçaları ayrı düğümlerdir: `spin_<ad>` (y ekseni), `spinx_<ad>`, `spinz_<ad>`, `piston_<ad>`;
düğümün orijini (translation) dönme noktasıdır. Örnek: `lathe.glb` içinde `spinx_chuck` (aynanın dönmesi) ve
`spinz_pulley`, `press.glb` içinde `piston_ram`. Bunları Blender'da yeniden adlandırmadan kullanabilirsin.

Çıktıyı `assets/` altına olduğu gibi bırakırsan oyun **aynı görüntüyü** verir (testle doğrulanır: dışa aktar → yükle →
aynı üçgenler). Sonra istediğin dosyayı değiştir.

Dışa aktarılmayanlar: zemin süsleri (`decor`: çim, çiçek...), insan ve hayvan gövdeleri (kodda kalır), animasyonlu
alevler (kodla çizilen ateşin dilleri anlık hâliyle `lit_extra` içinde statik gelir).

## Hot reload

`ALAZ_ASSETS=<klasör> ALAZ_HOTRELOAD=1` ile bir dosyayı kaydettiğinde (çeyrek saniyede bir kontrol edilir) model
meşhleri, ikonlar, arayüz resimleri ve zemin dokuları yeniden yüklenir; sahnedeki tüm parçalar yeniden kurulur
(sahne kısa bir an takılabilir). Önizleme için `ALAZ_DEMO_VIEW=<ad>` ile birlikte kullan.
