# Nudge Studio — Zengin Arayüz Yol Haritası (Envanter & Yedek)

> Amaç: Mevcut native renderer'ı (`winit 0.30` + `softbuffer 0.4` + `font8x8`)
> geliştirerek modern, "web uygulaması hissi" veren bir arayüze yaklaştırmak.
> Bu belge, eksik her maddeyi **kanıta dayalı** (kod üzerinden) listeler.
> Tarih: 21 Eki 2026 · Repo: NekomyaDev/nudge · HEAD: fff288a

---

## BÖLÜM 0 — MEVCUT DURUM (kanıt)

### Teknoloji yığını (koddan doğrulandı)
- Pencere/olay: `winit 0.30` (Wayland + X11) — `ui_window.rs:9-14`
- Sunum: `softbuffer 0.4` (yazılım rasterize, CPU) — `ui_window.rs:1,1283-1288`
- Font: `font8x8` 8x8 bitmap (sabit, MSB-sol) — `font8x8.rs`
- Çizim primitifleri: `draw_rect`, `draw_rect_outline`, `draw_char`, `draw_text`
- Framebuffer: `u32` (ARGB), `stride` tabanlı

### Renk paleti (hardcoded, kodda sabit)
| Kullanım | Değer | Satır |
|---|---|---|
| Arka plan (Zinc-900) | `0xFF18181B` | 587 |
| Metin (beyaz) | `0xFFF4F4F5` | 646 |
| Buton Normal (Blue-500) | `0xFF3B82F6` | 662 |
| Buton Hover (Blue-600) | `0xFF2563EB` | 660 |
| Buton Kenarlık (Blue-400) | `0xFF60A5FA` | 682 |
| Input Dolgu (Zinc-800) | `0xFF27272A` | 708 |
| Input Kenarlık Odaklı (Blue-500) | `0xFF3B82F6` | 695 |
| Input Kenarlık Odaksız (Zinc-600) | `0xFF52525B` | 697 |
| Seçim Vurgusu (Blue-700) | `0xFF1D4ED8` | 758 |
| İmleç (beyaz) | `0xFFFFFFFF` | 801 |
| Placeholder (Zinc-500) | `0xFF71717A` | 836 |
| Kutu metni | `0xFFE4E4E7` | 775 |

### Mevcut yetenekler (VAR)
- [x] Yatay/dikey yerleşim: `row`, `column` (padding/gap destekli)
- [x] Metin, buton, input (3 temel widget + 2 düzenleyici = 5 toplam)
- [x] Buton hover durumu (2 tonlu)
- [x] Input odak durumu, imleç, seçim vurgusu, placeholder
- [x] Metin sarma (`wrap_text`, satır sonu otomatik kırma)
- [x] Genel pencere kaydırması (`scroll_x/scroll_y`, MouseScrollDelta ile — 1347)
- [x] Viewport kırpma (out-of-bounds eleman çizilmiyor — 628)
- [x] Tıklama→id, change→id, key→id olay yönlendirme
- [x] Headless test altyapısı (`mount_headless`, dispatch_*)

### Renderer'ın SIRILDIĞI yerler (yumuşak noktalar)
- Yalnız `u32` ARGB buffer; alpha blending **yok** (her şey opak çiziliyor)
- Font tek boyutlu (8x8 × scale); anti-aliasing **yok** → kenarlar tırtıklı
- Renkler kodda `const` değil, satır aralarında literal → tema değişimi zor
- Ölçek yalnızca tam sayı (1,2,3...); kesirli font boyutu imkânsız

---

## BÖLÜM 1 — RENDERER GÖRSEL ALTYAPISI (temel katman)
> Bu katman olmadan hiçbir yeni widget düzgün görünmez. ÖNCE BUNLAR.
> Tahmini toplam: 8–12 gün

