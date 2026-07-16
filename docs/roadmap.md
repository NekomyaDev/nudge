# Nudge — Roadmap

Design v1.3 is frozen ([design.md](design.md)). This file tracks implementation.

## MVP — v0.1 (2-week plan)

| Days | Deliverable | Status |
|---|---|---|
| 1–3 | Lexer + parser + AST; end-to-end `hello llm` (no schema, fixed model) | ✅ done — `nudgec build examples/hello_llm.ndg` → Python, fake provider, trace record (22 tests) |
| 4–6 | Type checker core (records, lists, refinements); runtime schema validation + repair loop | ✅ done — E0101/E0201/E0202, `nudgec check`, `rt.schema` + repair loop; research_agent runs e2e (37 tests) |
| 7–8 | Effect inference + signature verification | ⬜ next |
| 9–10 | Trace store + `replay` mode + `test` blocks | ⬜ |
| 11–12 | `par map` + budget enforcement + Python codegen polish | ⬜ |
| 13–14 | `examples/research_agent.ndg` end-to-end + docs + `v0.1` tag | ⬜ |

**v0.1 acceptance:** the research agent (i) under 30 lines, (ii) zero manual JSON parsing, (iii) replay test passing in CI at zero token cost.

## After MVP

- **v0.2** — hybrid replay, streaming, checkpoint/resume
- **v0.3** — multi-server MCP, TypeScript backend, reducer state, OTel span export
- **v0.4** — `nudge cost` static cost report, user-defined routing
- **v1.0** — A2A export, LSP (VS Code), frozen trace format
- **Docs i18n** — Simplified Chinese documentation after v0.1; compiler diagnostics are localization-ready from day one

## Conventions

- Design changes require an amendment PR against `docs/design.md` first.
- Every diagnostic code needs a conformance fixture before it ships.
