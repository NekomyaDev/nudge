<p align="center">
  <img src="assets/logo.svg" width="128" height="128" alt="Nudge logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Don't parse your agents. Nudge them.</strong><br>
  A typed, replayable, budget-aware programming language for LLM agents — compiles to Python & TypeScript.
</p>

<p align="center">
  <img alt="release" src="https://img.shields.io/badge/release-v1.2.0-brightgreen">
  <img alt="license" src="https://img.shields.io/badge/license-Proprietary-red">
  <img alt="compiler" src="https://img.shields.io/badge/compiler-Rust%20%C2%B7%20zero%20deps-red">
  <img alt="target" src="https://img.shields.io/badge/target-Python%20%7C%20TypeScript-green">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code extension" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
</p>

---

Production agents are still held together with glue code: prompt chains parsed by hand, tool calls wrapped in try/except, no replay, no cost control, no regression tests. Libraries patch symptoms. **Nudge fixes the layer where the problem actually lives: the language.**

| Pain | Libraries | Nudge |
|---|---|---|
| Untyped LLM output | validate at runtime | schema is a type — proven at compile time |
| Hidden side effects | invisible | `uses LLM, Tool, IO` in every signature |
| No regression testing | record/replay bolted on | every run emits a trace; every trace is a test |
| Cost surprises | dashboards after the fact | budget is a contract, enforced by compiler + runtime |
| Async fan-out spaghetti | manual asyncio | `par map / race / all`, race safety proven |

## A taste

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

## What you get

- **Typed LLM calls** — output schema is a language type; violations trigger automatic repair, never reach your code
- **Effect system** — pure / `LLM` / `Tool` / `IO` effects inferred and shown in signatures
- **Deterministic replay** — full, hybrid, and live modes; traces are git-friendly JSONL
- **Budget contracts** — per-call, per-run, and per-repair USD ceilings (`NUDGE_REPAIR_BUDGET`) with static estimation (`nudgec cost`)
- **Checkpointed agent state** — crash, then `nudge resume` from the last checkpoint
- **Native parallelism** — `par map`, `par race`, `par all` with compile-time race safety
- **Prompt Clippy** — the compiler lints your `llm"""` blocks: vague instructions, missing output contracts, overlong prompts
- **MCP & Python interop** — consume real MCP servers over stdio as typed tools; escape to any pip package
- **Real providers** — one stdlib-only adapter for OpenAI / Gemini / Groq / MiMo / Mistral / Anthropic / Ollama; free tiers and local models work at $0
- **Trace viewer** — `nudgec trace-view <trace.jsonl>` opens a local web UI over any run: timeline, tokens, cost, repairs highlighted, `par` lanes color-coded (NTF v1.1 `branch` field)
- **Trace diff** — `nudgec trace-diff a.jsonl b.jsonl` answers "what changed when I edited the prompt?": totals and per-record deltas
- **A2A agent-card export, LSP, OpenTelemetry** — built in, not bolted on

### Backend parity

| Capability | Python backend | TypeScript backend |
|---|---|---|
| Typed calls, schema validation, repair | ✅ | ✅ |
| Traces, replay, budget walls | ✅ | ✅ |
| `par map/all/race` + branch labels (NTF v1.1) | ✅ thread pool | ✅ sequential (async codegen planned) |
| Streaming (`stream let`) | ✅ live SSE + early-abort repair | ✅ fake parity (no live providers) |
| Real providers (OpenAI/Gemini/Groq/MiMo/Mistral/Anthropic/Ollama) | ✅ | ⬜ routes to Python |
| MCP tools, checkpoint/resume, OTel | ✅ | ⬜ |

The TypeScript backend is a deliberately scoped subset today; parity work is tracked in [docs/roadmap.md](docs/roadmap.md).

> **Privacy note:** traces record prompts, model outputs, and tool results verbatim — they can contain secrets or personal data. Treat trace files as sensitive artifacts; a redaction hook is on the roadmap.

## Quickstart

```sh
# Install Nudge
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Create a simple Nudge program
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

# Check the program
nudgec check hello.ndg

# Build and run
nudgec build hello.ndg
export PYTHONPATH=$PWD/runtime
python3 out/hello.py

# Run tests (zero tokens)
nudgec test hello.ndg
```

Everything runs against a deterministic fake provider by default: **no API key, no token spend.**

## Install

### Quick Install (Recommended)

**Linux/macOS:**
```sh
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

### Homebrew (macOS/Linux)

```sh
brew install NekomyaDev/nudge/nudge
```

### Docker

```sh
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

### Manual Install

Download the latest prebuilt binary for your platform from the [Releases](https://github.com/NekomyaDev/nudge/releases) page:

- **Linux x86_64**: `nudgec-v1.2.0-linux-x86_64.tar.gz`
- **macOS x86_64**: `nudgec-v1.2.0-macos-x86_64.tar.gz`
- **macOS Apple Silicon**: `nudgec-v1.2.0-macos-aarch64.tar.gz`
- **Windows x86_64**: `nudgec-v1.2.0-windows-x86_64.zip`

After downloading:

```sh
# Linux/macOS
tar xzf nudgec-*.tar.gz
chmod +x nudgec
sudo mv nudgec /usr/local/bin/

# Windows (PowerShell)
Expand-Archive nudgec-*.zip
Move-Item nudgec.exe C:\Windows\System32\
```

### VS Code Extension

Install the [Nudge Language](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) extension from the VS Code Marketplace for syntax highlighting, snippets, and diagnostics.

## Documentation

Documentation is available in the private source repository. For access, contact [@NekomyaDev](https://github.com/NekomyaDev).

## Contributing

For contributions, please contact [@NekomyaDev](https://github.com/NekomyaDev) to access the private source repository.

## The name

*Nudge* — a small, intentional push. You don't command an LLM and parse whatever comes back; you nudge it into a schema and let the language enforce the rest. Files use the `.ndg` extension.

## License

Proprietary — see [LICENSE](LICENSE) and [LICENSE-BINARY](LICENSE-BINARY). Nudge is free to use but closed source.
