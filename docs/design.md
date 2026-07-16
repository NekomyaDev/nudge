# Nudge — Language Design

**Version:** 1.5 (2026-07-17) · **Status:** Frozen for MVP implementation
**Audience:** compiler implementers, language designers, early adopters

> **Changelog v1.5:** trace store + replay + test blocks landed (roadmap day 9–10). §6.1: `llm.call` records carry inline `input`/`output` at MVP — the content-addressed payload store is deferred post-MVP (v1-compatible additive fields). New `fn.return` record (`fn`, `output`), emitted by every effectful fn; `Trace.output` is the last `fn.return` value. §6.3 concrete MVP semantics: `replay(path)` returns a `Trace` (`.cost_usd` = Σ llm.call cost; `.output` dot-accessible); unsupported record versions raise `ReplayMismatch`. §6.2: full replay runs with `NUDGE_REPLAY=<trace>` — `llm_call` consumes recorded outputs in order (repair rounds replayed faithfully, zero provider calls, no new `llm.call` records). Test blocks lower to `nudge_test_<slug>()` and run via `nudgec test`.
>
> **Changelog v1.4:** effect inference landed (roadmap day 7–8). §3.2/§11 clarified: **E0301** = a function performs an effect but has no `uses` clause at all; **E0302** = a `uses` clause exists but omits an inferred effect (annotation too narrow). MVP effect sources: `llm"""` → `LLM`; calling a declared `tool` or `mcp(...)` → `Tool`; `replay(...)` / `python(...)` → `IO`. Effects propagate transitively through user-function calls (fixpoint over the call graph). `test` blocks are exempt — they exist to exercise effectful code. E0101 also covers unknown effect names in `uses` clauses.
>
> **Changelog v1.3:** type checker landed (roadmap day 4–6). §11: E0101 now also covers unknown type names; E0201 generalizes to *type mismatch* (schema ↔ return, let annotations, call arguments). §14 concrete MVP lowering: type aliases → `rt.schema({...})` JSON-Schema literals; record values are plain dicts validated at runtime (dataclasses post-MVP); tool bodies → `rt.tool_stub` until the MCP client lands (day 8–10). §15: the fake provider is schema-driven — it synthesizes conforming values, and `NUDGE_FAKE_FAIL_FIRST=k` forces k initial schema violations so the repair loop is testable at zero token cost. Trace records gain additive `repair_round` and `outcome` fields (v1-compatible: consumers must ignore unknown fields).
>
> **Changelog v1.2:** §12 keyword set split into **reserved** and **contextual** keywords. Option names (`schema`, `model`, `retry`, …), tool fields (`impl`, `side_effects`), and builtin names (`replay`, …) now lex as ordinary identifiers and are recognized by the parser only in their grammatical positions — so they remain usable as variable and record-field names. No surface-syntax change; this is an implementation-honesty fix discovered by the MVP parser test suite.
>
> **Changelog v1.1:** language renamed Niyet → **Nudge** (extension `.ndg`, CLI `nudge`, runtime `nudge_runtime`, store `~/.nudge/`); §10 toolchain line corrected to the actual zero-dependency implementation; §11 gained the E00xx lex/parse range. No semantic changes.

---

## 1. Overview

Nudge is a general-purpose programming language in which an LLM call is a **first-class, typed effect**. It compiles to Python and interoperates with the existing ecosystem (`import python("numpy")` reaches any pip package).

**One-sentence value proposition:** LangGraph's checkpoint guarantees, BAML's type safety, and DSPy's compiled-program philosophy — delivered as *language semantics*, not library conventions.

**Why a language and not a library:**

| Problem | Why a library falls short |
|---|---|
| Typed LLM output | a library validates at runtime; a compiler proves at compile time |
| Effect tracking (pure / LLM / Tool / IO) | a library cannot see what callers do; a compiler can |
| Deterministic replay | a library needs monkey-patching; a language runtime records natively |
| Static cost estimation | a library never sees the whole program; a compiler does |
| Safe parallel fan-out | library users manage async by hand; the compiler knows the effects |

## 2. Design Principles

