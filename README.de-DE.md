<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Parst eure Agenten nicht. Gebt ihnen einen Nudge.</strong><br>
  Eine typisierte, reproduzierbare und budgetbewusste Programmiersprache für KI-Agenten.<br>
  Native Bytecode-Laufzeitumgebung und eigenständiger Compiler. Kompiliert nach Python und TypeScript.
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.5-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Native%20%7C%20Python%20%7C%20TypeScript%20%7C%20WASM-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
  <a href="https://github.com/NekomyaDev/nudge/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22"><img alt="Good First Issues" src="https://img.shields.io/github/issues/NekomyaDev/nudge/good%20first%20issue?color=7057ff&label=good%20first%20issues"></a>
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

## Warum Nudge?

Produktions-Agenten werden heute immer noch mit provisorischem Klebecode zusammengehalten: manuell analysierte Prompt-Ketten, in try/except-Blöcke gehüllte Tool-Aufrufe, fehlende Reproduzierbarkeit, unvorhersehbare Kosten und keine Regressionstests. Bibliotheken beheben Symptome erst im Nachhinein. **Nudge repariert genau die Ebene, auf der das Problem tatsächlich liegt: die Programmiersprache.**

| Schwachstelle | Herkömmliche Bibliotheken | Nudge |
|:---|:---|:---|
| Untypisierte LLM-Ausgaben | Zur Laufzeit validieren | Schema ist ein nativer Sprachtyp, statisch zur Compilezeit verifiziert |
| Versteckte Nebeneffekte | Unsichtbar | 10 explizite typisierte Effekte in Funktionssignaturen (`uses LLM, Tool, IO, ...`) |
| Keine Regressionstests | Aufwändig nachträglich angehängt | Jeder Lauf schreibt einen NTF-Trace, jeder Trace ist ein deterministischer 0$-Test |
| Kostenüberraschungen | Überwachung erst nach Rechnungseingang | Budget ist ein harter Vertrag, garantiert durch Compiler und Laufzeit |
| Asynchrone Spaghettistrukturen | Manuelle Async-Schleifen | Natives `par map / race / all` mit Compile-Zeit-Racesafety |

## Ein Vorgeschmack auf Nudge

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extrahiere überprüfbare Erkenntnisse über {q} aus: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "bleibt im Budget auf aufgezeichnetem Trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // null Token-Verbrauch in CI
}
```

Der Compiler beweist die Übereinstimmung mit dem Schema, leitet Effekte ab und begrenzt Worst-Case-Kosten. Die Laufzeitumgebung zeichnet jeden Aufruf in einem inhaltsadressierten Trace auf, der versioniert, verglichen und für 0$ wiederholt werden kann.

## Schnelleinstieg

### 1. Nudge installieren

**Einzeiliger Installer (Empfohlen):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell als Administrator)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Paketmanager und Container:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Vorkompilierte Binärdateien für Linux, macOS und Windows stehen auch auf der [Releases](https://github.com/NekomyaDev/nudge/releases)-Seite bereit.

### 2. 60-Sekunden-Erstlauf (Nativ, 0$)

Kein Python, kein Node.js und keine API-Schlüssel erforderlich. Nudge enthält eine native Stack-VM und einen speicherinternen, deterministischen Fake-Provider:

```sh
# Programm erstellen
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Erstelle eine Begrüßung für {name}. Gib message und timestamp zurück."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

fn main() -> Greeting uses LLM {
    greet("World")
}
EOF

# Programm typprüfen
nudgec check hello.ndg

# Direkt auf der nativen Bytecode-VM ausführen (0$, keine API-Keys nötig)
nudgec run hello.ndg

# Offline-Regressionstests ausführen
nudgec test hello.ndg
```

### 3. Interaktive Lektionen und Vorlagen

```sh
# Sechs interaktive Lektionen im Terminal
nudgec learn

# Projekt aus fertigen Vorlagen gerüsten (triage, chatbot, rag-agent, etc.)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Nach Python oder TypeScript kompilieren (Optional)

```sh
# Python-Target
pip install nudge-runtime          # nur Standardbibliothek, null Abhängigkeiten
nudgec build hello.ndg              # erzeugt out/hello.py
python3 out/hello.py

# TypeScript-Target
nudgec build-ts hello.ndg           # erzeugt out/hello.ts
```

## Hauptfunktionen im Überblick

