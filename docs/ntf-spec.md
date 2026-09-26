# NTF — Nudge Trace Format (v1)

NTF is the open trace format for LLM-agent runs: JSON Lines, one record per
line, describing every LLM call, tool call and function return with its
tokens, cost and outcome. It is what makes nudge agents **replayable** — and
it is deliberately boring: no protobuf, no SDK required, `grep`-able, and
diffable (`nudgec trace-diff a.jsonl b.jsonl`).

Think of it as *"what happened in my agent, written down well enough to
re-run and audit"* — the same role OTel spans play for services, but shaped
for LLM calls (prompts, schemas, token counts, USD).

> **Status: frozen v1.** Additive fields may appear at any time; anything
> else (removals, renames, type changes) requires a `v: 2` schema and a
> migration path. Consumers MUST ignore unknown fields.

## Framing

- One JSON object per line, UTF-8, `\n`-separated. Empty lines are skipped.
- The whole file is the trace; record order is meaningful (see `seq`).

## Common required fields (every record)

| Field | Type | Meaning |
|:---|:---|:---|
| `v` | number | Record schema version. Must be `1` — validators raise **E0601** on any other value |
| `seq` | number | 1-based, gapless record counter across the trace |
| `kind` | string | One of `llm.call`, `tool.call`, `fn.return`, `decision.call` |

## Record kinds

### `llm.call` — one model invocation (including each repair attempt)

| Field | Type | Meaning |
|:---|:---|:---|
| `model` | string | Model id as invoked, e.g. `openai:gpt-4.5-mini` |
| `params` | object | Generation params actually sent (`temperature`, …) |
| `input` | string | Final rendered prompt (after interpolation) |
| `output` | string \| object | Raw text or the schema-validated structured answer |
| `tokens` | object | `{"in": number, "out": number}` |
| `cost_usd` | number | Metered cost of this call in USD |
| `repair_round` | number | 0 for first attempt, 1+ for schema-repair retries |
| `outcome` | string | `"ok"` or an error tag; consumers treat non-`"ok"` as failed |
| `provider` | string | Provider/backend id, e.g. `fake`, `openai`, `anthropic` |

### `tool.call` — one external tool invocation

| Field | Type | Meaning |
|:---|:---|:---|
| `tool` | string | Tool name as declared |
| `input` | string | Invocation input (rendered) |
| `output` | string \| object | Tool result |

### `decision.call` — one batched decision (v1.4, design §11)

| Field | Type | Meaning |
|:---|:---|:---|
| `model` | string | Decision model id, e.g. `laya:multilingual` |
| `provider` | string | Registry/provider entry used |
| `questions` | object | Questions keyed by name (`kind`, `prompt`, `options`/`levels`) |
| `answers` | object | Typed answers keyed by name (winner/p/distribution/confidence for choice) |
| `latency_ms` | number | Measured wall time of the decision call |
| `outcome` | string | `"ok"` or an error tag (`deadline_missed`) |

Additive: `deadline_ms` (declared budget), `level` (calibration level, e.g.
AnyJev `raw/L0/L1/L2`), `cache` (`"hit"` when the record's answers came from
`NUDGE_DECISION_CACHE` rather than the wire — docs/decision.md §4).
`trace-diff` reports decision counts and total latency; `--fail-on-regression`
gates latency growth and decision failures.

### `fn.return` — a traced function boundary

| Field | Type | Meaning |
|:---|:---|:---|
| `fn` | string | Function name |
| `output` | string \| object | Returned value |

## Additive fields (already shipping in v1, all optional)

Consumers MUST accept and preserve these; producers MAY emit them:

- `pricing: "unknown"` — `cost_usd` is a $0 placeholder, not a measurement
- `route` (on `llm.call`) — which `route{}` arm selected this model
- `server` (on `tool.call`) — MCP server the tool resolved to
- `streamed` / `chunks` / `early_abort` — streaming bookkeeping
- `branch` — `par` lane label the record was produced in

Producers must not remove or rename any field above. A record with an
unknown `kind` is invalid for consumers that need to interpret it; the
reference validator rejects it.

## Validation

The reference validator ships in the compiler:

```sh
nudgec trace-check run.jsonl     # exits 0 when the trace conforms
```

It checks framing (JSON-per-line), `v: 1` (E0601 otherwise), gapless `seq`
starting at 1, per-kind required fields, and field **types** (a string in
`tokens.in` is invalid, not merely odd). `nudgec trace-diff` builds on the
same parsing; `--fail-on-regression` turns a diff into a CI gate.

## Conformance suite

Any tool that writes or reads NTF can be checked against the reference
validator using the corpus in [`conformance/`](../conformance/): each case is
a `trace.jsonl` plus `expected.json` naming the error substrings a conformant
validator must report (or `{"valid": true}`). Run it with:

```sh
python3 conformance/run.py            # needs nudgec on PATH
cargo test -p nudgec conformance      # same cases through the Rust validator
```

## Why frozen matters

Traces outlive code: a run recorded today must still replay, diff and audit
years from now, across vendors. The frozen-schema discipline is what turns
`run.jsonl` from a log file into an artifact you can pin a CI gate or a
compliance report to.