1. **An LLM call produces a value, not a string.** Every model call binds to a schema; output that fails the schema never reaches user code.
2. **Side effects are visible.** Whether a function calls a model or writes a file is readable from its signature.
3. **Replay is a first-class citizen.** Every run emits a trace; every trace can become a test.
4. **Cost is a resource type.** Budget is a contract enforced by compiler and runtime.
5. **The escape hatch is always open.** Raw strings, raw API calls, raw Python — all allowed, all explicitly marked.
6. **Python is the ground, not the enemy.**

---

## 3. Language Core

### 3.1 Values and Types

```
int, float, bool, string, bytes, timestamp
[T]                    // list
{key: T}               // record
T | U                  // union
T?                     // optional (T | none)

type Step = { id: int, title: string, done: bool }
type Plan = { steps: [Step], confidence: float @range(0, 1) }

// Refinements are part of the schema:
type Score = float @range(0, 100)
type Url   = string @format(url)
```

Refinements apply to LLM output too: a model returning `confidence: 1.7` counts as a schema violation and triggers a repair round.

### 3.2 Functions and Effect Annotations

```
fn merge(a: Plan, b: Plan) -> Plan { ... }              // pure — compiler proves it
fn research(q: string) -> Report uses LLM, Tool { ... } // effectful — visible
fn save(r: Report, path: string) -> () uses IO { ... }
```

The effect set is **inferred**: if the user writes it, the compiler verifies; if omitted, the compiler fills it in. Calling an LLM inside a function that does not declare `uses LLM` is a **compile error**. This is the static answer to "can this function inflate my bill tonight?"

Verification rule (v1.4): a function that performs an effect with **no** `uses` clause at all is **E0301**; a `uses` clause that omits an inferred effect is **E0302** (annotation too narrow). Effects propagate transitively through user-function calls. `test` blocks are exempt.

| Effect | Meaning | Replay behavior |
|---|---|---|
| *(none)* | pure computation | re-executes (deterministic) |
| `LLM` | model call | read from trace |
| `Tool` | external tool (API, code exec) | per policy: mock / live |
| `IO` | file, network, clock, randomness | per policy |

## 4. LLM Call Semantics

### 4.1 Basic Call

```
let plan: Plan = llm"""
    Break this research question into steps: {question}
    Today is {now.date}. User locale: {locale}.
""" with {
    model:  "anthropic:sonnet-4.6",
    schema: Plan,                 // output type == return type
    retry:  3 with repair,        // automatic repair on schema violation
    budget: 0.02 USD,             // overrun => BudgetExceeded
    cache:  content_addressed,    // same input => cache hit, 0 tokens
    tags:   ["planner", "v3"],    // trace and eval filtering
}
```

The compiler proves: (a) `schema` is compatible with the declared return type; (b) every interpolated variable exists and is string-serializable; (c) a missing `budget` is inherited from the calling context (warning if no ancestor declares one).

### 4.2 Repair Protocol (exact)

On schema violation, the runtime:

1. Sends the model's raw output **plus** the JSON Schema validation error back to the same model: *"Your previous output failed validation. Errors: <list>. Emit corrected output only."*
2. Repeats up to `retry` times; every round is a separate trace record with `repair_round: n` and `outcome` (§6.1).
3. If exhausted, raises `SchemaFailure` carrying all raw outputs. The program may catch it, or the call may declare `fallback: <model>` to degrade to a cheaper/stronger model instead.

### 4.3 Budget Inheritance and Semantics

- Units: `USD` only in v0.1 (token ceilings planned).
- A call without `budget` inherits the remaining budget of its enclosing run.
- A run is started with `nudge run --budget 0.50` or a `run { budget: ... }` block.
- `par` branches split the remaining budget dynamically: the runtime tracks a shared counter; when it hits zero, all in-flight branches receive `BudgetExceeded`.
- Static analysis: the compiler sums literal budgets along each path and reports worst-case and expected (with cache-hit probability annotation) cost per function. Non-literal budgets are reported as `dynamic`.

### 4.4 Model Routing

```
model: route{
    cheap:  "openai:gpt-4.5-mini"  when confidence_not_needed,
    strong: "anthropic:sonnet-4.6" otherwise,
}
```

