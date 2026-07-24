# Nudge — Roadmap

Design v1.16 is frozen ([design.md](design.md)). This file tracks implementation.

## MVP — v0.1 (2-week plan)

| Days | Deliverable | Status |
|---|---|---|
| 1–3 | Lexer + parser + AST; end-to-end `hello llm` (no schema, fixed model) | ✅ done — `nudgec build examples/hello_llm.ndg` → Python, fake provider, trace record (22 tests) |
| 4–6 | Type checker core (records, lists, refinements); runtime schema validation + repair loop | ✅ done — E0101/E0201/E0202, `nudgec check`, `rt.schema` + repair loop; research_agent runs e2e (37 tests) |
| 7–8 | Effect inference + signature verification | ✅ done — E0301/E0302, transitive fixpoint over the call graph, test blocks exempt (44 tests) |
| 9–10 | Trace store + `replay` mode + `test` blocks | ✅ done — `fn.return` records, `Trace`/`replay`, `NUDGE_REPLAY` full replay, `nudgec test` runs `nudge_test_*` (49 tests) |
| 11–12 | `par map` + budget enforcement + Python codegen polish | ✅ done — thread-pool `par_map`/`par_all`/`par_race` (order-preserving), flat $0.001 fake pricing, `NUDGE_BUDGET` run budget shared across `par` branches, per-call walls, E0501 non-USD budgets (56 tests) |
| 13–14 | `examples/research_agent.ndg` end-to-end + docs + `v0.1` tag | ✅ done — 29 lines, zero JSON parsing, committed demo trace replays green (5 llm.calls, $0.005), [examples/README.md](../examples/README.md) walkthrough |

**v0.1 acceptance:** the research agent (i) under 30 lines ✅ 29, (ii) zero manual JSON parsing ✅, (iii) replay test passing at zero token cost ✅. **MVP COMPLETE.**

## After MVP

- **v1.1a** — real provider adapter ✅ done — one stdlib-only OpenAI-compatible HTTP adapter (`openai`/`gemini`/`groq`/`ollama`), provider prefix in the model string or `NUDGE_PROVIDER`, real usage tokens + priced cost in the trace (free/local models $0), mock-server e2e + secret-gated `provider-smoke` workflow against the Gemini free tier; TS adapter deferred to async codegen
- **v1.1b** — distribution ✅ done — tag-driven `release` workflow builds prebuilt nudgec binaries (linux x86_64, macOS x86_64 + aarch64, windows x86_64) and attaches them to the GitHub Release; crates.io / PyPI publish jobs ship gated behind repo variables (`CARGO_PUBLISH_ENABLED` / `PYPI_PUBLISH_ENABLED`) until the registry accounts + tokens exist
- **v1.1c** — VS Code extension ✅ done — `editors/vscode/` ships a marketplace-ready package: TextMate grammar (keywords, `llm"""` prompts + interpolation, USD literals, `@constraint`s), language configuration, 5 snippets (`llm`/`type`/`par`/`fn`/`test`), and diagnostics wired to `nudgec lsp` over stdio via `vscode-languageclient`; published on the VS Code Marketplace as **Nudge Language** (`Nekomya.nudge-lang`); `.vsix` also attached to the v1.0.1 release
- **v1.1d** — LSP depth + real MCP transport ✅ done — hover/definition/completion in `nudgec lsp` from a per-document symbol index; `NUDGE_MCP_SERVERS` entries with `command` get a real stdio JSON-RPC transport (initialize → tools/call, real outputs in the trace, fail-fast errors) with mock-server e2e tests