| Funktion | Was sie bietet | Abschnitt |
|:---|:---|:---|
| **Typisierte LLM-Aufrufe** | Ausgabeschema ist ein Typ, Verletzungen triggern automatische Reparatur | [Ein Vorgeschmack](#ein-vorgeschmack-auf-nudge) |
| **Kontrollfluss & Typen** | `if/else`, `for`-Schleifen, Records `{k: v}`, Indexierung, mutable Locals, `T?`, `T \| U` | [Kontrollfluss & Daten](#kontrollfluss--daten) |
| **Typisierte Effekte** | 10 bekannte Effekte statisch zur Compilezeit verfolgt und geprüft | [Typisierte Effekte](#typisierte-effekte) |
| **Workspace & Speicher** | Sandboxed isoliertes Dateisystem + typisierter Key-Value-Speicher | [Workspace & Store](#isolierter-workspace--persistenter-speicher) |
| **Lokale Benutzeroberfläche** | Typisierte Views, Windows, Headless-Tests und natives Desktop-Fenster | [Lokale UI](#lokale-ui) |
| **Tasks & Nebenläufigkeit** | Task spawn/join, kooperative Stornierung, sicheres `par race`, Budget-Freigabe | [Task-Lebenszyklus & Concurrency](#task-lebenszyklus--concurrency) |
| **Provider-Authentifizierung** | Fail-Closed RFC 8628 OAuth Device Flow, kein fremdes Token-Harvesting | [Provider-Authentifizierung](#provider-authentifizierung) |
| **Typisierte Entscheidungen** | Gebündelte Klassifikation, Rubrik-Scoring (`/v1/systemone`) | [Typisierte Entscheidungen](#typisierte-entscheidungen) |
| **Computer Use** | Bildschirmsensorik, typisierte Aktionen, App-Allowlisting, Notaus-Schalter | [Computer Use](#computer-use) |
| **Bytecode-VM** | Standard Stack-VM (~70x schnellere reine Berechnungen, Rekursionssicherheit) | [Bytecode-VM-Engine](#bytecode-vm-engine) |
| **Eigenständige Agenten** | Einzelne Binary ohne Cargo-Abhängigkeit (~2.09MB live, ~832KB replay) | [Eigenständige native Agenten](#eigenständige-native-agenten-cargo-frei) |
| **WebAssembly** | Offline-Replay in WASI (~526KB), Live-Tool-Host-Brücke | [WebAssembly-Target](#webassembly-target) |
| **Deterministische Traces** | Offene NTF v1 Traces, 0$ Replay, CI-Regressionsgates | [Deterministische Traces & Agent-CI](#deterministische-traces--agent-ci) |
| **Datenschutz & Maskierung** | Automatische Geheimnis- und PII-Maskierung an einer zentralen Engstelle | [Datenschutz & Trace-Maskierung](#datenschutz--trace-maskierung) |

## Sprache & Fähigkeiten

### Kontrollfluss & Daten

Nudge bietet strukturierte Programmierkonstrukte mit deterministischen Sicherheitsgrenzen:

```nudge
fn process_scores(raw: [int]) -> { total: int, count: int } {
    let mut total = 0
    let mut count = 0

    for (score, idx) in raw {
        if score >= 50 {
            total = total + score
            count = count + 1
        }
    }

    { total: total, count: count }
}
```

- **Verzweigungen:** `if/else` und `else if` Ausdrücke für mehrzeilige Verzweigungsblöcke.
- **Iteration:** sequentielle `for item in coll` Schleifen und indexierte `for (item, idx) in coll` Schleifen.
- **Strukturierte Daten:** Record-Literale `{ field1: expr1, field2: expr2 }` mit statischer Typisierung und Feldzugriff (`obj.field`).
- **Indexierung:** Indexzugriff auf Collections und Records (`items[0]`, `map["key"]`). Bereichsfehler schlagen sauber mit `E0705` fehl.
- **Veränderbare Lokale Variablen:** explizite `let mut name = init` Variablen mit Neuzuweisung (`name = next`).
- **Schleifenbegrenzung:** Schleifen sind durch `NUDGE_MAX_LOOP_ITERATIONS` (Standard: 10.000) begrenzt, um Endlosschleifen zu verhindern (`E0703`).
- **Optionale Typen (`T?`):** Nullable Werte mit `?` dargestellt (`string?`, `int?`). Fehlende Werte nehmen `none` an. Ungeprüfte Nutzung scheitert mit `E0201`.
- **Union-Typen (`T | U`):** Statische alternative Typen (`int | string`). Unterstützt statische Widening-Prüfungen und strikte Narrowing-Ablehnung (`E0201`).

### Typisierte Effekte

Jeder Nebeneffekt wird in der Funktionssignatur deklariert und vom Compiler validiert:

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Nicht deklarierte Effekte lösen Compiler-Fehler E0301 aus
}
```

Nudge definiert 10 bekannte Effekte:
- `LLM`: Ruft Sprachmodellvervollständigungen oder Streaming-Prompts auf.
- `Tool`: Ruft externe MCP- oder registrierte Tools auf.
- `IO`: Standard Ein-/Ausgabe (`print`, `println`).
- `Decision`: Fragt Entscheidungsmodelle über `decide{}` ab.
- `Computer`: Interagiert mit Desktop-Umgebungen und Accessibility-Trees über `computer.*`.
- `Workspace`: Isolierte Lese- und Schreiboperationen im Sandboxed-Workspace.
- `WorkspaceDelete`: Zerstörerisches Löschen von Dateien/Verzeichnissen im Workspace.
- `Store`: Persistente Key-Value-Speicheroperationen über `store.*`.
- `UI`: Erstellt oder dispatcht Ereignisse an grafische UI-Elemente.
- `Auth`: Greift auf authentifizierte Provider-Ressourcen zu (`Auth(account)`).

Funktionen ohne `uses`-Klausel sind nachweislich rein (`pure`).

### Isolierter Workspace & Persistenter Speicher

```nudge
record Config {
    theme: string,
    max_entries: int
}

fn update_workspace_file(path: string, new_content: string) -> bool uses Workspace, Store {
    if workspace.exists(path) {
        workspace.write(path, new_content)   // Atomares Schreiben: Staging, Fsync, Rename
    }
    store.set("app_config", Config { theme: "dark", max_entries: 100 })
    true
}
```

- **Sandboxed Workspace (`workspace.*`):** Alle Dateioperationen sind auf das Workspace-Verzeichnis beschränkt. Pfad-Traversierungen (`../`) und Symlink-Ausbrüche werden blockiert.
- **Trennung der Löschberechtigung:** Das Löschen von Dateien oder Verzeichnissen erfordert den dedizierten `WorkspaceDelete`-Effekt.
- **Persistenter Key-Value-Speicher (`store.*`):** Streng typisierter Speicher in `<workspace>/.nudge/store.json`, der über Prozessneustarts hinweg erhalten bleibt.

### Lokale UI

Erstelle native Desktop-Oberflächen und Genehmigungsstationen direkt in Nudge:

```nudge
type ReviewState = { prompt: string, approved: bool }

view ReviewCard(state: ReviewState) -> View {
    column(padding = 16, gap = 12) [
        text("Agent Review Station", size = 18, bold = true),
        text("Anfrage: " + state.prompt),
        row(gap = 8) [
            button("Genehmigen", on_click = approve_action),
            button("Ablehnen", on_click = reject_action)
        ]
    ]
}

agent ReviewStation {
    state {
        prompt: string = "Deploy-Skript ausführen",
        approved: bool = false
    }

    fn approve_action() uses UI, IO {
        state.approved = true
        io.println("Aktion genehmigt")
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

- **Dual-Mode-Ausführung:** Headless-Tests im Speicher (0$ in CI) oder natives Betriebssystem-Fenster über `winit` und `softbuffer`.

## Umgebungsvariablen

| Variable | Gültigkeit | Zweck | Standard |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | beide | Aktiver LLM-Provider (`fake`, `openai`, `anthropic`, `gemini`, etc.) | `fake` |
| `NUDGE_VM` | nativ | Bytecode-VM umschalten (`1` = VM Standard, `0` = AST-Interpreter) | `1` |
| `NUDGE_BUDGET` | beide | Laufweites USD-Budgetlimit für Ausführungen | Keine |
| `NUDGE_TRACE` | beide | Zielpfad für NTF JSONL Ausführungstrace | `trace.jsonl` |
| `NUDGE_REPLAY` | beide | Tracepfad für deterministisches Offline-Replay | Keine |
| `NUDGE_TRACE_REDACT` | nativ | Automatische Maskierung von Geheimnissen und PII (`1` oder `0`) | `1` (Aktiv) |
| `NUDGE_WORKSPACE_DIR` | nativ | Wurzelverzeichnis für Workspace-Operationen | Aktuelles Verzeichnis |
| `NUDGE_STORE_PATH` | nativ | Speicherdatei für persistente Key-Value-Daten | `<workspace>/.nudge/store.json` |

## Lizenz

Nudge steht unter der **Apache-2.0** Lizenz.
