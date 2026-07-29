# Nudge — Roadmap

Strategy: [docs/strategy.md](strategy.md) (Six Locked Doors). Design v1.24 is frozen ([design.md](design.md)). This file is the forward
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

## v1.2 — Door 1 close-out: *debuggable* (Depth) — **CLOSED** ✅

Finish the "why did my agent do that?" answer beyond doubt.
- ~~**Spanned AST**~~ ✅ shipped (stages 1+2) — statements and expressions carry byte spans; check errors print as `error[E0201] at 3:5`, LSP diagnostics land on their statement. Sub-expression hover/inline values build on this next
- ~~**Trace viewer**~~ ✅ shipped (v1.2 branch) — `nudgec trace-view <trace.jsonl>`: local web UI over traces: timeline of calls, retries, budget walls, token/cost stats, zero-dep embedded server
  walls, par branches. Chrome DevTools for agents
- ~~**Par branch labels (NTF v1.1)**~~ ✅ shipped — `par map/all/race` lanes carry an additive
  `branch` field in traces; trace-view badges and color-codes each lane
- ~~**DAP groundwork**~~ ✅ shipped — `nudgec debug <trace.jsonl>` speaks DAP over
  stdio: breakpoints on record seqs, step/continue through a run, record
  fields as variables. Live attach lands with the spanned AST
- **SSE/HTTP MCP transport** — design-blocked for now: remote MCP implies
  HTTPS, and TLS cannot be implemented under the zero-dependency constraint;
  needs either a dep allowance or a sidecar story
- ~~**TS runtime parity**~~ ✅ shipped — par helpers with NTF v1.1 branch labels,
  `llmStream` fake streaming, and the un-awaited `Promise.all` leak fixed;
  real providers and streamed repair stay Python-side by design
- ~~**Real streaming**~~ ✅ shipped — `llm_stream` now streams live over SSE
  (OpenAI-compatible + Anthropic Messages); early-abort prefix validation and
  the repair loop work against live streams, usage-based tokens/cost in traces
- ~~**Provider breadth: Anthropic + Mistral**~~ ✅ shipped — Anthropic Messages API
  adapter + OpenAI-compatible Mistral; `NUDGE_PROVIDER=fake` now explicitly overrides
  model prefixes so $0 runs stay $0

## v1.3 — Door 2: *testable* (create the agent-CI category)

- **NTF open standard** — spec page + conformance suite; "OTel for LLM agents".
  Bridges exporting LangChain / LangGraph / CrewAI runs into NTF
- **`nudgec trace-diff`** — agent regression testing as a CI primitive
- **Property-based agent tests** — `test ... for_all ... in gen { ... }`:
  fuzz against injection and garbage input, shrink failing cases
- **`nudge-ci` GitHub Action** (GitHub Marketplace) — agent regression CI in
  any repo; a distribution channel disguised as a feature
- **Web playground** — WASM `nudgec` on GitHub Pages: try in the browser,
  see the trace. The top of the star funnel

## v1.4 — Door 3: *safe* (the moat — flagship engineering) 🛡️

The language thesis ships: **the language where agents are safe to deploy.**
- **Capability-based tool security** — tools as capabilities; per-agent grants
  with attenuation (`fs.read` yes, `fs.write` no); the compiler proves the
  reachable-tool graph, so injected instructions cannot invoke ungranted calls.
  Not a prompt-level plea — a proof
- **Confidence-aware types** — values track `unverified → validated → verified`;
  `refine x until confidence > 0.8`; gradual verification for LLM output
- **Refinement types for cost** — `fn f() -> string costs< 0.05 USD`:
  the checker proves call graphs stay under budget statically
- **Unknown-model pricing policy** — unknown models currently price at $0,
  which can under-report real cost behind the budget wall; add an explicit
  policy (unknown cost marker / budget-active error / user override)
- **MCP deadline & lifecycle** — per-RPC timeouts, hung-server cleanup and
  restart strategy for persistent MCP sessions
- **Trace redaction hooks** — filter secrets/PII before records hit disk
- Security conformance suite + a threat-model document; possibly a short
  industry-track paper

## v1.5 — Doors 4+5: *provable & improving* (enterprise + compounding)

- **Compliance** — `nudgec audit` reports over NTF traces; EU AI Act
  positioning: traces as audit evidence out of the box
- **`optimize` block** — budget-bounded, type-safe search over prompts/models,
  learned from traces (DSPy as a language construct, not a library hack)
- **Cross-run learning** — budgets, routes, retries tuned from history
- **Published eval benchmark** — same agent in Nudge vs popular frameworks:
  lines, cost, failure modes; reproducible
- **Stdlib** (`std/http`, `std/fs`, `std/jsonl`, `std/text`), **RFC process**
  (`docs/rfc/`), **The Nudge Book** + interactive tutorial, **Agent Hub**

## v2.0 — Door 6 + the runtime thesis: *inevitable*

Agent frameworks are libraries; **Nudge is the language + runtime**.
- **Distributed trace store** — traces stream to a local daemon / OTLP
  collector; regression suites run against history
- **A2A serving** — a compiled Nudge agent is a network-addressable A2A peer
  out of the box
- **Self-hosting milestone** — parts of the runtime rewritten in Nudge
- **Stability** — language spec freeze, semver compiler, deprecation policy
- **Session types for conversations** (research track) — typed multi-turn
  protocols between agents and users

## Horizon (authority plays, date-free)

- **Talk circuit** — FOSDEM / RustConf / PyCon proposals
- **Jupyter kernel** — the data-science crowd meets Nudge in a notebook
- **Conformance certification** — "NTF-conformant" badge for third-party tools

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
| v1.2a | Prompt Clippy (design §20): W0001–W0004 lints, fn context, ×N dedupe, LSP surfacing | 113 |

## Conventions

- Design changes require an amendment PR against `docs/design.md` first.
- Every diagnostic code needs a conformance fixture before it ships.