- **v0.2a** — hybrid replay ✅ done — `NUDGE_REPLAY_MODE=llm` (LLM from trace, tools live), `tool.call` trace records, full-replay tool mocking, serialized trace emission (59 tests)
- **v0.2b** — streaming (`stream let`, incremental schema validation) ✅ done — `rt.llm_stream` + `_PrefixValidator` early-abort on unsatisfiable prefixes, `streamed`/`chunks`/`early_abort` trace fields, stream-replay parity (64 tests)
- **v0.2c** — checkpoint/resume (`agent`/`state` blocks, `nudge resume`) ✅ done — `rt.AgentState` checkpoints every state write to `.nudge/runs/<run_id>/`, resume replays the recorded prefix then goes live (replayed state writes suppressed via the `writes` counter), E0701 for stray state writes (69 tests)
- **v0.3a** — reducer state (`| merge`) ✅ done — `l | merge r` infix (contextual `merge`) lowers to `rt.merge`: dict union (right wins), list append-dedup; checker requires two records or two lists; composes with checkpoints + resume (74 tests)
- **v0.3b** — multi-server MCP ✅ done — `impl: mcp("server").…` routes through the `NUDGE_MCP_SERVERS` JSON registry (unknown server fails fast), `tool.call` records gain a `server` field; real MCP transport stays post-MVP (80 tests)
- **v0.3c** — TypeScript backend ✅ done — `nudgec build-ts` emits `out/<name>.ts` + `runtime/nudge_runtime.ts` ships (fake provider, replay, budget, merge); agents/streaming/par pools deferred with warning comments (80 tests)
- **v0.3d** — OTel span export ✅ done — `NUDGE_OTEL=<path>` writes every trace record as an OTel-shaped JSON-lines span; OTLP transport post-MVP (80 tests)
- **v0.4** — `nudge cost` static cost report, user-defined routing ✅ done — `nudgec cost` counts llm call sites at flat $0.001 fake pricing (`retry: N with repair` multiplies the worst case, par-map sites marked runtime-dependent); `route{ label: "model" when cond, … otherwise }` picks a model top-down (E0702 without `otherwise`, E0201 non-bool `when`, contextual `route`), chosen label recorded as the additive `route` trace field; TS backend mirrors it (88 tests)
- **v1.0** — A2A export, LSP, frozen trace format ✅ done — v1 trace schema frozen + `nudgec trace-check` validator (E0601 on unknown versions); `nudgec a2a` emits A2A agent cards (skills from fns, effects as tags); `nudgec lsp` serves stdio LSP — full sync + publishDiagnostics via the real pipeline, dependency-free JSON-RPC (97 tests)
- **Docs i18n** — Simplified Chinese documentation after v0.1; compiler diagnostics are localization-ready from day one

## Conventions

- Design changes require an amendment PR against `docs/design.md` first.
- Every diagnostic code needs a conformance fixture before it ships.

## Beyond v1.1 — the path to v2.0

Theme per release. Each line is a milestone, not a promise of dates; the
community milestones (registry accounts, Show HN) unblock in parallel.

### v1.2 — Depth (make the core airtight)
- **Spanned AST** — every node carries its byte span → check diagnostics point
  at the exact site (no more file-start diagnostics), LSP underline-precision
- **SSE/HTTP MCP transport** — remote servers, not just local stdio
- **TS runtime parity** — provider adapter + agents/`par` pools on async codegen
- **crates.io + PyPI live** — `cargo install nudgec`, `pip install nudge-runtime`
  (gated jobs already shipped in v1.1b; needs the registry tokens)
- Community: Show HN, first external contributors, good-first-issue triage

### v1.3 — Ecosystem (meet users where they are)
- **Stdlib** — `std/http`, `std/fs`, `std/jsonl`, `std/text` as typed tools
  with declared effects, so real programs stop escaping to raw Python
- **Package story** — `use` across files grows into a module path +
  lockfile-free workspace layout (Nudge stays single-binary, no registry yet)
- **JetBrains + Zed grammars** — the TextMate grammar ports cheaply; LSP is
  already editor-agnostic
- **Web playground** — WASM-compiled `nudgec` on GitHub Pages: type in the
  browser, see the trace. Zero-install trial = the top funnel for stars
- **Provider breadth** — Anthropic + Mistral adapters, streaming for real
  providers (SSE), per-provider pricing table refresh

### v2.0 — The agent runtime (the big thesis)
The bet: agent frameworks are libraries; **Nudge is the language + runtime**.
- **Distributed trace store** — traces stream to a local daemon/OTLP
  collector; `nudgec trace-check` grows into diff + regression suites
  ("did this prompt change make the agent worse?" as CI)
- **A2A serving** — v1.0 exports agent cards; v2.0 serves them: a compiled
  Nudge agent is a network-addressable A2A peer out of the box
- **Cross-run learning primitives** — budgets, routes and retries tuned from
  historical traces (the trace format v1 was frozen exactly for this)
- **Self-hosting milestone** — parts of the runtime rewritten in Nudge
- Stability: language spec freeze, semver on the compiler, deprecation policy

### Non-goals (staying honest)
- No hosted SaaS, no token, no "AI cloud" — Nudge is a toolchain, MIT, forever
- No framework lock-in: emitted Python/TS stays readable and ejectable

