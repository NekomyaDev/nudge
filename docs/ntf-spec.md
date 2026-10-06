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
| `kind` | string | One of `llm.call`, `tool.call`, `fn.return`, `decision.call`, `computer.observe`, `computer.act` |

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
`NUDGE_DECISION_CACHE` rather than the wire — docs/decision.md §4), `batch`
(`{size, wall_ms}` on records produced as part of a multi-state
`predict_batch` call; per-record `latency_ms` is the batch average —
docs/decision.md §4).
`trace-diff` reports decision counts and total latency; `--fail-on-regression`
gates latency growth and decision failures.

### `fn.return` — a traced function boundary

| Field | Type | Meaning |
|:---|:---|:---|
| `fn` | string | Function name |
| `output` | string \| object | Returned value |

### `computer.observe` — one accessibility observation (v1.5, docs/computer-use.md)

| Field | Type | Meaning |
|:---|:---|:---|
| `app` | string | App observed (as named by the program) |
| `state_id` | string | Provider's observation id (e.g. `s-1`; monotonic per bridge) |
| `element_count` | number | Number of elements in the observed tree |
| `outcome` | string | `"ok"` or an error tag (`denied`, `deadline_missed`, `unreachable`) |
| `latency_ms` | number | Measured wall time of the observation |

Additive: `title` (window title), `tree` (rendered accessibility tree — the
text a model would read), `elements` (the full element table — lets replay
and the drift diff rebuild the Observation), `snapshot_mode` (`"full"`
today; delta observations are a future extension), `screenshot_hash`
(sha256 of the screenshot when `include_screenshot` was set),
`screenshot_asset` (name of the screenshot's content-addressed sidecar
file — the pixels live in `<trace>.assets/<hash>.txt` next to the trace,
never in the JSONL; replay reads the sidecar to rebuild the full
Observation), `drift`
(`{changed, screenshot_changed, added, removed, summary}` — populated in
drift-check replay mode when the live observation differs from the
recorded one), `deadline_ms`, `branch`, `replay_check: true` (drift-check
mode: this observation was re-taken live).

### `computer.act` — one executed (or dry-run) computer action

| Field | Type | Meaning |
|:---|:---|:---|
| `action` | string | `click`, `type`, `key`, `scroll`, `set_value`, `drag`, `perform`, `paste` |
| `app` | string | App the action targeted |
| `target` | number \| object | Element index, or `{"x": ..., "y": ...}` raster pixels |
| `outcome` | string | The provider's own outcome tag: `"ok"`, `stale_state`, `not_actionable`, `denied`, `error`, `dry_run`, … — the deadline is NEVER an overwrite of it |
| `latency_ms` | number | Measured wall time |

Additive: `ok` (boolean success), `action_sent` (dispatch receipt — False
only when the bridge knows the input was NOT dispatched; that is the only
safe-to-retry case), `deadline_missed: true` (additive latency metadata —
never an outcome overwrite), `error` (message when not ok), `value` (text
payload for `type`/`set_value`/`paste`), `dry_run: true` (replay mode — the
action was NOT re-executed), `deadline_ms`, `branch`. Replay never re-fires
actions: `dry_run: true` is how an auditor tells a replayed run from a
live one. Replay is also signature-verified: the recorded action name,
app and target must match the program's call (targets compared in
canonical form — index or `{x, y}`) or replay raises `ReplayMismatch`
instead of replaying a decision that was never made.

## Additive fields (already shipping in v1, all optional)

Consumers MUST accept and preserve these; producers MAY emit them:

- `pricing: "unknown"` — `cost_usd` is a $0 placeholder, not a measurement
- `request_hash` (on `llm.call`, `decision.call`, `tool.call`,
  `computer.act`) — sha256 over the canonical request (sorted-key JSON of
  everything that affects the response: llm = prompt + model + schema +
  params + images; decision = state + questions + model; tool = server +
  tool + arguments; computer = app + action + full payload). Replay is
  identity-first: a record is served only when its request_hash matches
  the program's call, and never twice — a changed program cannot consume
  an old answer (`ReplayMismatch`). Records without the field (legacy
  traces) replay in global record order as before.
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