If routing is statically resolvable, the compiler reports cost ranges for both branches. User-defined routing functions are deferred (see Open Decisions).

### 4.5 Streaming

```
stream let report: Report = llm""" ... """ with { schema: Report }
for chunk in report.chunks() { ui.render(chunk) }   // chunk: Partial[Report]
```

Schema validation runs **incrementally** over partial JSON; a prefix that can no longer satisfy the schema aborts the stream early and triggers repair.

---

## 5. Parallelism

Concurrency primitives sit on top of the effect system; data races are compile errors.

```
let results = par map plan.steps |s| -> execute(s)        // fan-out
let (a, b)  = par all (fetch_x(), fetch_y())              // barrier
let fastest = par race [ask_a(q), ask_b(q)]               // first wins; losers cancelled, budgets refunded
let done    = par map(tasks, concurrency = 8) |t| -> run(t)
```

Compiler guarantee: no shared mutable state inside `par` (mutable state exists only in `state` blocks, §7). Two parallel branches writing the same `state` field is error E0402 unless that field declares a `merge` reducer.

## 6. Deterministic Replay and Traces

### 6.1 Trace Format

Every run emits an append-only trace (JSON Lines). Payloads live in a content-addressed store (`~/.nudge/store/`); the trace carries hashes only, so traces are small, diffable, and committable to git.

```json
{"v": 1, "seq": 12, "kind": "llm.call", "fn": "research", "prompt_hash": "b3f1…",
 "model": "anthropic:sonnet-4.6", "params": {"temperature": 0},
 "input_hash": "9aa2…", "output_hash": "c41d…", "tokens": {"in": 812, "out": 341},
 "cost_usd": 0.0075, "dur_ms": 2310, "repair_round": 1, "outcome": "ok"}
{"v": 1, "seq": 13, "kind": "tool.call", "tool": "web.search", "input_hash": "…", "output_hash": "…"}
{"v": 1, "seq": 14, "kind": "fn.return", "fn": "research", "output": {"title": "…", "findings": […]}}
```

**Payloads at MVP (v1.5):** `llm.call` records carry `input`/`output` inline; the content-addressed payload store above is deferred post-MVP. Every effectful fn also emits a `fn.return` record — `Trace.output` (§6.3) reads the last one.

**Additive fields (v1.3):** `llm.call` records carry `repair_round` (0-based attempt) and `outcome` (`ok` / `schema_violation`). Consumers of v1 traces must ignore unknown fields.

**Versioning:** every record carries `v` (record schema version). `nudge trace migrate` upgrades old traces. The v1 record schema is frozen with the MVP.

### 6.2 Run Modes

```
nudge run main.ndg                    // live: everything executes, trace written
nudge replay trace_042.jsonl          // full replay: LLM+Tool read from trace, pure code re-runs
nudge run main.ndg --replay=llm       // hybrid: LLM from trace, Tools live
```

Full replay sends **zero** requests to any model API → free regression tests in CI. Hybrid mode answers "did tool behavior drift?"

MVP mechanism (v1.5): `NUDGE_REPLAY=<trace.jsonl>` puts the runtime in full-replay mode — `llm_call` consumes recorded outputs in order (each repair round consumes its own record), no provider is called, and no new `llm.call` records are written. Running out of records raises `ReplayMismatch`.

### 6.3 Traces as Tests

```
test "research agent handles empty web results" {
    let t = replay("traces/empty_results.jsonl")
    assert t.output.status == "insufficient_data"
    assert t.cost_usd < 0.10
}
```

Any trace is automatically a property-test input. Snapshot update: `nudge test --accept`.

MVP lowering (v1.5): test blocks compile to `nudge_test_<slug>()` Python functions; `nudgec test <file.ndg>` type-checks, emits, and runs them (the `nudge test` runner follows at v0.1). `replay(path)` returns a `Trace`: `.cost_usd` sums `llm.call` costs, `.output` is the last `fn.return` value with dot access (`t.output.findings`). Unsupported record versions raise `ReplayMismatch`.

## 7. Agent State and Checkpoints

