<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Don't parse your agents. Nudge them.</strong><br>
  A typed, replayable, budget-aware programming language for LLM agents.<br>
  Compiles to Python & TypeScript.
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.2.0-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Proprietary-red">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Python%20%7C%20TypeScript-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
</p>

---

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.zh-CN.md">中文</a>
</p>

---

## Why Nudge?

Production agents are still held together with glue code: prompt chains parsed by hand, tool calls wrapped in try/except, no replay, no cost control, no regression tests. Libraries patch symptoms. **Nudge fixes the layer where the problem actually lives: the language.**

| Pain | Libraries | Nudge |
|---|---|---|
| Untyped LLM output | validate at runtime | schema is a type — proven at compile time |
| Hidden side effects | invisible | `uses LLM, Tool, IO` in every signature |
| No regression testing | record/replay bolted on | every run emits a trace; every trace is a test |
| Cost surprises | dashboards after the fact | budget is a contract, enforced by compiler + runtime |
| Async fan-out spaghetti | manual asyncio | `par map / race / all`, race safety proven |

## A Taste of Nudge

<p align="center">
  <img src="https://via.placeholder.com/800x400/1a1a2e/00d4ff?text=Nudge+Demo+GIF" alt="Nudge Demo" width="600">
</p>

```
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

The compiler proves the schema matches, infers effects, and computes a static cost bound. The runtime records every call to a content-addressed trace you can diff, commit, and replay.

## Features

<div align="center">

| Feature | Description |
|:---:|:---|
| **Typed LLM Calls** | Output schema is a language type; violations trigger automatic repair |
| **Effect System** | Pure / `LLM` / `Tool` / `IO` effects inferred and shown in signatures |
| **Deterministic Replay** | Full, hybrid, and live modes; traces are git-friendly JSONL |
| **Budget Contracts** | Per-call, per-run, and per-repair USD ceilings with static estimation |
| **Checkpoint Resume** | Crash, then `nudge resume` from the last checkpoint |
| **Native Parallelism** | `par map`, `par race`, `par all` with compile-time race safety |
| **Prompt Clippy** | Compiler lints your `llm"""` blocks: vague instructions, missing contracts |
| **MCP & Python Interop** | Consume real MCP servers over stdio; escape to any pip package |
| **Real Providers** | OpenAI / Gemini / Groq / MiMo / Mistral / Anthropic / Ollama |
| **Trace Viewer** | Local web UI: timeline, tokens, cost, repairs highlighted |
| **Trace Diff** | Compare two traces: "what changed when I edited the prompt?" |
| **A2A & LSP & OTel** | Built in, not bolted on |

</div>

## Quick Start

### Install

**One-Line Install (Recommended):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell as Admin)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Package Managers:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

**GUI Installers (Double-Click):**

- **Windows:** Download [`install.bat`](https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/install.bat) and double-click
- **macOS:** Download [`install.command`](https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/install.command) and double-click

**Manual Install:**

Download from [Releases](https://github.com/NekomyaDev/nudge/releases) page:

| Platform | File |
|:---|:---|
| Linux x86_64 | `nudgec-v1.2.0-linux-x86_64.tar.gz` |
| macOS x86_64 | `nudgec-v1.2.0-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `nudgec-v1.2.0-macos-aarch64.tar.gz` |
| Windows x86_64 | `nudgec-v1.2.0-windows-x86_64.zip` |

```sh
# Linux/macOS
tar xzf nudgec-*.tar.gz
chmod +x nudgec
sudo mv nudgec /usr/local/bin/

# Windows
# Extract zip and add to PATH
```

### Your First Nudge Program

```sh
# Create a program
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Create a greeting for {name}. Return message and timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

test "greet works on recorded trace" {
    let t = replay("traces/greet.jsonl")
    assert t.output.message != ""
}
EOF

# Type check
nudgec check hello.ndg

# Compile to Python
nudgec build hello.ndg

# Run (no API key needed - uses fake provider)
export PYTHONPATH=$PWD/runtime
python3 out/hello.py

# Run tests (zero tokens)
nudgec test hello.ndg
```

Everything runs against a deterministic fake provider by default: **no API key, no token spend.**

## Backend Parity

| Capability | Python | TypeScript |
|:---|:---:|:---:|
| Typed calls, schema validation, repair | ✅ | ✅ |
| Traces, replay, budget walls | ✅ | ✅ |
| `par map/all/race` + branch labels | ✅ | ✅ |
| Streaming (`stream let`) | ✅ | ✅ |
| Real providers | ✅ | ⬜ |
| MCP tools, checkpoint/resume, OTel | ✅ | ⬜ |

## VS Code Extension

Install the [Nudge Language](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) extension for:

- Syntax highlighting
- Code snippets
- Real-time diagnostics via `nudgec lsp`
- Hover information
- Go to definition

## Privacy Note

Traces record prompts, model outputs, and tool results verbatim — they can contain secrets or personal data. Treat trace files as sensitive artifacts; a redaction hook is on the roadmap.

## License

Proprietary — see [LICENSE](LICENSE) and [LICENSE-BINARY](LICENSE-BINARY).

Nudge is free to use but closed source. For licensing inquiries, contact [@NekomyaDev](https://github.com/NekomyaDev).

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
