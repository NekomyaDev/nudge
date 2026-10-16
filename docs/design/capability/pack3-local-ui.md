# Nudge Language Capability Request: Pack 3 (Local UI)

**Status:** Implemented (see git log)
**Implementation Commits:** `565f6df` (PACK 3 slice 1: typed UI capability with headless view-tree runtime), `6363e3d` (PACK 3 slice 2: real native OS window backend winit + softbuffer, opt-in)
**Author:** Pack 3 Coordinator
**Target Repository:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` at commit `e2cafc0`
**Classification:** MEVCUT / KISMI / ÖNERİ

---

## 1. Durum Tasnifi (MEVCUT / KISMI / ÖNERİ)

### MEVCUT (Existing)
1. **Beş Sabit Efektli Tip Sistemi:** `crates/nudgec/src/check.rs:214` (commit `3b092ce`) içinde `const KNOWN_EFFECTS: [&str; 5] = ["LLM", "Tool", "IO", "Decision", "Computer"];`. Bildirilmemiş efektler `E0301`, bilinmeyen efektler `E0302` koduyla reddedilir (`crates/nudgec/src/hints.rs:15-16`, commit `3b092ce`).
2. **Computer-Use Dış Ekran Otomasyonu:** `crates/nudgec/src/parser.rs:1113-1129` (commit `3b092ce`), `crates/nudge-runtime/src/stdlib/computer.rs:24-40, 847-897` (commit `a5f20ad`), `tools/cu_bridge.py:1-47` (commit `85f68d8`). 15 metot (`observe`, `click`, `type`, `key`, `scroll`, `set_value`, `drag`, `perform`, `paste`, `screen_size`, `window_info`, `mouse_move`, `mouse_down`, `mouse_up`, `cursor_pos`) harici işletim sistemi pencerelerini erişilebilirlik ağacından tarar ve girdi enjekte eder.
3. **Trace-View Statik HTML Görüntüleyici:** `crates/nudgec/src/traceview.rs:1-35, 67-90` (commit `f452559`, `ca801f0`), `crates/nudgec/src/traceview.html`. Çalışma zamanı sonrasında NTF v1 izlerini tarayıcıda incelemek için gömülü bir TCP soket sunucusudur.
4. **Bytecode VM Varsayılan Yürütme ve Efekt Delegasyonu:** `crates/nudgec/src/eval/interpreter.rs:1733-1756` (commit `e3df881`, `3a66a56`, `cc4ee9f`). Varsayılan olarak Bytecode VM yürütülür (`NUDGE_VM=0` ile kapatılabilir). Efektli fonksiyonlar ve ifadeler `Op::DelegateCall` ve `Op::DelegateExpr` üzerinden ağaç yorumlayıcısına (`Interpreter`) delege edilir (`crates/nudgec/src/vm.rs:14-19, 566-575`, commit `d48fcac`, `cc4ee9f`).

### KISMI (Partial)
1. **Agent Durum Kalıcılığı:** `crates/nudgec/src/ast.rs:258-262` (commit `f452559`), `crates/nudgec/src/parser.rs:308` (commit `f452559`). `agent` yapıları `state { field: Ty = default }` alanları ve `state.x = v` atamaları ile reaktif durum yönetimine uygun bir temel sağlar. Ancak bu durum salt uçbirim/arka plan döngüleri içindir; render döngüsü veya arayüz olay bağlayıcısı bulunmaz.
2. **Bytecode Efekt Ayrımı:** `crates/nudgec/src/bytecode.rs:603, 622, 747-768` (commit `d48fcac`, `cc4ee9f`, `f8a56b9`). Bytecode derleyicisi `Tool` ve `LLM` ifadelerini yorumlayıcıya delege edebilirken, `ComputerCall` doğrudan derleme hatası verir (`Err("not in bytecode subset: computer call")`). Bu durum bütün programın tree-walking interpreter fallback yoluna düşmesine neden olur.
3. **Yerleşik Kayıt Tipleri:** `crates/nudgec/src/check.rs:220-284` (commit `3b092ce`). `Observation`, `Element`, `ActionResult`, `WindowInfo` tipleri tanımlıdır; tip denetleyicide yerleşik yapıların nasıl çözüleceğinin kanıtıdır ancak görsel arayüz eleman tipleri yoktur.

### ÖNERİ (Proposed)
1. **Birinci Sınıf `UI` Efekti:** `KNOWN_EFFECTS` içine altıncı efekt olarak eklenir (`uses UI`).
2. **`.ndg` Kaynak Sözdizimi:** `view`, `window` ve olay işleyici (`handle on_click`, `handle on_change`) bildirimleri.
3. **Yerel Pencere Çalışma Zamanı (Native Window Runtime):** `nudge-runtime` içinde çalışan, harici tarayıcı veya yabancı dil kabukları (Electron, Python vb.) içermeyen hafif, yerel pencere altyapısı.
4. **Çift Modlu Çalışma:** Hem interaktif masaüstü pencereleme hem de başsız (headless) deterministik test yürütümü.

---

## 2. Capability & Why It Blocks Local Apps

### Yetenek Tanımı
Nudge dilinde doğrudan tanımlanan, tip güvenli, durum reaktif ve yerel işletim sistemi üzerinde pencere açıp kullanıcı girdilerini (düğme tıklaması, metin girişi, seçim vb.) işleyen yerel arayüz (Local UI) mimarisidir.

### Neden Yerel Uygulama Geliştirmeyi Engelliyor?
Nudge halihazırda salt uçbirim veya headless ajan mantığı sunmaktadır:
- Bir geliştirici insan denetimli ajan (human-in-the-loop review station), onay arayüzü, prompt deneme paneli veya masaüstü etkileşim aracı geliştirmek istediğinde, Nudge dilinde pencere veya görsel bileşen tanımlama olanağı bulunmamaktadır.
- Dış dillerde (Electron, Python/Tkinter, Tauri) bir arayüz kabuğu yazılması zorunlu kalmakta, bu durum Nudge'ın bağımsız ikili dağıtım (`crates/nudgec/src/standalone.rs`) ve tek dilde güvenli ajan vizyonunu zedelemektedir.

---

## 3. Current State with File:Line + Commit Evidence

### 3.1. `computer.*` Neden Cevap Değildir?
- **Kod Kanıtı:** `crates/nudge-runtime/src/stdlib/computer.rs:24-40` ve `847-897` (commit `a5f20ad`), `crates/nudgec/src/parser.rs:1113-1129` (commit `3b092ce`), `tools/cu_bridge.py:1-47` (commit `85f68d8`), `docs/computer-use.md:5-29` (commit `cf66e97`).
- **Analiz:** `computer.*` bir kullanıcı arayüzü oluşturma kütüphanesi değil, yapay zeka ajanlarının işletim sistemindeki yabancı pencereleri fare ve klavye taklidiyle kontrol etmesi için tasarlanmış bir ekran otomasyonu (screen control / accessibility scraper) altyapısıdır. `computer.observe` işletim sisteminin AT-SPI ağacını okur, `computer.click` koordinat veya indeks üzerine sanal tıklama gönderir. Bu sistemde Nudge programı arayüz çizemez, pencere sunamaz veya kendi içine olay yakalayamaz; yalnızca başka uygulamaları simüle edilmiş girdiyle manipüle eder.

### 3.2. `trace-view` Neden Cevap Değildir?
- **Kod Kanıtı:** `crates/nudgec/src/traceview.rs:1-35, 67-90` (commit `f452559`, `ca801f0`), `crates/nudgec/src/traceview.html`.
- **Analiz:** `trace-view`, NTF v1 formatındaki iz günlüklerini (`llm.call`, `tool.call`, `fn.return`) incelemek için yerel `TcpListener` üzerinde çalışan ve `xdg-open` / `open` ile varsayılan web tarayıcısını açan statik bir HTML raporlayıcıdır. Çalışma zamanında Nudge programının akışına veya durumuna müdahale eden etkileşimli bir GUI değildir, bir geliştirici hata ayıklama aracıdır.

### 3.3. Efekt ve AST İncelemesi
- **Kod Kanıtı:** `crates/nudgec/src/check.rs:214` (commit `3b092ce`) satırında bilinen efektler `["LLM", "Tool", "IO", "Decision", "Computer"]` ile sınırlıdır.
- `crates/nudgec/src/ast.rs:233-265` (commit `f452559`) satırlarında en üst seviye öğeler yalnızca `TypeAlias`, `Fn`, `Tool`, `Test`, `Agent` olarak tanımlıdır; pencere veya görünüm öğesi yer almaz.

---

## 4. Proposed .ndg Source Syntax

Dil seviyesinde iki yeni en üst seviye bildirim (`view` ve `window`) ile `agent` bloklarıyla tam uyumlu reaktif olay bağlama sözdizimi:

### Sözdizimi Kuralları
1. **`view Name(params...) -> View`:** Saf (pure) veya durum bağımlı hiyerarşik bileşen ağacı.
2. **`window Name { ... }`:** Başlık, boyut, kök görünüm ve olay yönlendirmesini tutan pencere bildirimi.
3. **`handle <event>(args...) { ... }`:** Görünüm elemanından tetiklenen olay fonksiyonu.

### Somut `.ndg` Örneği:
```nudge
// review_console.ndg: İnsan onaylı ajan denetim konsolu
type ReviewState = {
    prompt: string,
    approved: bool,
    notes: string
}