```
agent Researcher {
    state {
        notes:   [Note] = [],
        visited: {string: bool} = {},     // reducer: merge (union)
        round:   int = 0,                 // reducer: overwrite
    }

    fn step(q: string) -> () uses LLM, Tool {
        let r = llm"""Next action for: {q}""" with { schema: Action, budget: 0.01 USD }
        state.notes   += [r.note]
        state.visited  = state.visited | merge {r.url: true}
        state.round   += 1
    }
}
```

- Every `state` write is an automatic checkpoint (SQLite by default; Postgres via config).
- After a crash: `nudge resume <run_id>` continues from the last checkpoint.
- Fields with a `merge` reducer may be written by parallel branches safely (CRDT-style join).

## 8. Tools and MCP

```
tool web_search(q: string) -> [SearchResult] {
    impl: python("duckduckgo_search").ddg(q, max_results = 8)
    cost_hint: 0 USD
    side_effects: none              // read-only => mockable in replay
}

tool send_email(to: string, body: string) -> () {
    impl: python("smtplib")...
    side_effects: writes_external   // replay default: mock + warning
}
```

Tools are typed; when exposed to a model, their JSON schema is generated automatically. `side_effects` drives replay policy.

**MCP:** Nudge consumes MCP servers as native tool sources.

```
use mcp "filesystem" as fs
let files = fs.list("/data")        // typed tool call
```

---

## 9. Multi-Agent

Agents are values: passed as arguments, run under `par`, exported over A2A.

```
let planner = Planner()
let workers = par map subtasks |t| -> Researcher().run(t)
let report  = Synthesizer().merge(workers)

export agent Researcher at a2a://agents.example.com/researcher   // emits an Agent Card
```

## 10. Compiler and Runtime Architecture

```
.ndg source
   │
   ▼
Lexer + Parser (hand-rolled, zero dependencies at MVP)
   ▼
AST → HIR (desugar: llm""" """ → llm_call node)
   ▼
Type checker ── refinement/schema validation ── effect inference
   │
   ▼
Static cost analysis (model price table + token estimation)
   │
   ▼
Python codegen (single file, stdlib + nudge_runtime)
   │
   ▼
nudge_runtime (pip): trace store, checkpoints, MCP client, repair loop, par scheduler
```

**Decision — compiler in Rust:** fast (compilation imperceptible), single-binary distribution, portable to a WASM playground later. The lexer is hand-rolled and dependency-free (shipped); if grammar complexity grows, `logos`/`winnow` may be adopted without changing the token/AST contracts.
**Decision — first backend Python:** the target user (AI/ML engineer) lives there. TypeScript is the second backend; the split is made at HIR so backends share all analysis.

## 11. Error Model and Diagnostics

Runtime errors (all catchable; an uncaught error still leaves a complete trace up to the crash point):

```
BudgetExceeded   — budget wall hit (run or call level)
SchemaFailure    — retries exhausted; raw outputs preserved in trace
ToolFailure      — tool exception; retry policy declared on the tool
ReplayMismatch   — hybrid mode: live tool output inconsistent with trace
ModelUnavailable — provider failure; falls to fallback chain if declared
```

Compile-time diagnostics use stable codes; messages are English-first and localizable (zh-CN first). Initial catalog:

| Code | Meaning | Example trigger |
|---|---|---|
| E00xx | lex/parse errors (reserved range) | E0001 unexpected character, unterminated literal |
| E0101 | unknown identifier or type name | `{qusetion}` typo; `x: Strnig` |
| E0201 | type mismatch (schema ↔ return, let annotation, call argument) | `schema: Plan` vs `-> Report` |
| E0202 | refinement malformed | `@range(1)` |
| E0301 | effect used with no `uses` clause at all | LLM call in a plain `fn` |
| E0302 | `uses` clause omits an inferred effect | body calls a tool, signature says only `uses LLM` |
| E0401 | `par` branch captures mutable outer state | writing outer `let` |
| E0402 | two branches write same state field without reducer | both write `state.round` |
| E0501 | budget unit unknown | `budget: 5 EUR` (v0.1) |
| E0601 | replay trace version unsupported | v0 trace with v1-only runtime |

