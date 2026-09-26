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
  <img alt="Version" src="https://img.shields.io/badge/version-1.2.1-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
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

## Real-World Examples

See what you can build with Nudge:

| Example | Description | Features |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | Conversational agent with memory | Typed LLM, Replay, Budget |
| [Code Reviewer](examples/code-reviewer/) | Code quality analyzer | Structured output, Scoring |
| [Research Agent](examples/research-agent/) | Multi-source research | Confidence scores, Par map |
| [Data Analyzer](examples/data-analyzer/) | Data pattern recognition | Insights, Recommendations |
| [Translator](examples/translator/) | Multi-language translation | Quality scoring, Parallel |

```sh
# Try any example
cd examples/chatbot
nudgec check chatbot.ndg && nudgec build chatbot.ndg
python3 out/chatbot.py
```

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
| **Nudge CI** | GitHub Action: agent regression testing on every push, $0 |
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

Install the Python runtime that the generated code imports:

```sh
pip install nudge-runtime   # pure stdlib, no dependencies
```

```sh
# Create a program
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Create a greeting for {name}. Return message and timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}
EOF

# Type check
nudgec check hello.ndg

# Compile to Python
nudgec build hello.ndg

# Run (no API key needed - uses fake provider)
python3 out/hello.py
```

Everything runs against a deterministic fake provider by default: **no API key, no token spend.** Prefer building from source? See [Building from Source](#building-from-source).

## Backend Parity

| Capability | Python | TypeScript |
|:---|:---:|:---:|
| Typed calls, schema validation, repair | ✅ | ✅ |
| Traces, replay, budget walls | ✅ | ✅ |
| `par map/all/race` + branch labels | ✅ | ✅ |
| Streaming (`stream let`) | ✅ | ✅ |
| Real providers | ✅ | ⬜ |
| MCP tools, checkpoint/resume, OTel | ✅ | ⬜ |

## MCP Integration

Tools that declare `impl: mcp("server").tool` talk to real MCP servers over
stdio. This section documents the contract as of v1.2.1.

### Declaring and calling MCP tools

```nudge
type Result = { url: string, title: string }

tool web_search(query: string) -> [Result] {
    impl: mcp("search").web_search(query)
    side_effects: none
}

fn main() -> string uses Tool {
    let hits = web_search("nudge lang")
    hits[0].title        # tool results support .field access (v1.2.1)
}
```

The compiler lowers every call to the tool's declared parameters. As of
v1.2.1, arguments are passed to the MCP server as a **named `arguments`
object** (`{"query": "nudge lang"}`), which is what MCP `tools/call`
expects — servers built on FastMCP and similar frameworks reject the older
positional framing.

### Server registry

Live MCP calls resolve servers from the `NUDGE_MCP_SERVERS` environment
variable (JSON):

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
python3 out/hello.py
```

- `command` may be a string (shell-parsed) or an argv list.
- A call to a tool whose server is missing from the registry **fails fast**
  with a clear error.
- A registry entry **without** a `command` returns `[]` for that tool in
  live mode (no transport configured) — but the call is still recorded in
  the trace, so replay works.
- Unknown tool or server errors raise; nothing silently fakes a result.
- An MCP server that reports `isError` raises a `RuntimeError` with the
  server's content.
- Text content that parses as JSON is returned decoded; other content
  types are returned as-is.

### Replay semantics

Full replay (`NUDGE_REPLAY=trace.jsonl`) mocks both llm and tool calls from
the trace. A program that makes **more** calls than the trace holds fails
with `ReplayMismatch` — there is no silent empty-result mocking. Use
`NUDGE_RESUME` to continue past the recorded prefix against a live provider.

## Environment Variables

Everything a compiled Nudge program reads comes from these variables:

| Variable | Purpose |
|:---|:---|
| `NUDGE_PROVIDER` | Provider override (`fake`, `openai`, `anthropic`, …). `fake` synthesizes schema-valid outputs — no API key needed |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | Credentials and endpoint for OpenAI-compatible providers |
| `NUDGE_MCP_SERVERS` | MCP server registry JSON (see above) |
| `NUDGE_TRACE` | Write a JSONL trace to this path while running |
| `NUDGE_REPLAY` | Load a trace and replay it (`NUDGE_REPLAY_MODE=all` for tools+llm, `llm` for llm-only) |
| `NUDGE_RESUME` | Continue a crashed run from its checkpoint, consuming the recorded trace prefix |
| `NUDGE_RUN_ID` | Checkpoint/run directory id (default: `run-<pid>`) |
| `NUDGE_BUDGET` / `NUDGE_REPAIR_BUDGET` | Run-wide USD ceilings enforced across `par` fan-out |
| `NUDGE_OTEL` | OTel endpoint for span export |
| `NUDGE_FAKE_FAIL_FIRST` | Make the fake provider fail the first attempt (k times) — for testing repair loops |
| `NUDGE_ALLOW_FAKE_RESUME` | Explicitly allow `NUDGE_RESUME` to continue on the fake provider |

Notes:

- A program's entry point is `fn main()` with **no parameters** — argv and
  stdin are not read in v1.2.x. External data enters through MCP tools.
- The fake provider is deterministic and schema-aware; it is what makes
  `nudgec test` and the CI examples run without keys.

## Agent CI (GitHub Action)

Regression-test your agents on every push — replays recorded traces, so it
costs **$0** and needs **no API keys**. Add `.github/workflows/agents.yml`:

```yaml
name: agents
on: [push, pull_request]
jobs:
  agents:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: NekomyaDev/nudge@v1
        with:
          files: |
            agents/**/*.ndg
```

Inputs: `files` (required, globs allowed), `version` (pin nudgec),
`check-only` (skip replay), and `trace-diff: "baseline.jsonl candidate.jsonl"`
— a regression gate that fails the build when the candidate trace spends
more, repairs more, or turns a passing call into a failure. CI is where the
trace/replay design pays off: the agent's behavior becomes a testable
artifact, like any other.

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

Apache-2.0 — see [LICENSE](LICENSE).

Nudge is open source: the compiler (`crates/nudgec`), the bytecode VM experiment (`crates/nudge-runtime`), the Python runtime (`runtime/`), and the VS Code extension (`editors/vscode/`) all live in this repository. Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Building from Source

Requires Rust 1.75+ (stable):

```bash
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # compiler binary at target/release/nudgec
cargo test --workspace            # compiler + VM tests
```

The Python runtime is standalone (pure stdlib, no dependencies):

```bash
pip install nudge-runtime         # from PyPI
# or from this repo:
pip install ./runtime
```

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