view ReviewCard(state: ReviewState) -> View {
    column(padding = 16, gap = 12) [
        text("Ajan İstek İnceleme İstasyonu", size = 18, bold = true),
        text("İstek: " + state.prompt),
        input(value = state.notes, placeholder = "Denetçi notu...", on_change = set_notes),
        row(gap = 8) [
            button("Onayla", on_click = approve_action),
            button("Reddet", on_click = reject_action)
        ],
        text(if state.approved { "Durum: ONAYLANDI" } else { "Durum: BEKLİYOR" })
    ]
}

agent ReviewStation {
    state {
        prompt: string = "rm -rf /tmp/cache dizinini sil",
        notes: string = "",
        approved: bool = false
    }

    fn set_notes(new_text: string) uses UI {
        state.notes = new_text
    }

    fn approve_action() uses UI, IO {
        state.approved = true
        io.println("İşlem onaylandı: " + state.notes)
    }

    fn reject_action() uses UI {
        state.approved = false
    }

    window MainWindow {
        title: "Nudge Denetim İstasyonu",
        width: 500,
        height: 350,
        content: ReviewCard(state)
    }
}

fn main() -> bool uses UI, IO {
    ReviewStation.MainWindow.open()
}
```

---

## 5. Type & Effect Semantics

### 5.1. Yeni Efekt: `UI`
- `crates/nudgec/src/check.rs:214` içindeki `KNOWN_EFFECTS` dizisine `"UI"` eklenir:
  `["LLM", "Tool", "IO", "Decision", "Computer", "UI"]`.
- Bir pencere açan, görünüm durumunu güncelleyen veya olay döngüsünü başlatan tüm fonksiyonlar `uses UI` bildirmek zorundadır.
- Saf görünüm oluşturma fonksiyonları (`view Name(...) -> View`) yan etkisizdir ve `pure` kalabilir; sadece olay işleyiciler ve pencere komutları `UI` efekti gerektirir.

### 5.2. Tipler
- `View`: Opak yerel arayüz bileşen düğümü.
- `Event`: Temel olay kaydı (`{ target_id: string, timestamp: int }`).
- `ClickEvent`: `{ target_id: string }`.
- `InputEvent`: `{ target_id: string, text: string }`.

### 5.3. Hata Kodları (Error Codes)
Nudge E-kod hiyerarşisine (`crates/nudgec/src/hints.rs`) tam uyumlu yeni kodlar:
- **`E1001`:** `window` veya `view` olay işleyicisinde bildirilmemiş efekt kullanımı (`uses UI` eksik).
- **`E1002`:** Tanımsız arayüz bileşeni veya geçersiz bileşen hiyerarşisi.
- **`E1003`:** Olay işleyici imza uyumsuzluğu (`on_click` metodu argümansız veya `ClickEvent` beklerken uyumsuz parametre verilmesi).
- **`E1004`:** Aynı isimde birden fazla `window` veya `view` tanımlanması.

---

## 6. Local Execution Behavior (Native Window)

### Yerel Pencere Yürütme Motoru
- **Harici Kabuk Yok:** Electron, Node.js veya Python arayüz paketleri kullanılmaz.
- **Doğrudan `nudge-runtime` Entegrasyonu:** `nudge-runtime` içine minimal bir yerel pencere ve 2D raster arayüz arka ucu (ör. `winit` + `softbuffer` veya platform native framebuffer) eklenir.
- **Olay Döngüsü (Event Loop):**
  1. `window.open()` çağrısı yerel işletim sistemi pencere olay döngüsünü başlatır.
  2. Kullanıcı olayları (tıklama, klavye girişi) Nudge değerlerine (`NudgeVal`) dönüştürülüp ilgili `handle` fonksiyonuna gönderilir.
  3. Durum değişikliğinde (`state.x = v`) görünüm ağacı yeniden hesaplanır (diff/re-render) ve tampon güncellenir.
- **NTF v1 İz Uyumu:** Her kullanıcı olayı (`ui.event`) ve render çıktısı (`ui.render`), `nudge_runtime::trace::emit` (`crates/nudgec/src/eval/interpreter.rs:1821`) ile deterministik olarak kaydedilir; böylece UI içeren bir akış da `NUDGE_REPLAY` altında birebir tekrar oynatılabilir.

---

## 7. Security Boundary

1. **Ayrı Yetkilendirme (`NUDGE_UI_ALLOW`):** Tıpkı araç yetkilendirmesi `NUDGE_TOOL_GRANTS` (`crates/nudge-runtime/src/grants.rs:8-37`, commit `ea7f29b`) ve `computer.allow` kapsamı gibi, arayüz açma izni ortam değişkeni üzerinden kısıtlanabilir (`NUDGE_UI_ALLOW=1`).
2. **Başsız CI/Sandbox Güvenliği:** Sunucu ortamlarında veya CI testlerinde yanlışlıkla grafiksel pencere açılmasını engellemek için `NUDGE_UI_HEADLESS=1` modu bulunur; bu modda pencere açılmaz, arayüz bellek içi sanal ağaçta çalışır.
3. **Efekt Yalıtımı:** `UI` efekti tek başına dosya sistemine (`IO`), dış servislere (`Tool`) veya LLM modellerine (`LLM`) erişim hakkı vermez. Bir düğme tıklandığında araç çağrılacaksa o fonksiyon hem `UI` hem `Tool` bildirmek zorundadır.

---

## 8. Minimal Acceptance Test

Harici araçlara bağımlı olmadan, yalnızca `.ndg` test sözdizimi ile doğrulanabilir minimal kabul testi:

```nudge
view CounterView(count: int) -> View {
    column [
        text("Sayı: " + count),
        button("Artır", id = "btn_inc", on_click = inc)
    ]
}

