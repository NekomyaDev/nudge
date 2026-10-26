<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Ajanlarınızı parse etmekle uğraşmayın. Onları Nudge edin.</strong><br>
  Yapay zeka ajanları için tipli, yeniden oynatılabilir ve bütçe farkındalığına sahip bir programlama dili.<br>
  Yerel bytecode çalışma zamanı ve bağımsız derleyici. Python ve TypeScript'e derlenir.
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.4.0-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Native%20%7C%20Python%20%7C%20TypeScript%20%7C%20WASM-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
  <a href="https://snyk.io"><img alt="Snyk Verified" src="https://img.shields.io/badge/Snyk-Verified%20Secure-4C556A?logo=snyk"></a>
</p>

---

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.zh-CN.md">中文</a> •
  <a href="README.ja-JP.md">日本語</a> •
  <a href="README.ko-KR.md">한국어</a> •
  <a href="README.ru-RU.md">Русский</a> •
  <a href="README.es-ES.md">Español</a> •
  <a href="README.tr-TR.md">Türkçe</a> •
  <a href="README.de-DE.md">Deutsch</a> •
  <a href="README.fr-FR.md">Français</a>
</p>

---

## Neden Nudge?

Üretim ortamındaki ajanlar hâlâ yapıştırıcı kodlarla (glue code) bir arada tutuluyor: elle ayrıştırılan prompt zincirleri, try/except bloklarıyla sarılmış tool çağrıları, eksik replay yeteneği, öngörülemeyen maliyetler ve olmayan regresyon testleri. Kütüphaneler yalnızca sorun ortaya çıktıktan sonra semptomları yamamaya çalışır. **Nudge ise sorunun asıl yaşadığı katmanı onarır: dili.**

| Sorun Noktası | Kütüphaneler | Nudge |
|:---|:---|:---|
| Tipsiz LLM çıktısı | Çalışma zamanında doğrula | Schema, derleme zamanında doğrulanan birinci sınıf bir dil tipidir |
| Gizli yan etkiler | Görünmez | İmzalarda bildirilen 10 açık ve tipli effect (`uses LLM, Tool, IO, ...`) |
| Regresyon testinin olmaması | Sonradan eklenen kayıt ve replay | Her çalıştırma bir NTF trace'i yazar ve her trace deterministik bir 0$ testtir |
| Sürpriz maliyetler | Fatura geldikten sonra izlenir | Budget, derleyici ve çalışma zamanı tarafından zorunlu kılınan kesin bir sözleşmedir |
| Eşzamanlılık spagettisi | Manuel asenkron döngüler | Derleme zamanında yarış güvenliği (race safety) sunan yerel `par map / race / all` |

## Nudge'a Bir Bakış

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // zero tokens burned in CI
}
```

Derleyici schema'nın eşleştiğini kanıtlar, effect'leri çıkarır ve en kötü durum maliyetini sınırlandırır. Çalışma zamanı ise her çağrıyı diff alabileceğiniz, commit edebileceğiniz ve 0$'a yeniden oynatabileceğiniz (replay) içerik adresli bir trace'e kaydeder.

## Hızlı Başlangıç

### 1. Nudge'ı Kurun

**Tek satırlık yükleyici (Önerilen):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell as Administrator)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Paket yöneticileri ve konteynerler:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Linux, macOS ve Windows için önceden derlenmiş ikili dosyalar [Releases](https://github.com/NekomyaDev/nudge/releases) sayfasında da mevcuttur.

### 2. 60 Saniyede İlk Çalıştırma (Native, 0$)

Python, Node.js veya API anahtarı gerekmez. Nudge yerel bir yığın VM'si (stack VM) ve bellek içi deterministik bir fake provider içerir:

```sh
# Create a program
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Create a greeting for {name}. Return message and timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

fn main() -> Greeting uses LLM {
    greet("World")
}
EOF

# Type-check your program
nudgec check hello.ndg

# Run directly on the native Bytecode VM ($0, no keys needed)
nudgec run hello.ndg

# Run offline regression tests
nudgec test hello.ndg
```

### 3. Etkileşimli Dersler ve Proje İskeleti Oluşturma

```sh
# Six interactive lessons in your terminal (hello, types, decide, policy, test, trace)
nudgec learn

# Scaffold a project from ready-made templates (triage, chatbot, rag-agent, etc.)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Python veya TypeScript'e Derleyin (İsteğe Bağlı)

Mevcut bir Python veya TypeScript servisine dağıtım yapıyorsanız:

```sh
# Python target
pip install nudge-runtime          # standard library only, zero dependencies
nudgec build hello.ndg              # emits out/hello.py
python3 out/hello.py

# TypeScript target
nudgec build-ts hello.ndg             # emits out/hello.ts
```

## Bir Bakışta Temel Özellikler

