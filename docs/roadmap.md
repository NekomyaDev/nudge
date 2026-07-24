# Nudge — Roadmap

Design v1.11 is frozen ([design.md](design.md)). This file tracks implementation.

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

- **v0.2a** — hybrid replay ✅ done — `NUDGE_REPLAY_MODE=llm` (LLM from trace, tools live), `tool.call` trace records, full-replay tool mocking, serialized trace emission (59 tests)
- **v0.2b** — streaming (`stream let`, incremental schema validation) ✅ done — `rt.llm_stream` + `_PrefixValidator` early-abort on unsatisfiable prefixes, `streamed`/`chunks`/`early_abort` trace fields, stream-replay parity (64 tests)
- **v0.2c** — checkpoint/resume (`agent`/`state` blocks, `nudge resume`) ✅ done — `rt.AgentState` checkpoints every state write to `.nudge/runs/<run_id>/`, resume replays the recorded prefix then goes live (replayed state writes suppressed via the `writes` counter), E0701 for stray state writes (69 tests)
- **v0.3a** — reducer state (`| merge`) ✅ done — `l | merge r` infix (contextual `merge`) lowers to `rt.merge`: dict union (right wins), list append-dedup; checker requires two records or two lists; composes with checkpoints + resume (74 tests)
- **v0.3b** — multi-server MCP ✅ done — `impl: mcp("server").…` routes through the `NUDGE_MCP_SERVERS` JSON registry (unknown server fails fast), `tool.call` records gain a `server` field; real MCP transport stays post-MVP (80 tests)
- **v0.3c** — TypeScript backend ✅ done — `nudgec build-ts` emits `out/<name>.ts` + `runtime/nudge_runtime.ts` ships (fake provider, replay, budget, merge); agents/streaming/par pools deferred with warning comments (80 tests)
- **v0.3d** — OTel span export ✅ done — `NUDGE_OTEL=<path>` writes every trace record as an OTel-shaped JSON-lines span; OTLP transport post-MVP (80 tests)
- **v0.4** — `nudge cost` static cost report, user-defined routing
- **v1.0** — A2A export, LSP (VS Code), frozen trace format
- **Docs i18n** — Simplified Chinese documentation after v0.1; compiler diagnostics are localization-ready from day one

## Conventions

- Design changes require an amendment PR against `docs/design.md` first.
- Every diagnostic code needs a conformance fixture before it ships.