### 1.1 Alpha blending / yarı saydamlık  (1 gün)
- [ ] `draw_rect`'e `alpha: u8` parametresi; `dst = src*a + dst*(1-a)`
- [ ] Overlay, gölge ve modal arka plan karartması için şart
- Not: buffer formatı ARGB olduğu için kanal maskesiyle mümkün

### 1.2 Gerçek font rasterizasyonu (TTF/OTF)  (2–3 gün) ★ en büyük fark
- [ ] Font crate ekle: öneri `ab_glyph` (hafif) veya `fontdue` (hızlı)
- [ ] Font dosyasını gömülebilir binary'ye ekle (`include_bytes!`)
- [ ] Ölçeklenebilir boyut (8, 10, 12, 14, 16, 20, 24...)
- [ ] Anti-aliasing (grayscale coverage)
- [ ] UTF-8 / Türkçe karakter desteği (şu an 8x8'de `ğüşıöç` render yok → `?`)
- [ ] Ölçüm: `measure_text(width)` ile doğru sarma ve dikey hizalama
- [ ] Satır içi stil: **bold** (ikinci geçiş / synthetic bold)
- Uyarı: `draw_text` çağrılarının tümü (satır ~639, 688, 767) güncellenmeli

### 1.3 Yuvarlatılmış köşe (rounded rect)  (1 gün)
- [ ] `draw_rounded_rect`: yarıçap + kenarlık
- [ ] Buton, input, kart, modal için kullanılacak

### 1.4 Gölge (drop shadow)  (1 gün)
- [ ] Yumuşak gölge (blur yerine çok katmanlı alpha, blur'suz approximation)
- [ ] Kart ve modal için derinlik hissi

### 1.5 Gradyan  (0.5 gün)
- [ ] Dikey/yatay linear gradient (buton, header, seçili öğe)

### 1.6 İmleç & fare durumu  (1 gün)
- [ ] Metin imleci yanıp sönme animasyonu (zamanlayıcı gerekli)
- [ ] Fare imleç biçimi değiştirme (pointer/text/wait) — winit `set_cursor`
- [ ] Buton `active` (basılı) üçüncü tonu; disabled (soluk) durumu
- [ ] Tooltip (hover'da küçük etiket)

### 1.7 Zamanlayıcı / animasyon altyapısı  (1–2 gün)
- [ ] Redraw için frame clock / `request_redraw` zamanlaması
- [ ] Hover geçiş yumuşatma (renk interpolasyonu)
- [ ] Dönen gösterge (spinner) animasyonu için temel

### 1.8 Renk/tema sistemi  (1 gün)
- [ ] Renkleri `struct Theme` içine taşı (şu an hepsi literal — BÖLÜM 0 tablosu)
- [ ] Light/Dark tema anahtarı
- [ ] Widget'lara tema referansıyla erişim

### 1.9 Kaydırma çubuğu (scrollbar)  (1 gün)
- [ ] Dikey+yatay scrollbar çizimi (thumb + track)
- [ ] Sürükleme ile kaydırma, hover vurgusu
- [ ] Mevcut genel `scroll_y` ile entegrasyon

---

## BÖLÜM 2 — EKSİK BİLEŞENLER (widget)
> Her biri 3 katmanda çalışmalı: Nudge dil sözdizimi + compiler (AST/tip) + renderer.
> Tahmini: basit 0.5 gün, orta 1 gün, karmaşık 2 gün

### 2.1 `scroll` (iç içe kaydırma alanı)  (2 gün) ★ kritik
- [ ] Belirli bir bölgeyi kaydırılabilir kırmak (şu an yalnız genel pencere kaydırması var)
- [ ] Sohbet akışı için şart; sayfalama çözümünün yerini alır
- [ ] İçerik yüksekliğini ölçme + scroll pozisyonu + klavye okları

### 2.2 `select` / dropdown  (1 gün)
- [ ] Model seçimi, ayar menüleri için
- [ ] Açılır liste, seçili öğe, klavye gezinme

### 2.3 `checkbox` / `toggle`  (0.5 gün)
- [ ] Ayarlar anahtarları (on/off)

### 2.4 `slider`  (1 gün)
- [ ] Sıcaklık, budget gibi sayısal değer kaydırıcı

### 2.5 `progress` / `spinner`  (1 gün)
- [ ] Yanıt gelirken dönen gösterge; belirsiz (indeterminate) mod
- [ ] 1.7'deki zamanlayıcıya bağımlı

### 2.6 `image` / `icon`  (1 gün)
- [ ] Avatar, logo, ikon çizimi
- [ ] PNG decode (crate: `image` veya hafif `png`) + ölçekleme + alfa

### 2.7 `tabs`  (0.5 gün)
- [ ] Sohbet/sekmeler, ayar bölümleri

### 2.8 `code_block`  (2 gün)
- [ ] Kod renklendirmeli blok, satır numarası, yatlama (monospace)
- [ ] 1.2'deki font altyapısı monospace font ister

### 2.9 `divider` / `spacer`  (0.5 gün)
- [ ] Ayraç çizgisi, esnek boşluk

### 2.10 `badge` / `tag`  (0.5 gün)
- [ ] Rol etiketi (YOU/NUDGE), model adı rozeti

---

## BÖLÜM 3 — DÜZEN (LAYOUT) İYİLEŞTİRMELERİ
> Tahmini: 3–4 gün

### 3.1 Gerçek sohbet baloncukları (chat bubbles)  ★
- [ ] Kullanıcı/asistan için yuvarlatılmış, hizalı balonlar (1.3 + 1.1 ile)
- [ ] Balon içi kenar boşluğu, maksimum genişlik, sol/sağ hizalama
- [ ] Bu, "Chatgpt hissinin" en görünür parçası

### 3.2 Zengin metin satır içi (inline rich text)  (1–2 gün)
- [ ] Kalın, italik, kod (`code`), bağlantı vurgusu tek metin içinde
- [ ] 1.2'deki font altyapısı şart

### 3.3 Sistem mesajı / durum satırı stilleri
- [ ] Bilgi/uyarı/hata için renkli satır stilleri

### 3.4 Akıllı boşluk & hizalama  (0.5 gün)
- [ ] `row`/`column`'a `align`, `justify`, `expand` özellikleri
- [ ] Şimdilik sabit padding/gap — esnek hizalama yok

---

## BÖLÜM 4 — ETKİLEŞİM & AKILLI DAVRANIŞ
> Tahmini: 3–4 gün

### 4.1 Klavye kısayolları  (1 gün)
- [ ] Ctrl+K (komut paleti), Ctrl+B (sidebar), Ctrl+/ (yardım)
- [ ] Tüm pencereye küresel kısayol yöneticisi

### 4.2 Komut paleti (command palette)  (1–1.5 gün)
- [ ] Ctrl+K ile açılan arama listesi (yeni sohbet, ayarlar, model değiştir...)
- [ ] Fuzzy arama + yukarı/aşağı gezinme

### 4.3 Klavye ile liste gezinme  (0.5 gün)
- [ ] Sohbet listesinde yukarı/aşağı + Enter ile açma

### 4.4 Sürükle-bırak (drag & drop)  (1 gün)
- [ ] Mesaj düzenleme, sohbet sıralama, dosya bırakma

### 4.5 Bağlam menüsü (right-click)  (0.5 gün)
- [ ] Mesaj kopyala/yeniden üret, sohbet sil/yeniden adlandır

### 4.6 Kopyala / yapıştır zenginleştirme  (0.5 gün)
- [ ] Zengin metin yapıştırma, kod kopyalama bildirimi

---

## BÖLÜM 5 — MİMARİ/ALTYAPI (kalıcılık, hata, performans)
> Tahmini: 3–5 gün

### 5.1 Hata yakalama & gösterimi  ★ (2 gün)
- [ ] LLM çağrısı hatasını UI'da göster (CAPABILITIES.md: try/catch doğrulanmadı)
- [ ] Ağ/timeout hatası için yeniden dene butonu
- [ ]崩溃 kurtarma: taslakla yeniden açma

### 5.2 Asenkron LLM çağrısı & iptal  (2 gün) ★
- [ ] Senkron bekleme yerine arka iş parçacığı + canlı güncelleme
- [ ] Durdur (cancel) butonu; akış (streaming) yanıt
- [ ] Uyarı: UI redraw ile güvenli entegrasyon CAPABILITIES'te doğrulanmamış

### 5.3 Gerçek kaydırma (bağımsız sohbet)  → 2.1 `scroll` ile örtüşür

### 5.4 Pencere durumu kalıcılığı  (0.5 gün)
- [ ] Pencere boyutu/posisyon, tema, sidebar durumunu hatırlama

### 5.5 Performans: kısmi yeniden çizim  (1 gün)
- [ ] Şu an her olayda tüm framebuffer sıfırdan çiziliyor
- [ ] Kirlı bölge (dirty rect) takibi ile uzun sohbetlerde akıcılık

---

## BÖLÜM 6 — DOSYA/DIŞA AKTARIM (uzatma)
> Daha ileri hedefler; şimdilik opsiyonel

- [ ] 6.1 Sohbet dışa aktarım (Markdown/JSON/PNG)
- [ ] 6.2 Ayarlar ekranı için ayrı diyalog penceresi
- [ ] 6.3 Çoklu pencere (ayrı sohbet pencereleri)
- [ ] 6.4 Yerelleştirme (TR/EN) — font altyapısı 1.2 şart
- [ ] 6.5 Erişilebilirlik: okuyucu (screen reader) semantiği, kontrast oranı
- [ ] 6.6 Klavye odak halkaları (focus rings) — kısmen var (input)

---

## ÖZET SAYIMLAR

| Bölüm | Madde | Tahmini |
|---|---|---|
| 1 — Renderer görsel altyapısı | 9 büyük | 8–12 gün |
| 2 — Eksik bileşenler | 10 widget | 9–10 gün |
| 3 — Düzen iyileştirmeleri | 4 | 3–4 gün |
| 4 — Etkileşim | 6 | 3–4 gün |
| 5 — Mimari/altyapı | 5 | 3–5 gün |
| 6 — Uzatma (opsiyonel) | 6 | — |
| **TOPLAM (1–5)** | **~34 madde** | **~26–35 gün** |

### Sıralama önerisi (bağımlılıklara göre)
1. **1.8 Tema** (renkleri merkezileştir) — hızlı kazanç, her şeyi kolaylaştırır
2. **1.2 Gerçek font** + **1.3 Yuvarlatılmış köşe** + **1.1 Alpha** — görsel sıçrama
2. **2.1 `scroll`** — sohbet akışının önü açılır
4. **3.1 Baloncuklar** — "Chatgpt hissi" buradan gelir
5. **5.2 Asenkron + 5.1 Hata** — gerçek kullanım için şart
6. Kalan widget'lar (BÖLÜM 2) ve etkileşim (BÖLÜM 4)

### Kritik bağımlılık notları
- `code_block` (2.8) ve Türkçe/metin → **1.2 font** olmadan olmaz
- Baloncuklar (3.1) → **1.3 yuvarlak köşe + 1.1 alpha** olmadan düzgün görünmez
- Spinner (2.5) → **1.7 zamanlayıcı** olmadan animasyon yapamaz
- Tüm yeni widget'lar → compiler + renderer + dil sözdizimi üçlüsü ister

### Kapsam dışı (bilinçli)
- Web/HTML arayüzü (renderer'ı tamamen değiştirmek gerekir)
- Harici GUI kütüphanesi (egui/iced) — ayrı yol
- GPU hızlandırma (softbuffer yazılımsal kalır; istenirse wgpu ayrı proje)
