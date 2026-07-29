<p align="center">
  <img src="assets/logo.svg" width="128" height="128" alt="Nudge logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Don't parse your agents. Nudge them.</strong><br>
  A typed, replayable, budget-aware programming language for LLM agents — compiles to Python & TypeScript.
</p>

<p align="center">
  <a href="https://github.com/NekomyaDev/nudge/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/NekomyaDev/nudge/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="release" src="https://img.shields.io/badge/release-v1.2.0--alpha.1-brightgreen">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-blue">
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
- **Real providers** — one stdlib-only adapter for OpenAI / Gemini / Groq / Ollama; free tiers and local models work at $0
- **Trace viewer** — `nudgec trace-view <trace.jsonl>` opens a local web UI over any run: timeline, tokens, cost, repairs highlighted
- **Trace diff** — `nudgec trace-diff a.jsonl b.jsonl` answers "what changed when I edited the prompt?": totals and per-record deltas
- **A2A agent-card export, LSP, OpenTelemetry** — built in, not bolted on

## Quickstart

```sh
cargo build                              # the nudgec compiler
export PYTHONPATH=$PWD/runtime           # emitted code imports nudge_runtime

nudgec check examples/research_agent.ndg # type + effect verification
nudgec cost examples/research_agent.ndg  # static cost report
nudgec build examples/research_agent.ndg # emit Python to out/
cd examples && nudgec test research_agent.ndg   # replay the committed trace — zero tokens
```

Everything runs against a deterministic fake provider by default: **no API key, no token spend.** See [examples/README.md](examples/README.md) for live runs and the full walkthrough.

## Install

- **Prebuilt binaries** — Linux, macOS (x86_64 + Apple Silicon), Windows on the [Releases](https://github.com/NekomyaDev/nudge/releases) page
- **From source** — `cargo build --release` → `target/release/nudgec`. Zero dependencies, builds in seconds
- **VS Code** — [Nudge Language](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) on the Marketplace: highlighting, snippets, and diagnostics via `nudgec lsp`

## Documentation

- [Language design](docs/design.md) — types, effects, replay, budgets, compiler architecture (frozen at v1.24)
- [Strategy: Six Locked Doors](docs/strategy.md) — why Nudge exists, and the order in which it becomes indispensable
- [Roadmap](docs/roadmap.md) — shipped history and what's next (trace viewer, NTF standard, capability-based tool security)
- [Examples](examples/) — the self-testing research agent

## Contributing

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) and the open [`good first issue`s](https://github.com/NekomyaDev/nudge/labels/good%20first%20issue): new example agents, replay conformance tests, provider adapters, and editor support.

## The name

*Nudge* — a small, intentional push. You don't command an LLM and parse whatever comes back; you nudge it into a schema and let the language enforce the rest. Files use the `.ndg` extension.

## License

MIT — see [LICENSE](LICENSE). No SaaS, no token, no lock-in: Nudge is an open toolchain, forever.
