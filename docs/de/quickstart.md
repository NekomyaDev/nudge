# Nudge Schnellstartanleitung (5 Minuten)

Willkommen bei **Nudge**, der typsicheren, budgetbewussten und deterministisch wiederholbaren Programmiersprache für autonome KI-Agenten.

---

## 1. Installation

Installieren Sie den Nudge-Compiler (`nudgec`) über das offizielle Installationsskript:

```bash
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash
```

Überprüfen Sie die erfolgreiche Installation:

```bash
nudgec --version
```

---

## 2. Ihr erstes Agentenprogramm (`hello.ndg`)

Erstellen Sie eine Datei namens `hello.ndg`:

```nudge
type UserGreeting = {
    message: string,
    confidence: float @range(0, 1),
}

fn greet(name: string) -> UserGreeting uses LLM {
    llm"""Erstelle eine freundliche, professionelle Begrüßung für {name}."""
    with {
        model: "anthropic:sonnet-4.6",
        schema: UserGreeting,
        budget: 0.02 USD,
    }
}

fn main() -> UserGreeting uses LLM {
    greet("Entwickler")
}
```

---

## 3. Typ- und Sicherheitsprüfung

Überprüfen Sie Ihr Programm statisch vor der Ausführung:

```bash
nudgec check hello.ndg
```

Der Compiler stellt sicher, dass:
- Alle Effekte (`uses LLM`) deklariert sind (`E0301`).
- Das Ausgaben-Schema eingehalten wird (`schema: UserGreeting`).
- Das Kostenlimit (`budget`) garantiert nicht überschritten wird.

---

## 4. Ausführung und Deterministischer Replay

Führen Sie das Programm lokal im Replay- oder Live-Modus aus:

```bash
nudgec run hello.ndg
```

Nudge speichert automatisch jeden Schritt in einem standardisierten NTF-Trace (`trace.jsonl`). Sie können denselben Lauf jederzeit mit $0 Token-Kosten deterministisch wiederholen:

```bash
nudgec test hello.ndg
```

---

## 5. Nächste Schritte

- Interaktive Tutorials: `nudgec learn`
- Dokumentationsübersicht: [docs/roadmap.md](../../docs/roadmap.md)
- Architektur & Spezifikation: [docs/architecture.md](../../docs/architecture.md)
