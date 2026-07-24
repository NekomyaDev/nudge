<p align="center">
  <img src="assets/logo.svg" width="128" height="128" alt="Nudge logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Don't parse your agents. Nudge them.</strong><br>
  A typed, replayable, budget-aware programming language for LLM agents — compiles to Python & TypeScript.
</p>

<p align="center">
  <img alt="release" src="https://img.shields.io/badge/release-v1.0.0-brightgreen">
  <img alt="tests" src="https://img.shields.io/badge/tests-97%20green-brightgreen">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-blue">
  <img alt="compiler" src="https://img.shields.io/badge/compiler-Rust%20%C2%B7%20zero%20deps-red">
  <img alt="target" src="https://img.shields.io/badge/target-Python%20%7C%20TypeScript-green">
</p>

---

## Why Nudge exists

Production agents in 2026 are still held together with Python glue: prompt chains parsed by hand, tool calls wrapped in try/except, no replay, no cost control, no regression tests. Libraries patch symptoms. **Nudge fixes the layer where the problem actually lives: the language.**

Nudge is an open-source **AI agent programming language** with **deterministic replay** (every run emits a trace; every trace replays as a test at zero token cost), **LLM cost control** (compile-time budgets and a static cost report), typed tool use over **MCP (Model Context Protocol)**, **A2A agent-card export**, an **LSP language server** for editor diagnostics, and **OpenTelemetry** span export — one `.ndg` file in, production-grade agent infrastructure out.

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

## Guarantees at a glance

- **Typed LLM calls** — output schema is a language type; violations trigger automatic repair, never reach your code.
- **Effect system** — pure / `LLM` / `Tool` / `IO` effects inferred and shown in signatures.
- **Deterministic replay** — full, hybrid, and live modes; traces are git-friendly JSONL.
- **Budget contracts** — per-call and per-run USD ceilings with static estimation.
- **Checkpointed agent state** — crash, then `nudge resume` from the last checkpoint.
- **Native parallelism** — `par map`, `par race`, `par all` with compile-time race safety.
- **MCP & Python interop** — consume MCP servers as typed tools; escape to any pip package.

## Install

- **Prebuilt binaries** — every `v*` tag builds `nudgec` for Linux, macOS (x86_64 + Apple Silicon), and Windows and attaches the archives (.tar.gz for Linux/macOS, .zip for Windows) to the [GitHub Release](https://github.com/NekomyaDev/nudge/releases). Extract, add to `PATH`, done.
- **From source** — `cargo build --release` → `target/release/nudgec`. Zero dependencies, builds in seconds.
- **Runtime** — `export PYTHONPATH=$PWD/runtime` (emitted code imports `nudge_runtime`). crates.io / PyPI packages land when the registry secrets are enabled.
- **VS Code** — grab `nudge-lang-1.0.0.vsix` from the [release](https://github.com/NekomyaDev/nudge/releases) → `code --install-extension nudge-lang-1.0.0.vsix`. Syntax highlighting, snippets, and diagnostics powered by `nudgec lsp` (source in [`editors/vscode/`](editors/vscode/)).

## Quickstart

```sh
cargo build                              # the nudgec compiler
export PYTHONPATH=$PWD/runtime           # emitted code imports nudge_runtime (absolute: survives cd)

nudgec check examples/research_agent.ndg # type + effect verification
nudgec cost examples/research_agent.ndg  # static cost report (fake pricing)
nudgec a2a examples/research_agent.ndg   # export an A2A agent card to out/
nudgec build examples/research_agent.ndg # emit Python to out/
cd examples && nudgec test research_agent.ndg   # replay the committed trace — zero tokens
```

Everything runs against the deterministic fake provider by default: no API key, no token spend. See [examples/README.md](examples/README.md) for the full walkthrough (live runs, full replay, budget walls).

## Status

**v1.0 — roadmap complete.** v0.1 MVP, v0.2, v0.3, v0.4, and v1.0 all shipped. The language design is frozen ([docs/design.md](docs/design.md), v1.15) and the full 14-day MVP plan is done ([docs/roadmap.md](docs/roadmap.md)): lexer → parser → **type checker** (E0101–E0202) → **effect inference** (E0301/E0302) → Python codegen → **trace store + replay** → **budget enforcement + parallel scheduler** → **self-testing research agent**. The v0.1 acceptance criteria all pass: the agent is 29 lines, does zero manual JSON parsing, and its replay test passes at zero token cost (97 tests green). **v0.2a hybrid replay landed:** `NUDGE_REPLAY_MODE=llm` replays the LLM from a trace while tools run live; traces now carry `tool.call` records. **v0.2b streaming landed:** `stream let` streams LLM output through an incremental schema validator that aborts unsatisfiable prefixes early (design §4.5). **v0.2c checkpoint/resume landed:** `agent`/`state` blocks checkpoint every state write, and `nudge resume <run_id>` continues a crashed run from its last checkpoint (design §7). **v0.3a reducer state landed:** `l | merge r` joins dicts (union) and lists (append-dedup) for CRDT-style state writes (design §7). **v0.3b–d landed:** multi-server MCP routing via the `NUDGE_MCP_SERVERS` registry (design §8), a TypeScript backend (`nudgec build-ts` + `runtime/nudge_runtime.ts`), and OTel span export (`NUDGE_OTEL`). **v0.4 landed:** `nudgec cost` reports llm call sites statically at flat fake pricing (design §13), and `route{ cheap: "m1" when cond, strong: "m2" otherwise }` picks a model per call, recording the chosen label in the trace (design §4.4). **v1.0 landed:** the v1 trace schema is frozen with a `nudgec trace-check` validator (design §6), `nudgec a2a` exports A2A agent cards (design §9), and `nudgec lsp` serves the Language Server Protocol over stdio for editor diagnostics (design §10). **v1.1 adds distribution + editor support:** prebuilt binaries per tag (release workflow), real providers (design §4.6), and a VS Code extension (`editors/vscode/`, `.vsix` on the release page). **v1.1a detail:** the same `.ndg` runs against OpenAI/Gemini/Groq/Ollama through one dependency-free adapter (design §4.6) — free tiers and local models work at $0.

## The name

*Nudge* — a small, intentional push. That is what a well-typed prompt really is: you don't command an LLM and parse whatever comes back, you nudge it into a schema and let the language enforce the rest. Files use the `.ndg` extension.

Documentation is in English; Simplified Chinese docs are planned after v0.1.

## Documentation

- [Language design](docs/design.md) — types, effects, replay, budgets, compiler architecture
- [Roadmap](docs/roadmap.md) — MVP plan and v0.1→v1.0 milestones
- [Examples](examples/) — the self-testing research agent

## License

MIT — see [LICENSE](LICENSE).