agent CounterApp {
    state {
        val: int = 0
    }

    fn inc() uses UI {
        state.val = state.val + 1
    }

    window Win {
        title: "Test Penceresi",
        content: CounterView(state.val)
    }
}

test "pencere acilisi, girdi tetiklemesi ve gorunum guncellemesi" {
    // 1. Pencere baslatma (Headless sanal agac modunda)
    let app = CounterApp.Win.mount_headless()
    assert app.find("btn_inc").exists()
    assert app.find_text("Sayı: 0").exists()

    // 2. Sentetik olay tetikleme
    app.dispatch_click("btn_inc")

    // 3. Reaktif guncelleme dogrulamasi
    assert app.find_text("Sayı: 1").exists()
    assert CounterApp.state.val == 1
}
```

---

## 9. VM-Path Note

### Kaynak Analizi ve Doğrulanan Davranış
- `crates/nudgec/src/eval/interpreter.rs:1734-1755` (commit `3a66a56`) incelendiğinde, `NUDGE_VM=0` ayarlanmadığı sürece tüm programlar varsayılan olarak Bytecode VM üzerinden çalıştırılır.
- Fonksiyon düzeyinde efekt delegasyonu `cc4ee9f`, ifade düzeyinde delegasyon ise `d48fcac` (`crates/nudgec/src/vm.rs:14-19`) ile getirilmiştir. VM içinde çalışırken efektli bir fonksiyon çağrısı `Op::DelegateCall(name, argc)`, efektli bir ifade ise `Op::DelegateExpr(idx)` ile yakalanarak ağaç yorumlayıcısına (`interp.invoke_fn` / `interp.eval_expr`) aktarılır.
- **Kaynak Tarafından Doğrulanan:** Saf `view` fonksiyonları (saf kayıt ve liste üreten ifadeler) Bytecode VM'in mevcut alt kümesinde derlenip tam hızda yürütülebilir.
- **Mevcut Kısıt:** `crates/nudgec/src/bytecode.rs:622, 768` (commit `f8a56b9`) satırlarında görüldüğü üzere `ComputerCall` doğrudan derleyici tarafından reddedilmektedir (`Err("not in bytecode subset: computer call")`). Dolayısıyla benzer bir hataya düşmemek için, UI pencere açılışı ve olay döngüsü `Op::DelegateCall` üzerinden doğrudan çalışma zamanı yorumlayıcısına delege edilmelidir.
- **Kaynaktan Doğrulanamayan Husus:** VM çağrı yığını derinliği sınırı (`MAX_CALL_FRAMES`, commit `3e10c49`) mevcuttur; uzun ömürlü bir kullanıcı arayüzü olay döngüsünün VM yürütme döngüsü (`run_loop`) içinde bloke edici olarak mı yoksa asenkron bir geri çağırma (event-driven dispatch) olarak mı çalışması gerektiği mevcut kaynak kod yapısından teyit edilememektedir. Bu nedenle olay döngüsünün VM yığınında değil, interpreter tarafında barındırılması ve olay başı tekil VM delegasyonu yapılması önerilir.