| Özellik | Ne Sağlar | Bölüm |
|:---|:---|:---|
| **Tipli LLM Çağrıları** | Çıktı schema'sı bir tiptir ve ihlaller otomatik onarımı tetikler | [Nudge'a Bir Bakış](#nudgea-bir-bakış) |
| **Kontrol Akışı ve Tipler** | `if/else`, `for` döngüleri, record sabitleri `{k: v}`, indeksleme, değişebilir yerel değişkenler, Opsiyonel (`T?`), Union (`T \| U`) | [Kontrol Akışı ve Veri](#kontrol-akışı-ve-veri) |
| **Tipli Effect'ler** | Derleme zamanında takip edilen ve denetlenen bilinen 10 effect | [Tipli Effect'ler](#tipli-effectler) |
| **Workspace ve Store** | Korumalı alanlı kapsamlı dosya sistemi + tipli kalıcı anahtar-değer deposu | [Kapsamlı Workspace ve Kalıcı Store](#kapsamlı-workspace-ve-kalıcı-store) |
| **Yerel Arayüz (Local UI)** | Tipli görünümler, pencereler, headless testler ve yerel işletim sistemi masaüstü penceresi | [Yerel Arayüz (Local UI)](#yerel-arayüz-local-ui) |
| **Görevler ve Eşzamanlılık** | Görev spawn/join, işbirlikçi iptal, güvenli `par race`, bütçe serbest bırakma | [Görev Yaşam Döngüsü ve Eşzamanlılık](#görev-yaşam-döngüsü-ve-eşzamanlılık) |
| **Provider Kimlik Doğrulama** | Fail-closed RFC 8628 OAuth cihaz akışı, üçüncü taraf token toplamama | [Provider Kimlik Doğrulama](#provider-kimlik-doğrulama) |
| **Tipli Kararlar** | Durumlar üzerinde toplu sınıflandırma, evet/hayır, rubrik puanlama (`/v1/systemone`) | [Tipli Kararlar](#tipli-kararlar) |
| **Bilgisayar Kullanımı (Computer Use)** | Ekran gözlemi, tipli eylemler, uygulama izin listesi, acil durdurma anahtarı (kill switch) | [Bilgisayar Kullanımı (Computer Use)](#bilgisayar-kullanımı-computer-use) |
| **Bytecode VM Motoru** | Varsayılan yığın VM motoru (~70 kat daha hızlı saf hesaplama, derin özyineleme güvenliği) | [Bytecode VM Motoru](#bytecode-vm-motoru) |
| **Bağımsız Ajanlar** | Cargo gerektirmeyen tek bir ikili dosya paketleme (~2.09MB canlı, ~832KB replay) | [Bağımsız Yerel Ajanlar (Cargo Gerektirmez)](#bağımsız-yerel-ajanlar-cargo-gerektirmez) |
| **WebAssembly** | WASI içinde çevrimdışı replay (~526KB), canlı araç ana makine köprüsü | [WebAssembly Hedefi](#webassembly-hedefi) |
| **Deterministik Trace'ler** | Açık NTF v1 trace'leri, 0$ replay, CI regresyon kapıları, trace diff alma | [Deterministik Trace'ler ve Ajan CI](#deterministik-traceler-ve-ajan-ci) |
| **Gizlilik ve Sansürleme** | Tek bir kontrol noktasında gizli bilgilerin ve kişisel verilerin (PII) varsayılan otomatik sansürlenmesi | [Gizlilik ve Trace Sansürleme](#gizlilik-ve-trace-sansürleme) |

## Dil ve Yetenekler

### Kontrol Akışı ve Veri

Nudge, deterministik güvenlik sınırlarıyla tanıdık yapılandırılmış programlama yapıları sunar:

```nudge
fn process_scores(raw: [int]) -> { total: int, count: int } {
    let mut total = 0
    let mut count = 0

    for (score, idx) in raw {
        if score >= 50 {
            total = total + score
            count = count + 1
        } else {
            // skip failing scores
        }
    }

    { total: total, count: count }
}
```

- **Dallanma:** Çoklu ifade içeren dal bloklarını destekleyen `if/else` ve `else if` ifadeleri.
- **Yineleme:** Sıralı `for item in coll` döngüleri ve indeksli `for (item, idx) in coll` döngüleri.
- **Yapılandırılmış Veri:** Statik tipleme ve alan erişimine (`obj.field`) sahip record sabitleri `{ field1: expr1, field2: expr2 }`.
- **İndeksleme:** Koleksiyonlar ve record'lar üzerinde indeksli erişim (`items[0]`, `map["key"]`). Sınır dışı hataları `E0705` ile temiz bir şekilde sonlanır.
- **Değişebilir Yerel Değişkenler:** Yeniden atama yapılabilen (`name = next`) açık `let mut name = init` değişkenleri.
- **Döngü Sınırları:** Kontrolden çıkan çalıştırmaları önlemek için döngüler `NUDGE_MAX_LOOP_ITERATIONS` (varsayılan 10.000) ile sınırlandırılmıştır ve aşım durumunda `E0703` hatası verir.
- **Opsiyonel Tipler (`T?`):** `?` ile ifade edilen null olabilen tipler (örneğin `string?`, `int?`, `(int | string)?`). Değer yoksa `none` olur; `== none` ve `!= none` ile kontrol edilir. Denetlenmeyen kullanımlar fail-closed olarak `E0201` hatası verir.
- **Union Tipleri (`T | U`):** Belirtilen alternatif tipleri kabul eden statik union tipleri (örneğin `int | string` veya `string | bool`). Derleme zamanı genişletme denetimlerini ve katı daraltma reddini (`E0201`) destekler; LLM çıktı şemalarında, JSON serileştirmede ve kod üretiminde tam olarak desteklenir.

### Tipli Effect'ler

Her yan etki fonksiyon imzasında bildirilir ve derleyici tarafından doğrulanır:

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Calling unlisted effects triggers compiler error E0301
}
```

Nudge bilinen 10 effect tanımlar (`crates/nudgec/src/check.rs`):
- `LLM`: Dil modeli tamamlama veya akış (streaming) prompt'larını çağırır.
- `Tool`: Harici MCP veya kayıtlı araçları çağırır.
- `IO`: Ana makine girdi/çıktı işlemleri (`print`, `println`).
- `Decision`: `decide{}` aracılığıyla karar modellerini sorgular.
- `Computer`: `computer.*` aracılığıyla masaüstü ortamları ve erişilebilirlik ağaçlarıyla etkileşime girer.
- `Workspace`: Korumalı alanlı workspace içinde kapsamlı okuma ve yazma işlemleri.
- `WorkspaceDelete`: Workspace içinde yıkıcı dosya veya dizin silme işlemi.
- `Store`: `store.*` aracılığıyla kalıcı anahtar-değer depolama işlemleri.
- `UI`: Grafik kullanıcı arayüzü öğeleri oluşturur, bağlar (mount) veya bunlara olaylar iletir.
- `Auth`: Kimliği doğrulanmış provider kaynaklarına erişir (kapsamlı `Auth(account)` destekler).

`uses` yan tümcesi bulunmayan fonksiyonların derleme zamanında saf (pure) olduğu kanıtlanır. Bilinmeyen effect'ler `E0302` ile reddedilir.

### Kapsamlı Workspace ve Kalıcı Store

Yerel ajanlar, sistem güvenliğinden ödün vermeden dosyalarla çalışabilmeli ve çalıştırmalar arasında durumlarını koruyabilmelidir:

```nudge
record Config {
    theme: string,
    max_entries: int
}

fn update_workspace_file(path: string, new_content: string) -> bool uses Workspace, Store {
    // 1. Scoped filesystem access (path traversal and symlink escapes denied)
    if workspace.exists(path) {
        workspace.write(path, new_content)   // Atomic write: staging, fsync, rename
    }

    // 2. Typed persistent store (survives process restarts and reboots)
    store.set("app_config", Config { theme: "dark", max_entries: 100 })
    true
}
```

- **Korumalı Alanlı Workspace (`workspace.*`):** Tüm işlemler açık bir workspace köküyle sınırlandırılmıştır (`--workspace` veya `NUDGE_WORKSPACE_DIR` ile yapılandırılır, varsayılan olarak geçerli dizindir). Dizin geçişleri (`../`) ve kök dışına sembolik bağ (symlink) kaçışları engellenir. Yazma işlemleri çökmeye karşı tutarlıdır (geçici dosyaya atomik yazma ve yeniden adlandırma).
- **Silme Yetkisinin Ayrılması:** Dosya veya dizinleri silmek özel `WorkspaceDelete` effect'ini gerektirir (`uses WorkspaceDelete`), böylece genel yazma yordamlarının dizinleri silmesi engellenir.
- **Kalıcı Anahtar-Değer Deposu (`store.*`):** `<workspace>/.nudge/store.json` (veya `NUDGE_STORE_PATH`) konumuna kaydedilen güçlü tipli depolama. Geçici çalıştırma kontrol noktalarının aksine, bu veri deposu farklı çalıştırmalar ve yeniden başlatmalar boyunca kalıcıdır.

### Süreç ve Dosya İzleme

Derleme/test çalıştırıcılarını, dil sunucularını ve canlı yeniden yükleme araçlarını yöneten ajanlar geliştirin (diğer her şeyle aynı effect ve yetki disipliniyle):

```nudge
fn watch_and_rebuild() -> bool uses FsWatch, Process {
    // 1. Bounded, workspace-confined file watching (no indefinite hang)
    let changes = fs.watch("src", 5000)          // dir/path + timeout (ms)

    // 2. Controlled child process: program and args SEPARATELY (never a shell)
    let handle = proc.spawn("cargo", ["build", "--quiet"], 60000)
    let out = proc.await(handle)
    proc.alive(handle) == false
}
```

- **Dosya İzleme (`fs.watch`, `FsWatch` effect'i):** Bir workspace yolunu değişiklik olayları için izler. Bir zaman aşımıyla sınırlandırılmıştır (asla süresiz olarak takılmaz) ve workspace köküyle sınırlandırılmıştır (kaçış reddedilir `E0922`). Bildirilmemiş kullanım tip denetimiyle engellenir (`E0301`).
- **Kontrollü Süreçler (`proc.*`, `Process` effect'i):** `proc.spawn(cmd, argv, timeout_ms)` / `proc.run(...)` (spawn + await) / `proc.await` / `proc.kill` / `proc.alive`. `cmd` ve `argv` argümanları **ayrı ayrı** iletilir ve kabuk (shell) olmadan yürütülür (kabuk enjeksiyonu yüzeyi yoktur). Yaşam döngüsü sınırlandırılmıştır (`E0921`), **öksüz (orphan) süreç bırakılmaz** (alt süreçler görev sonlandığında veya iptal edildiğinde temizlenir) ve iptal durumunda sonlandırma (kill-on-cancel) uygulanır. Herhangi bir şey yürütmek **hata durumunda kapalıdır (fail-closed)**: `NUDGE_PROCESS_GRANTS` ile yetkilendirilmedikçe varsayılan olarak reddedilir (`E0920`).

### Yerel Arayüz (Local UI)

Doğrudan Nudge içinde yerel döngüde-insan (human-in-the-loop) arayüzleri, onay istasyonları ve etkileşimli inceleme panelleri oluşturun:

```nudge
type ReviewState = { prompt: string, approved: bool }

view ReviewCard(state: ReviewState) -> View {
    column(padding = 16, gap = 12) [
        text("Agent Review Station", size = 18, bold = true),
        text("Request: " + state.prompt),
        row(gap = 8) [
            button("Approve", on_click = approve_action),
            button("Reject", on_click = reject_action)
        ]
    ]
}

agent ReviewStation {
    state {
        prompt: string = "Execute deploy script",
        approved: bool = false
    }

    fn approve_action() uses UI, IO {
        state.approved = true
        io.println("Action approved")
    }

    fn reject_action() uses UI {
        state.approved = false
    }

    window MainWindow {
        title: "Nudge Review Station",
        width: 500,
        height: 300,
        content: ReviewCard(state)
    }
}
```

- **Çift Modlu Yürütme:**
  - **Headless Test:** `mount_headless()` ve `dispatch_click()`, fiziksel bir pencere açmadan UI ağaçlarını ve durum geçişlerini tamamen bellek içinde değerlendirir. CI ortamında 0$ maliyetle çalışır.
  - **Yerel Masaüstü GUI:** `native-gui` Cargo özelliğiyle derlendiğinde, `window.open()` fonksiyonu `winit` ve `softbuffer` destekli yerel bir işletim sistemi penceresi oluşturur.
- **Headless Geri Çekilme (Fallback):** `NUDGE_UI_HEADLESS=1` ayarlandığında veya trace replay (`NUDGE_REPLAY`) sırasında çalıştırıldığında pencere çağrıları otomatik olarak headless motora yönlendirilir.

### Görev Yaşam Döngüsü ve Eşzamanlılık

Resmi görev durumu yönetimi ve işbirlikçi iptal (cooperative cancellation) ile eşzamanlı ajan kollarını koordine edin:

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race automatically cancels losing branches
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **İşbirlikçi İptal (Cooperative Cancellation):** Bir yarış (race) çözümlendiğinde, kaybeden kollar arka planda bütçe harcamaya devam etmek yerine döngü, akış (stream) ve effect sınırlarında durdurulur.
- **Bütçe Koruması:** `NUDGE_BUDGET` altındaki harcanmamış bütçe rezervasyonları görev iptal edildiğinde derhal serbest bırakılır.
- **Fail-Closed Yetkileri:** İptal edilen görevler daha fazla tool çağrısı (`Tool`) veya bilgisayar eylemi (`Computer`) gönderme izinlerini anında kaybeder.
- **Görev Temel Öğeleri (Primitives):** `task.spawn`, `task.join`, `task.status` ve `task.cancel` aracılığıyla açık görev kontrolü.

### Provider Kimlik Doğrulama

Doğrudan dile ve çalışma zamanına yerleşik güvenli kimlik bilgisi yönetimi:

- **Hata Durumunda Kapalı Kimlik Doğrulama:** Fonksiyonlar `uses Auth` veya `uses Auth(account)` ile açık kimlik doğrulama gereksinimlerini bildirir. Kimliği doğrulanmamış çağrılar kapalı kalarak başarısız olur.
- **Üçüncü Taraf Token Toplamama:** Nudge hiçbir zaman üçüncü taraf araçlara ait özel token önbelleklerini veya kimlik bilgisi depolarını okumaz.
- **Yerel RFC 8628 Cihaz Akışı:** OAuth 2.0 Device Authorization Grant ile etkileşimli terminal yetkilendirmesi ve `~/.nudge/credentials.json` (dosya modu 0600) içinde güvenli kimlik bilgisi depolaması.
- **Provider Kapıları:** Provider'lar doğrulanabilir güvenlik sözleşmelerini karşılamalıdır (eksik kimlik bilgileri için `E-auth-0401`, kapsam yetersizlikleri için `E-auth-0403`, devre dışı bırakılmış provider'lar için `E-auth-0404`).
- **Kanıtlanmamış Provider'lar Kapalı Kalır (İzin Kapısı):** Resmi üçüncü taraf kullanım izni kanıtlanmamış bir provider **varsayılan olarak fail-closed'dır**. Bu durum şu anda `antigravity` için geçerlidir: OAuth giriş akışı ve provider kodu mevcuttur, ancak OAuth istemcisinin ve model uç noktasının bağımsız bir üçüncü taraf istemciden kullanılabileceğine dair resmi bir kanıt bulunamadığından, izin doğrulanana kadar kapı **devre dışı** (`E-auth-0404`) kalır. **Sahte (mock) bir OAuth testi veya başarılı bir CI çalıştırması, canlı hesap erişimini veya kullanım iznini KANITLAMAZ.** Tüketiciler (Nudge üzerine inşa edilmiş herhangi bir uygulama dahil), bu izin belgelenene kadar bu bağlantıya güvenmemelidir. İzin doğrulandıktan sonra kullanıcının açıkça belirttiği bir katılım (`NUDGE_ENABLE_ANTIGRAVITY=1`), riski kullanıcıya ait olmak üzere bağlantıyı etkinleştirir.

### Tipli Kararlar

`/v1/systemone` kablo protokolü aracılığıyla karar modellerinden (Laya ve Jev gibi) gelen belirsizliği derleyin:

```nudge
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept:    "which team handles this?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "laya:multilingual", deadline: 50 }   // ms latency budget
    route {
        auto: resolve(d.dept.winner) when d.dept.confidence > 0.8,
        review: escalate(d) otherwise
    }
}
```

- **Toplu Yürütme:** Tek bir çağrı tüm soruları aynı anda değerlendirerek kazananı, olasılığı, dağılımı ve güven skorunu döndürür.
- **Sıfır Dolar Maliyetli Test:** Varsayılan sahte (fake) karar provider'ı, 0$ maliyetli test çalıştırmaları için deterministik tohumlanmış dağılımlar sentezler.
- **Canlı Provider'lar:** `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'` aracılığıyla yerel veya uzak modelleri bağlayın.
- **Karar Önbelleği (Decision Cache):** `NUDGE_DECISION_CACHE=<path>`, doğrulanmış yanıtları çalıştırmalar arasında önbelleğe alır, böylece aynı durum için asla iki kez ödeme yapmazsınız.
- **Politika Taramaları (Policy Sweeps):** Kaydedilmiş dağılımlar üzerinde sıfır model çağrısıyla eşik ayarlamalarını değerlendirmek için `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` komutunu çalıştırın. [Karar Dokümantasyonu](docs/decision.md) sayfasını okuyun.

### Bilgisayar Kullanımı (Computer Use)

Dilin kontrol ve güvenliği üstlendiği, modelin ise algıyı yönettiği tipli gözlemler ve eylemlerle masaüstü ortamlarını kontrol edin:

```nudge
fn automate_app(app: string) -> bool uses Computer, Decision {
    let obs = computer.observe(app, allow = [app], deadline = 30000)
    let check = decide {
        ready: "is the target button visible?" yes/no
    } on obs.tree
    with { deadline: 100 }

    let r = route {
        click: computer.click(1, allow = [app]) when check.ready.p > 0.5,
        close: computer.key("Escape", allow = [app]) otherwise
    }
    r.ok
}
```

- **Tipli Temel Öğeler:** `computer.observe`, bir `Observation` kaydı döndürür (erişilebilirlik ağacı, öğeler, isteğe bağlı ekran görüntüsü). Eylemler (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`), iletim makbuzlarıyla (`action_sent`) birlikte `ActionResult` döndürür.
- **Replay Güvenliği:** Trace replay işlemi asla fiziksel eylemleri yeniden tetiklemez. `NUDGE_COMPUTER_DRIFT=1` ile çalışma zamanı canlı durumu yeniden gözlemler ve eylemleri yürütmeden mekanik bir `Drift` farkı hesaplar.
- **Sertleştirme:** Uygulama izin listesi (`allow: [app]`), denetleyici kiralama kilitleri (lease locks), eski öğe indekslerinin reddedilmesi (`stale_state`) ve donanımsal bir acil durdurma anahtarı (`NUDGE_COMPUTER_KILL=1`).
- **Deterministik Masaüstü:** Varsayılan sahte (fake) provider, bellek içi kullanıcı arayüzü senaryolarını (`NUDGE_COMPUTER_SCENARIO`) çalıştırarak 0$ maliyetli uçtan uca testlere olanak tanır. [Bilgisayar Kullanımı Dokümantasyonu](docs/computer-use.md) sayfasını okuyun.

## Yerel Altyapı

### Bytecode VM Motoru

Saf fonksiyonlar, veri dönüştürmeleri ve rota dallanmaları özel bir yığın tabanlı bytecode VM üzerinde yürütülür (`crates/nudgec/src/bytecode.rs` ve `vm.rs`):

- **Yüksek Hızlı Saf Hesaplama:** Saf kodları AST yorumlayıcısından yaklaşık 70 kat daha hızlı yürütür.
- **Derin Özyineleme Güvenliği:** Yönetilen yığın çerçevesi (frame stack), özyinelemeli algoritmalarda işletim sistemi yığın taşması riskini ortadan kaldırır.
- **Şeffaf Effect Devri:** Yan etkisi olan ifadeler (`llm`, `tool`, `computer`, `workspace`), `DelegateCall` ve `DelegateExpr` aracılığıyla VM'den çalışma zamanı değerlendiricisine sorunsuz şekilde devredilir.
- **Varsayılan Motor:** VM, `nudgec run` ve bağımsız ikili dosyalar için varsayılan olarak çalışır. AST yorumlayıcısına geri dönmek için `NUDGE_VM=0` ayarlayın.

### Bağımsız Yerel Ajanlar (Cargo Gerektirmez)

Ajanınızı derleme ortamı araç zinciri bağımlılıkları olmadan tek, bağımsız bir yerel çalıştırılabilir ikili dosya olarak paketleyin:

```sh
# Package an agent into a standalone binary
nudgec build --standalone my-agent.ndg -o bin/my-agent

# Run directly on the host (zero Python/Node/Rust dependencies)
./bin/my-agent
```

- **AST Damgalama:** `nudgec` kaynak kodu tip denetiminden geçirir, AST paketini bincode formatında serileştirir ve milisaniyeler içinde önceden derlenmiş `nudge-agent` çalışma zamanı ikili dosyasına ekler (`NDGAGNT1` altbilgisi).
- **İki Çalışma Zamanı Türü:**
  - **Tam (~2.09MB, varsayılan):** Canlı LLM'ler ve MCP sunucuları için gömülü `ureq` + `rustls` HTTP/TLS yığınına sahip canlı kullanıma hazır çalışma zamanı.
  - **Yalnızca Replay (~832KB):** Ağ bağımlılıkları çıkarılmış, 0$ maliyetli çevrimdışı yürütme için tüm işlemleri kaydedilmiş NTF trace'lerinden sunan kompakt çalışma zamanı.
- **Çalışma Zamanı Keşfi:** `NUDGE_AGENT_RUNTIME` ortam değişkeni aracılığıyla veya `nudgec` yanındaki kardeş ikili dosya olarak bulunur. [Bağımsız Dağıtım Dokümantasyonu](docs/standalone.md) sayfasını okuyun.

### WebAssembly Hedefi

Ajanları tarayıcı korumalı alanlarına, edge çalışma zamanlarına (Cloudflare Workers, Fastly) ve WASI ortamlarına dağıtın:

- **Çevrimdışı Replay (`nudge-replay-wasi`):** Kaydedilmiş NTF trace'lerini `wasmtime` altında yaklaşık 526KB boyutundaki bir `.wasm` modülü içinde yerel çalıştırmalarla bayt bayt aynı çıktıyla çevrimdışı olarak yeniden oynatın:
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **Canlı Tool Ana Makine Köprüsü:** Tarayıcı veya edge JavaScript çalışma zamanlarındaki ajanlar, standartlaştırılmış `nudge_host_tool_call` içe aktarma sözleşmesi aracılığıyla canlı tool çağrıları yapar (Node WASI altında doğrulanmıştır, bkz. `examples/wasm-host/`).
- **Korumalı Alan Güvenliği:** Ağ soketleri ve işletim sistemi süreç başlatma işlemleri `wasm32` üzerinde sınırlandırılmıştır ve korumalı alanda güvenli yürütme sağlar. [WebAssembly Dokümantasyonu](docs/wasm.md) sayfasını okuyun.

## Araçlar ve Entegrasyonlar

### MCP Entegrasyonu

stdio üzerinden iletişim kuran Model Context Protocol (MCP) sunucuları tarafından desteklenen araçları bildirin:

```nudge
type SearchResult = { url: string, title: string }

tool web_search(query: string) -> [SearchResult] {
    impl: mcp("search").web_search(query)
    side_effects: none
}

fn main() -> [SearchResult] uses Tool {
    web_search("nudge language")
}
```

Canlı sunucuları `NUDGE_MCP_SERVERS` üzerinden yapılandırın:

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

Parametreler, FastMCP ve standart sunucularla uyumluluk için adlandırılmış JSON argüman nesneleri olarak iletilir. Eksik sunucular net tanılama mesajlarıyla hızlıca başarısız olur.

### Deterministik Trace'ler ve Ajan CI

Her çalıştırma, tüm effect etkileşimlerini yakalayan açık bir NTF v1 JSONL trace'i üretir:

```sh
# Verify trace integrity against the open specification
nudgec trace-check trace.jsonl

# Compare two traces to identify cost or behavior changes (--json makine tarafından okunabilir rapor üretir)
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression --json
```

**GitHub Action (`action.yml`):** Ajanları her push işleminde sıfır API anahtarıyla ve 0$ maliyetle CI'da test edin:

```yaml
name: agents
on: [push, pull_request]
jobs:
  test-agents:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: NekomyaDev/nudge@v1
        with:
          files: "agents/**/*.ndg"
          trace-diff: "baseline.jsonl candidate.jsonl"
```

Dondurulmuş NTF spesifikasyonu [docs/ntf-spec.md](docs/ntf-spec.md) adresinde yer almaktadır; ayrıca [`conformance/`](conformance/) dizininde tam bir test paketi ve LangChain, CrewAI ve LangGraph için köprüler bulunmaktadır.

### Geliştirici Araçları

- **Statik Maliyet Tahmincisi:** `nudgec cost <file.ndg> [--json]`, derleme zamanında fonksiyon başına en kötü durum token maliyetlerini ve bütçelerini hesaplar.
- **Özellik ve Fuzz Testi:** `nudgec fuzz [file.ndg] [--iterations N] [--seed S] [--json]`, mutasyon tabanlı fuzz testlerini ve özellik doğrulamasını çalıştırır.
- **Veri Kümesi Değerlendirmesi:** `nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8] [--json]`, fonksiyonları JSONL veri kümelerine karşı kıyaslar ve CI'ı doğruluk eşiğine göre kilitler.
- **Değerlendirme Karşılaştırması:** `nudgec compare <results_a.jsonl> <results_b.jsonl> [--json]`, performans veya doğruluk gerilemelerini tespit etmek için iki değerlendirme çalıştırmasını karşılaştırır.
- **Çalıştırma Açıklayıcısı:** `nudgec explain <trace.jsonl> [--json]`, hataları, token tüketimini ve düşük güvenilirlikli kararları vurgulayan insan tarafından okunabilir bir CLI özeti veya JSON analizi üretir.
- **Trace Diff Alma:** `nudgec trace-diff <a.jsonl> <b.jsonl> [--json] [--fail-on-regression]`, regresyonları tespit etmek için iki çalıştırma trace'ini karşılaştırır; kilitlendiğinde regresyonda kod 1 ile çıkar.
- **Uyumluluk Denetimi:** `nudgec audit <trace.jsonl> [--json]`, bir NTF trace'ini AB Yapay Zeka Yasası uyumluluk kriterlerine (şeffaflık, yönetişim, sansürlenmemiş sırlar, risk metrikleri) göre denetler.
- **Yerel Trace Görüntüleyici:** `nudgec trace-view <trace.jsonl> [--port N] [--no-open]`, görsel yürütme zaman çizelgeleri, token harcamaları ve onarım vurguları içeren yerel bir web arayüzü başlatır.
- **Formatlayıcı ve İzleyici:** `nudgec fmt <file.ndg> [--check] [--json]`, kodu güvenli bir şekilde formatlar ve `nudgec check <file.ndg> [--json] --watch` her kaydetmede tip kontrolü yapar.
- **Linter ve Otomatik Düzeltme:** `nudgec lint <file.ndg> [--fix] [--json]`, programları en iyi uygulamalar (sınırsız bütçeler, belirsiz prompt'lar, eksik onarımlar) açısından tarar ve güvenli düzeltmeleri uygular.
- **Çalıştırma Deposu Denetleyicisi:** `nudgec runs [--run <run_id>] [--json]`, dayanıklı ajan çalıştırmaları tarafından oluşturulan kalıcı yürütme durumlarını, trace'leri ve kontrol noktalarını inceler.
- **Kimlik Doğrulama Yöneticisi:** `nudgec auth login [provider]` ve `nudgec auth status [--json]`, OAuth kimlik bilgilerini yönetir ve bağlı yapay zeka sağlayıcılarını doğrular.
- **Statik HTML Dışa Aktarma:** `nudgec trace-html <trace.jsonl> [-o report.html]`, bağımsız ve etkileşimli denetim raporları dışa aktarır.

### VS Code Eklentisi

`nudgec lsp` tarafından desteklenen sözdizimi vurgulama, kod parçacıkları, üzerine gelme (hover) dokümantasyonu ve gerçek zamanlı tip tanılamaları için [Nudge Language Extension](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) eklentisini yükleyin.

## Gizlilik ve Trace Sansürleme

Yapay zeka ajanları sıklıkla hassas token'ları, yetkilendirme başlıklarını, özel anahtarları ve kullanıcıların kişisel verilerini (PII) işler. Nudge, verilerinizi hata durumunda kapalı kalan (fail-closed) bir trace sansürleme hattı ile korur:

- **Tek Kontrol Noktası:** Sansürleme, kayıtlar diske veya bellek arabelleklerine serileştirilmeden önce `TraceWriter::emit` içinde gerçekleşir. Her kayıt türü (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) bu tek darboğazda maskelenir.
- **Varsayılan Olarak Fail-Closed:** Trace sansürleme varsayılan olarak etkindir (`NUDGE_TRACE_REDACT=1`). `NUDGE_TRACE_REDACT=0` ayarını yalnızca açık yerel hata ayıklama durumlarında kullanın.
- **`NUDGE_TRACE_REDACT=0` bir kolaylık değil, bir güvenlik sınırıdır:** Sansürlemeyi devre dışı bırakmak gizli anahtarları, API anahtarlarını, yetkilendirme başlıklarını ve kişisel verileri trace dosyasına **DÜZ METİN (PLAINTEXT)** olarak kaydeder. Bu durum, kapatma anahtarının belgelenmiş ve hedeflenen sonucudur (testlerle doğrulanmıştır). Bunu asla CI ortamında, paylaşılan veya commit edilen dosyalarla ya da Nudge üzerine inşa edilmiş bir uygulama trace'leri işlerken kullanmayın. Alt tüketiciler `NUDGE_TRACE_REDACT=0` varlığını güvenlikle ilgili kritik bir bayrak olarak ele almalı ve bu moddaki trace'lerin depolanmasının veya paylaşılmasının güvenli olduğunu ASLA varsaymamalıdır.
- **Otomatik Gizli Bilgi Tespiti:** `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT` ile biten veya `AUTH` içeren (6 veya daha fazla karaktere sahip değerler) ortam değişkenlerini otomatik olarak tanımlar ve maskeler.
- **Katmanlı Kalıp Maskeleme:** Taşıyıcı token'ları (bearer tokens), yetkilendirme başlıklarını, anahtar-değer kimlik bilgisi çiftlerini, özel anahtar bloklarını ve sağlayıcı token öneklerini (OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`) temizler.
- **Kriptografik Replay Değişmezliği:** İçerik adresli `request_hash`, sansürleme öncesindeki girdiler üzerinden hesaplanır. Bu, maskelenmiş trace'lerin kimlik bilgilerini diske kaydetmeden O(1) eşleşmeyle replay araması sırasında bayt düzeyinde aynı kalmasını garanti eder.

## Ortam Değişkenleri

| Değişken | Kapsam | Amaç | Varsayılan |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | both | Aktif LLM provider'ı (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | native | Bytecode VM yürütme motoru geçişi (`1` = VM varsayılan, `0` = AST yorumlayıcıya geri dönüş) | `1` |
| `NUDGE_DEFAULT_MODEL` | native | `with` bloğunda açıkça belirtilmediğinde varsayılan model dizesi | Provider varsayılanı |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | both | OpenAI uyumlu LLM provider'ları için kimlik bilgileri ve temel uç nokta | Yok |
| `NUDGE_PRICING` | both | Özel JSON fiyatlandırma tablosu: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | Yerleşik tablo |
| `NUDGE_GUARD` | python/TS | Çıktı koruyucuları (`pii`, LLM çıktılarındaki gizli bilgileri, e-postaları, IP adreslerini maskeler) | Yok |
| `NUDGE_BUDGET` | both | Paralel dallanmalarda uygulanan çalıştırma geneli USD harcama tavanı | Yok |
| `NUDGE_REPAIR_BUDGET` | python | Onarım turları için kümülatif USD harcama tavanı | Yok |
| `NUDGE_MAX_LOOP_ITERATIONS` | native | Çalışma zamanı hatası `E0703` üretilmeden önceki maksimum sıralı döngü yinelemesi | `10000` |
| `NUDGE_TRACE` | both | NTF JSONL yürütme trace'inin yazılacağı hedef yol | `trace.jsonl` |
| `NUDGE_REPLAY` | both | Deterministik çevrimdışı replay için trace yolu (`NUDGE_REPLAY_MODE=all` veya `llm`) | Yok |
| `NUDGE_RESUME` | both | Kaydedilen trace önekini kullanarak çöken yürütmeyi kontrol noktasından devam ettirir | Yok |
| `NUDGE_TRACE_REDACT` | native | Trace'lerde tek kontrol noktasında gizli bilgi ve PII maskelemeyi etkinleştirir (`1` veya `0`) | `1` (Aktif) |
| `NUDGE_TRACE_REDACT_ENV` | native | Trace'lerdeki gizli ortam değişkenlerinin değerlerini otomatik olarak maskeler | `1` (Aktif) |
| `NUDGE_TRACE_REDACT_KEYS` | native | Maskelenecek ek JSON nesne anahtarlarının virgülle ayrılmış listesi | Yok |
| `NUDGE_TRACE_REDACT_PATTERNS` | native | Özel trace maskeleme için virgülle ayrılmış regex listesi veya bir regex dosyasının yolu | Yok |
| `NUDGE_WORKSPACE_DIR` | native | Korumalı alanlı `workspace.*` işlemleri için kök dizin yetkisi | Geçerli dizin |
| `NUDGE_STORE_PATH` | native | Kalıcı tipli anahtar-değer verileri için depolama dosyası konumu | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | native | UI pencere işlemlerini bellekte headless olarak çalışmaya zorlar (`1` veya `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | both | Yürütme katmanı JSON tool politikası (örneğin `{"web_search": ["*"]}`). Fail-closed çalışır | Yok |
| `NUDGE_PROCESS_GRANTS` | native | `proc.*` komutları ve argümanları için fail-closed JSON yürütme politikası | Yok (reddedilir) |
| `NUDGE_PROCESS_TIMEOUT` | native | `proc.*` için milisaniye cinsinden varsayılan alt süreç yürütme zaman aşımı | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | native | `fs.watch` için milisaniye cinsinden varsayılan yoklama ve bekleme zaman aşımı | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | native | Antigravity provider'ını etkinleştirmek için katılım anahtarı (`1` veya `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | native | Antigravity provider kimlik doğrulaması için doğrudan bearer token | Yok |
| `NUDGE_AUTH_GITHUB_COPILOT` | native | GitHub Copilot provider kimlik doğrulaması için doğrudan token geçersiz kılma | Yok |
| `NUDGE_MCP_SERVERS` | both | MCP sunucu adlarını stdio komutlarına eşleyen JSON kaydı | Yok |
| `NUDGE_MCP_TIMEOUT_MS` | native | MCP sunucuları için istek başına okuma süresi sınırı ve takılan oturum zaman aşımı | `30000` |
| `NUDGE_DECISION_SERVERS` | both | Karar modeli uç noktaları için JSON kaydı (örneğin `{"laya": {"base_url": "..."}}`) | Yok |
| `NUDGE_DECISION_CACHE` | both | Kalıcı hale getirilen karar modeli dağılımları için önbellek dosyası yolu | Yok |
| `NUDGE_COMPUTER_SERVERS` | both | Bilgisayar kullanımı köprüleri için JSON kaydı (alt süreç JSONL veya HTTP) | Yok |
| `NUDGE_COMPUTER_PROVIDER` | both | Bilgisayar eylemleri için seçilen köprü (`fake` veya kayıtlı sunucu adı) | `fake` |
| `NUDGE_COMPUTER_KILL` | both | Donanımsal durdurma anahtarı: `1` tüm bilgisayar işlemlerini anında reddeder | `0` |
| `NUDGE_COMPUTER_DRIFT` | both | `1` olduğunda, replay sırasında canlı durumu yeniden gözlemler ve mekanik drift farklarını ekler | `0` |
| `NUDGE_AGENT_RUNTIME` | native | `nudgec build --standalone` için önceden derlenmiş `nudge-agent` ikili dosyasının yolu | Kardeş ikili dosya |
| `NUDGE_CREDENTIALS_PATH` | native | Nudge kimlik doğrulama bilgileri deposunun özel yolu | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | python | OpenTelemetry span dışa aktarma dosyası | Yok |
| `NUDGE_OTEL_ENDPOINT` | python | OpenTelemetry HTTP dışa aktarma uç noktası | Yok |

`native`, Rust çalışma zamanı (`nudgec`/`nudge-runtime`) için geçerlidir. `python/TS`, üretilen eski çalışma zamanları için geçerlidir ve `both`, her iki çalışma zamanı ailesi için de geçerlidir.

## Gerçek Dünya Örnekleri

[`examples/`](examples/) dizinindeki eksiksiz ajan uygulamalarını inceleyin:

| Örnek | Alan | Öne Çıkanlar |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | Sohbet Asistanı | Tipli bellek schema'ları, deterministik replay, bütçe tavanları |
| [Triage Agent](examples/triage-agent/) | Destek Sınıflandırması | Toplu `decide{}` blokları, güven eşikleri, yönlendirme politikaları |
| [Code Reviewer](examples/code-reviewer/) | Yazılım Mühendisliği | Yapılandırılmış sözdizimi tanılamaları, otomatik rubrik puanlama |
| [Research Agent](examples/research_agent.ndg) | Çok Kaynaklı Sentez | `par map` ile paralel veri getirme, güvenilirlik puanlaması |
| [Computer Automation](examples/computer-automation/) | Masaüstü Ajanı | Ekran gözlemi, erişilebilirlik ağaçları, drift doğrulaması |
| [RAG Agent](examples/rag-agent/) | Bilgi Artırımlı Üretim (RAG) | MCP stdio getirme aracı bağlamaları, alıntı çıkarma |
| [Property Fuzzer](examples/property-fuzz/) | Ajan Testi | `for_all` değişmezlik testi, prompt enjeksiyonu fuzzing'i |
| [Dungeon Crawler](examples/dungeon-crawler/) | Durum Makinesi Simülasyonu | Saf Nudge, ajan durum geçişleri, sıra tabanlı dövüş |

## Lisans ve Kaynaktan Derleme

Nudge, **Apache-2.0** lisansı altında açık kaynaklıdır. Ayrıntılar için [LICENSE](LICENSE) dosyasına bakın.

### Kaynaktan Derleme

Derleyiciyi oluşturmak için Rust 1.75+ (kararlı) sürümü gereklidir:

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # compiler binary at target/release/nudgec
cargo test --workspace            # compiler, VM, and runtime test suite
```

Python çalışma zamanı `runtime/` dizininde bulunur (yalnızca standart kütüphane, sıfır harici bağımlılık):

```sh
pip install ./runtime
```

---

<p align="center">
  <a href="https://github.com/NekomyaDev">NekomyaDev</a> tarafından sevgiyle yapılmıştır
</p>
