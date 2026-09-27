# Nudge Recipes

Small, copy-pasteable patterns — each file is a complete program that
compiles against the current grammar (CI type-checks every recipe, so
they never drift). For full runnable examples see
[`examples/`](../../examples/); for a guided tour run `nudgec learn`.

| Recipe | Pattern |
|:---|:---|
| [typed-extraction](typed-extraction.ndg) | pin LLM output to a record type with `schema:` — downstream code sees typed fields, never free text |
| [fallback-model](fallback-model.ndg) | cheap-first model routing: small inputs to the cheap model, `otherwise` to the strong one |
| [human-escalation](human-escalation.ndg) | automate the confident, escalate the rest — `decide{}` + `route{}` with a human `otherwise` |
| [injection-guard](injection-guard.ndg) | prove pure code survives adversarial input — `for_all` over `gen.injection()`, failures shrink to a minimal case |
| [cost-capped-call](cost-capped-call.ndg) | `budget:` wall + `retry: 2 with repair` — schema violations get repaired, never crash |
| [par-map](par-map.ndg) | fan out over a list with `par map` — concurrent, traced, results in input order |