## 12. Grammar Summary (informative)

**Reserved keywords** (never usable as identifiers): `fn let type tool agent state uses with par map all race for in if else return test assert export use and or true false none`

**Contextual keywords** (keyword meaning only in specific grammatical positions; lex as ordinary identifiers and remain usable as variable/field names): `schema model retry repair budget cache tags stream` (with-block options) · `impl side_effects` (tool fields) · `replay` (builtin) · `fallback route when otherwise` (routing, lands with the type checker)

Operators by precedence (high → low):

```
1  ( )  [ ]  .            call, index, field
2  -  !                   unary
3  *  /  %                multiplicative
4  +  -                   additive
5  ==  !=  <  <=  >  >=   comparison
6  and                    conjunction
7  or                     disjunction
8  |>                     pipe (left-assoc)
9  =  +=  -=              assignment (non-associative)
```

Literals: `42`, `4.2`, `"text"` (no interpolation), `llm""" ... """` (interpolated prompt), `0.02 USD`, `true/false/none`, lists `[a, b]`, records `{k: v}`.

**Lexical rules:** source is UTF-8; identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`); string literals may contain arbitrary UTF-8; `//` comments run to end of line.

## 13. CLI Specification

```
nudge build <file.ndg>            compile; emit Python to ./out/
nudge run <file.ndg> [--budget X] [--replay=llm|all]
nudge replay <trace.jsonl> [--at seq N]
nudge resume <run_id>
nudge test [--accept] [--filter "name"]
nudge cost <file.ndg>             static cost report per function (v0.4)
nudge trace migrate <trace>       upgrade record schema versions
nudge fmt <file.ndg>              canonical formatter
```

Exit codes: `0` ok · `1` compile/runtime error · `2` budget exceeded · `3` replay mismatch.

---

## 14. Codegen Contract (Python backend)

How core constructs lower to Python + `nudge_runtime`:

```
Nudge                                   Python (conceptual)
─────────────────────────────────────────────────────────────────
fn f(x: T) -> U uses LLM          →     @rt.effectful(effects={"LLM"})
                                        def f(x: T_) -> U_: ...

let p: Plan = llm"""..."""        →     p = rt.llm_call(
    with {schema: Plan, ...}              prompt=rt.render(tmpl_hash, {"question": q}),
                                          schema=rt.schema({...}), retry=3, repair=True,
                                          budget=USD("0.02"), cache="content_addressed",
                                          tags=("planner","v3"))

par map xs |x| -> g(x)            →     rt.par_map(xs, g, concurrency=None)

state.x += v                      →     rt.state_update(run, "x", rt.ADD, v)   # checkpointed

replay("t.jsonl")                 →     rt.replay(Path("t.jsonl"), mode=rt.Mode.FULL)
```

Rules: (a) generated code imports only stdlib + `nudge_runtime`; (b) type aliases lower to `rt.schema({...})` JSON-Schema literals — record values are plain dicts validated at runtime (dataclasses land post-MVP), and tool bodies lower to `rt.tool_stub` until the MCP client lands (day 8–10); (c) no reflection over user code at runtime — all checks resolved at compile time; (d) the emitted file is deterministic for identical input (enables codegen golden tests).

## 15. Conformance and Testing Strategy

- **Lexer/parser:** golden token streams and AST snapshots per fixture in `conformance/syntax/`.
- **Type/effect checker:** positive + negative fixtures; every E-code must have at least one triggering fixture (§11).
- **Codegen:** golden-output tests — identical `.ndg` input must emit byte-identical Python.
- **Runtime:** fake-provider harness (deterministic mock model) drives repair, budget, cache, and checkpoint tests without network. The fake provider is schema-driven — it synthesizes conforming values; `NUDGE_FAKE_FAIL_FIRST=k` forces k initial schema violations so the repair loop is testable at zero token cost.
- **End-to-end:** `examples/research_agent.ndg` is the v0.1 acceptance test (§16).
- **CI:** `cargo test` + fixture suite + e2e with fake provider on every PR.

