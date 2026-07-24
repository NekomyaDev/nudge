# Nudge — Roadmap

Design v1.16 is frozen ([design.md](design.md)). This file is the forward
plan; shipped history lives at the bottom in condensed form.

## Where we are

**v1.1 series shipped** — real providers, binary distribution, VS Code
Marketplace, LSP depth, real MCP transport. The toolchain is real end to end:
write → compile → run against live LLMs → trace → replay in CI → debug in the
editor.

## Now — community unblock (no compiler work)

- crates.io + PyPI accounts → flip `CARGO_PUBLISH_ENABLED` / `PYPI_PUBLISH_ENABLED`
- Reddit / Show HN / LinkedIn announcement cadence
- `good first issue` triage for first external contributors

## v1.2 — Depth (make the core airtight)

- **Spanned AST** — every node carries its byte span → check diagnostics point
  at the exact site; LSP underline-precision
- **SSE/HTTP MCP transport** — remote servers, not just local stdio
- **TS runtime parity** — provider adapter + agents/`par` pools on async codegen
- **Provider breadth** — Anthropic + Mistral, real-provider streaming (SSE)

## v1.3 — The standard play (category-defining)

- **Nudge Trace Format (NTF) as an open standard** — spec page + conformance
  suite; "OTel for LLM agents". Bridges that export LangChain / LangGraph /
  CrewAI runs into NTF, so the whole ecosystem can diff, replay, and CI on
  our format
- **`nudgec trace-diff`** — agent regression testing as a CI primitive:
  "did this prompt change make the agent worse?" answered mechanically
- **Trace viewer** — local web UI over traces: timeline of calls, retries,
  budget walls, par branches. Chrome DevTools for agents; the no-lock-in
  answer to hosted agent observability
- **Web playground** — WASM `nudgec` on GitHub Pages: try in the browser,
  see the trace. The top of the star funnel

## v1.4 — Trust (safety & rigor)

- **Property-based agent tests** — `test ... for_all q in gen { ... }`:
  fuzz agents against injection and garbage input, shrink failing cases
- **Published eval benchmark** — same agent written in Nudge vs popular
  frameworks: lines, cost, failure modes; reproducible repo + blog post
- **Stdlib** — `std/http`, `std/fs`, `std/jsonl`, `std/text` as typed tools
  with declared effects
- **RFC process** — `docs/rfc/`: design decisions become public proposals;
  the governance signal of a serious language

## v2.0 — The agent runtime (the big thesis)

Agent frameworks are libraries; **Nudge is the language + runtime**.

- **Distributed trace store** — traces stream to a local daemon / OTLP
  collector; regression suites run against history
- **A2A serving** — v1.0 exports agent cards; v2.0 serves them: a compiled
  Nudge agent is a network-addressable A2A peer out of the box
- **Cross-run learning** — budgets, routes and retries tuned from historical
  traces (the v1 trace freeze exists exactly for this)
- **Self-hosting milestone** — parts of the runtime rewritten in Nudge
- **Stability** — language spec freeze, semver compiler, deprecation policy

## Non-goals (staying honest)

- No hosted SaaS, no token, no "AI cloud" — Nudge is an MIT toolchain, forever
- No framework lock-in: emitted Python/TS stays readable and ejectable

---

## Shipped (condensed history)

| Version | Delivered | Tests |
|---|---|---|
| v0.1 | Lexer → parser → checker → effect inference → Python codegen → trace + replay → budget walls → 29-line self-testing research agent | 52 |
| v0.2 | Hybrid replay, streaming with early-abort schema validation, checkpoint/resume | 69 |
| v0.3 | Reducer state (`\| merge`), multi-server MCP registry, TypeScript backend, OTel span export | 80 |
| v0.4 | `nudgec cost` static report, `route{ when … otherwise }` model routing | 88 |
| v1.0 | Frozen v1 trace schema + `trace-check`, A2A agent-card export, `nudgec lsp` | 97 |
| v1.1a | Real providers: one stdlib-only OpenAI-compatible adapter (openai/gemini/groq/ollama), real tokens + priced cost in trace, CI smoke against live Groq | 98 |
| v1.1b | Tag-driven release workflow → 4 platform binaries attached to GitHub Releases | 98 |
| v1.1c | VS Code extension on the Marketplace (`Nekomya.nudge-lang`) | 98 |
| v1.1d | LSP hover/definition/completion; real MCP stdio transport | 103 |

## Conventions

- Design changes require an amendment PR against `docs/design.md` first.
- Every diagnostic code needs a conformance fixture before it ships.
