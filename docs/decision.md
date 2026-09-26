# RFC — Decision: typed decisions as a language primitive (v1.4)

Status: implemented in v1.4 (this document is the contract). Supersedes the
"decisions" section of the 2026-09 product plan draft.

## 1. Thesis

Nudge started as a typed DSL for LLM agents. The wider thesis it was always
pointing at: **uncertainty becomes programmable** — whether it comes from an
LLM generating text or from a decision model producing a calibrated
distribution. v1.4 makes the second source a language primitive on par with
`llm"""`:

- **Same effect system.** `Decision` joins `LLM / Tool / IO`; inference,
  signature verification and property purity (E0804) treat it identically.
- **Same trace/replay/CI story.** Decisions land in NTF traces (§11.4 in
  design.md), replay for $0, and diff/CI-gate like everything else.
- **Same backend parity rule.** Python and TypeScript share one wire
  contract and one test-vector suite.
- **The metric axis shifts.** The decision-model family (Laya, Jev, AnyJev,
  Valen, …) prices in *milliseconds*, not dollars — the local end is a
  322M-parameter encoder with no currency at all. So `deadline` is a
  first-class option next to `budget`, traces record `latency_ms`, and
  `trace-diff --fail-on-regression` treats latency growth as a regression.
  USD budgets remain for LLM calls; they stop being the headline.

## 2. Wire compatibility, not allegiance

The JEV family converged on one request shape: `state` + named typed
questions (`choice` / `score` / `noul`) → typed answers with distributions
plus derived confidence. Laya's `laya.serve` and the TypeSafe Jev API speak
the same `POST /v1/systemone` contract; AnyJev extracts the same primitives
from any LLM's logits (with per-level calibration heads); Valen trains
decision-native models whose outputs match the same three kinds.

Nudge targets the **wire contract**. No vendor endorsement is implied, no
family governance is claimed, and protocol conformance is separate from
task quality: a provider passing our adapter tests is not thereby accurate.

## 3. Language surface

```nudge
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept:    "which team handles this?" choose [billing, technical, security],
        risk:    "does this mention churn?" yes/no,
        urgency: "how urgent?" score [low, soon, critical]
    }
    on t
    with { model: "laya:multilingual", deadline: 50 }
    d.dept.winner
}
```

- `decide { name: "prompt" <kind>, ... } on <state> with { ... }` — one
  batched call for all questions (family semantics). `on` is required.
- Question kinds (frozen v1): `choose [labels…]`, `yes/no` (noul),
  `score [levels…]` (ordinal rubric). Labels are bare identifiers or
  strings.
- With-options (frozen v1): `model` (string, provider:tag), `deadline`
  (int milliseconds), `null_option` (reserved for abstention labels).
- The result is a record with one field per question:
  - choice → `winner: string, p: float, distribution, confidence: float`
  - noul → `p: float`
  - score → `score: float, distribution`
  - additive at runtime: `model`, `level` (calibration level, e.g. AnyJev
    `raw/L0/L1/L2`), `deadline_missed`.

### Checker rules

| Code | Meaning |
|:---|:---|
| E0301 | `decide` without `uses Decision` (inferred automatically) |
| E0806 | unknown decide option, or wrong option type |
| E0807 | empty/duplicate-free choice with 0 or >255 options; rubric outside 2..=10 levels |
| W0005 | >20 options — family heads degrade past ~20 (Laya 422s >126) |

`route{}` generalization: arm values are now expressions (lazy). A bare
string arm keeps the model-routing semantics; any other value makes the
route a **policy switch** — the intended home for confidence thresholds:

```nudge
let action = route {
    auto:  resolve(d.dept.winner) when d.dept.confidence > 0.8,
    human: escalate(d.dept.winner) when d.risk.p > 0.5,
    review: queue(d) otherwise
}
```

## 4. Providers

- **fake (default)** — deterministic FNV-seeded distributions over the
  declared options; `nudgec test` stays $0 and byte-reproducible.
- **HTTP `/v1/systemone`** — `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'`;
  `model: "laya:multilingual"` selects the entry by prefix.
- Adapter validation (both backends): ids preserved exactly; missing /
  extra / duplicate answers are hard errors; NaN, out-of-range and
  non-normalized distributions rejected (renormalization is explicit,
  bounded, and recorded); noul polarity never flipped; rubric order and
  mapping preserved; answers never silently truncated.
- Unconfigured named providers raise — a decision never silently falls
  back to fake (the audit's "silent fake resume" lesson, applied).

### Deadline semantics

Soft by default: an overrun annotates every answer with
`deadline_missed: true` (visible in the trace; policy can route on it).
`NUDGE_DECISION_STRICT=1` makes overruns fatal.

### Decision cache

`NUDGE_DECISION_CACHE=<path>` persists validated answers for **real**
providers across runs — a representation cache: the same state text +
question shape + model is the same decision, so paying for it twice is a
bug, not a feature. Semantics (both backends):

- Key: SHA-256 over canonical JSON of `{state, model, questions}` — any
  change to the state, an option list, a rubric, or the model changes the
  key; formatting never does.
- Only the HTTP transport is cached. The fake provider is already
  deterministic and free — caching it would add file I/O for nothing.
- Replay (`NUDGE_REPLAY=all`) always takes precedence over the cache, so a
  recorded trace replays byte-identically even with a warm cache.
- Records carry the additive field `cache: "hit"` when served from the
  cache; misses are unmarked (a fresh run's trace looks exactly like a
  no-cache run's).
- A missing or corrupt cache file is a cold start, never an error.

trace-diff note: cache hits drive `latency_ms` toward zero. The regression
gate only fails on growth, so a hit can never fake a regression — but it
can hide one: run latency gates on cache-cold traces.

## 5. Non-goals (v1.4)

Generic type parameters, calibration training, speculative execution,
distributed scheduling, a second syntax channel, and adapters beyond the
wire contract (AnyJev in-process, Valen, MCP transport) — all deferred
without closing the door.