## 16. Full Example — the Self-Testing Research Agent

See [examples/research_agent.ndg](../examples/research_agent.ndg). Its guarantees: schema conformance proven at compile time; static worst-case cost ≈ $0.075 + merge call; `resume` after crash; replay test in CI at zero token cost.

## 17. Competitive Landscape (as of July 2026)

| Rival | What it does | Nudge's edge |
|---|---|---|
| **BAML** (closest) | typed prompt DSL; TS/Python/Ruby codegen, repair, tracing | stops at the single call. Nudge is a full language: effects, parallelism, state, replay, budget contracts |
| **Pydantic AI v2** (Jun 2026) | typed agents, budget caps, durable execution, YAML specs | a Python library: no effect tracking, no compile-time schema proofs, no language-level replay |
| **LangGraph** | checkpointed state graphs, time-travel, strongest state mgmt | verbose graph definitions; replay is a debugging feature, not test semantics. Nudge programs read as programs, not graphs |
| **DSPy / Ax** | prompt-optimizing compiler; typed signatures | optimization-focused; no orchestration/effects/replay. (Possible v1+ Nudge plugin) |
| **Mastra** (TS) | workflows + memory + evals + replay UI | framework conventions; no language guarantees |
| **LMQL** (2023) | constraint query language | stalled; no modern tool/agent semantics |

**Gap confirmed:** no general-purpose language combines typed LLM effects, trace-as-test, budget contracts, and safe parallelism. MCP/A2A have matured as protocols — Nudge rides on top of them rather than competing.

## 18. Risks and Decisions

### Risks

1. **Model API drift** — same prompt, different behavior months later. Mitigation: traces test *program logic*; hybrid replay + periodic live runs documented as the drift-detection practice.
2. **Replay fidelity** — temperature 0 is not bit-deterministic. Mitigation: full replay never calls the model; hybrid mismatches surface as `ReplayMismatch` warnings.
3. **Two-backend maintenance (Python + TS)** — early TS work would kill the MVP. Deferred to v0.3.
4. **Scope creep** — "general-purpose language" wants everything. The `import python(...)` escape hatch absorbs ~90% of feature pressure.

### Decisions locked

| Question | Decision | Rationale |
|---|---|---|
| DSL or full language? | Full language, small core | effects only make sense in a language |
| Backend | Python first, TS second | users are on Python; backend split at HIR |
| Trace format | JSONL + content-addressed store | git-friendly, diffable; OTel span export in v0.3 |
| Multi-agent primitive? | agent = value; A2A export at v1.0 | let the protocol mature |
| Killer demo | self-testing research agent (§16) | replay + budget + parallelism in one artifact |
| Name | **Nudge** (`.ndg`) | English, short, N-logo preserved, no PL conflict |
| Syntax keywords | English only | international audience |
| Docs languages | English now; zh-CN after v0.1 | diagnostics designed localizable from day one |

### Still open

- [ ] User-defined routing functions (v0.4?)
- [ ] DSPy-style automatic prompt optimization as a compiler plugin
- [ ] Trace privacy: prompt/response encryption for PII
- [ ] Name registration: domain + crates/pypi availability

## 19. Implementation Readiness Checklist

- [x] Type system core and refinements defined (§3)
- [x] Effect algebra + inference rule (§3.2)
- [x] LLM call semantics incl. exact repair protocol (§4)
- [x] Budget units, inheritance, static analysis rules (§4.3)
- [x] Parallel primitives + race-freedom rule (§5)
- [x] Trace schema, versioning, store layout, run modes (§6)
- [x] State/checkpoint semantics + reducer rule (§7)
- [x] Tool contract + MCP consumption (§8)
- [x] Compiler pipeline + backend split point (§10)
- [x] Diagnostics catalog with stable codes (§11)
- [x] Grammar summary + lexical rules (§12)
- [x] CLI surface + exit codes (§13)
- [x] Codegen lowering contract (§14)
- [x] Test strategy incl. fake-provider harness (§15)
- [x] v0.1 acceptance example (§16)

**Verdict:** sufficient to implement days 1–14 of the MVP without further design rounds. Open items are all post-v0.1.
