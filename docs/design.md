# Nudge — Language Design

**Version:** 1.22 (2026-07-25) · **Status:** Frozen for MVP implementation
**Audience:** compiler implementers, language designers, early adopters

> **Changelog v1.22:** bug-hunt round 7 — (1) `llm_stream` had the §4.3 pre-fix semantics: each round was walled individually, so streaming call sites could exceed their declared budget across repair rounds; streaming now shares the same site-cumulative wall as `llm_call`. (2) Prompt Clippy W0004 was wrongly suppressed for `stream let` (introduced in v1.19 on a mistaken premise): streaming shares the §4.2 repair loop — an early abort counts as a schema violation — so `stream` + `schema` without `retry … with repair` now warns like any other call.
>
> **Changelog v1.21:** §4.6 — the adapter gains a fifth provider: `mimo` (Xiaomi MiMo, OpenAI-compatible token plans; `mimo:mimo-v2.5-pro`, key from `MIMO_API_KEY`). Subscription-plan models price at $0 — budget walls keep working. The `provider-smoke` workflow accepts `mimo` as an input.
>
> **Changelog v1.20:** budget-wall fix (§4.3): the declared per-call `budget` now caps the **whole call site** — repair rounds share the site budget and a round that would exceed the remainder raises `BudgetExceeded` ("call site budget exhausted … repair rounds share the site budget"). Previously each round was walled individually, so a site with `budget: 0.001 USD` and `retry: 2 with repair` could spend 3× its declared budget without raising. This also aligns runtime behavior with the static cost report's retry worst-case. Regression-locked by two e2e tests.
>
> **Changelog v1.19:** Prompt Clippy audit round 2 — W0004 no longer fires on `stream let` calls (streaming validates incrementally per §4.5; there is no repair loop to miss), and identical warnings collapse into one line with a repetition count (`×N`) until spans give each warning its own position.
>
> **Changelog v1.18:** Prompt Clippy quality pass — warnings carry their context (`in fn analyze` / `agent X / fn f`); W0003 mentions match on word boundaries (a field named `inp` no longer matches "instruction"); W0004 (new): `schema` without `retry: N with repair` — a violation raises instead of repairing; and lints now surface in the editor as severity-2 LSP diagnostics on otherwise-clean files, not just CLI stderr.
>
> **Changelog v1.17:** Prompt Clippy shipped early (strategy backlog) — §20 (new): the compiler lints `llm"""` blocks with non-fatal W-code warnings printed on `check`/`build`/`build-ts`. W0001: llm call without a `budget` (uncapped cost). W0002: prompt under 4 words (vague instruction; `{interpolation}` holes don't count). W0003: a record `schema: T` whose fields never appear in the prompt text (the model can't guess an output contract it was never told). Warnings never fail the build.
>
> **Changelog v1.16:** v1.1d done — §10: the LSP server gains `textDocument/hover` (signatures + keyword docs), `textDocument/definition`, and `textDocument/completion` (keywords, primitive types, `with`-keys, user symbols), all served from a per-document declaration index built by a line scan (robust against partial input; the spanned AST will replace the scan without changing the protocol surface). §8: the MCP registry grows a real transport — entries with a `command` spawn the server over stdio and speak newline-delimited JSON-RPC (`initialize` → `notifications/initialized` → `tools/call`, one persistent session per server); real tool outputs land in the trace, JSON-shaped text content is decoded, and any transport/server error raises instead of silently faking. Entries without `command` keep the stub (`[]`) behavior; unknown server names still fail fast. SSE/HTTP transport is post-v1.1.
>
> **Changelog v1.15:** v1.1c done — the VS Code extension landed in `editors/vscode/`: a TextMate grammar for `.ndg` (keywords, record types, `llm"""` prompts with interpolation, USD money literals, `@format`/`@range` constraints), language configuration, snippets, and diagnostics wired to the §10 LSP server over stdio via `vscode-languageclient` — no protocol re-implementation in the extension. Packaged as a `.vsix` attached to the GitHub Release; hover/completion remain post-v1.1 (v1.1d).
>
> **Changelog v1.14:** v1.1a in progress — real providers landed. §4.6 (new): one stdlib-only OpenAI-compatible HTTP adapter serves `openai`, `gemini`, `groq`, and `ollama`. The provider is chosen by the model string prefix (`gemini:gemini-2.5-flash`) or by `NUDGE_PROVIDER`; `NUDGE_BASE_URL` overrides the endpoint, keys come from `NUDGE_API_KEY` or provider-specific envs (`GEMINI_API_KEY`, …). Real answers are JSON-extracted when a schema is set (fences → first balanced span → raw text into the repair loop), trace records carry the real provider name, real `tokens.in/out` from the API's usage, and a priced `cost_usd` from the runtime pricing table — free-tier quotas and local models price at $0, so budget walls keep working at zero cost. Streaming against real providers falls back to non-streaming; the TS runtime stays fake/replay-only at v1.1a (its adapter lands with async codegen). A secret-gated `provider-smoke` workflow runs a real agent against the Gemini free tier on demand.
>
> **Changelog v1.13:** v1.0 complete — the trace format is frozen, A2A export and the LSP server landed. §6: the v1 record schema (kinds `llm.call` / `tool.call` / `fn.return` with their required fields; additive fields like `streamed`/`route`/`server` remain allowed) is **frozen**, and `nudgec trace-check <trace.jsonl>` validates any trace against it (JSON-per-line, `v: 1`, sequential `seq`, per-kind required fields; E0601 on unknown versions). §9: `nudgec a2a <file.ndg>` emits A2A agent cards to `out/<name>.agent.json` — one card per `agent` block (skills from its fns, effects as tags), or a single card wrapping the top-level fns when the file declares no agents; serving the card over HTTP/A2A transport is post-v1.0. §10: `nudgec lsp` serves the Language Server Protocol over stdio (dependency-free JSON-RPC with Content-Length framing): `initialize`, full-document `didOpen`/`didChange`/`didClose` sync with `publishDiagnostics` backed by the real lex→parse→check pipeline (E-codes attached; check diagnostics point at file start until the spanned AST lands), `shutdown`/`exit`; hover/completion are post-v1.0.
>
> **Changelog v1.12:** v0.4 complete — the static cost report and user-defined model routing landed. §13: `nudgec cost <file.ndg>` walks each fn's AST and reports llm call sites under flat fake pricing ($0.001/call): per-fn lines plus a `total` line show `N llm call site(s), min $X, max $Y`; `retry: N with repair` multiplies the worst case (1+N calls), and sites inside `par map` bodies are marked runtime-dependent (× collection size). §4.4: `route{ label: "model" when cond, … , label: "model" otherwise }` is an expression returning a model string — arms evaluate top-down, the first truthy `when` wins, an `otherwise` arm is mandatory (E0702), non-bool conditions are E0201, and `route` stays a contextual keyword (only special directly before `{`). It lowers to `rt.route((label, model, lambda: cond), …)`; the winning label lands on the next `llm.call` record as the additive `route` field (§6). The TS backend mirrors the lowering (`rt.route([label, model, () => cond], …)`).
>
> **Changelog v1.11:** v0.3 complete — multi-server MCP routing, the TypeScript backend, and OTel span export landed. §8: a tool's `impl: mcp("server").…` server now flows into the emitted stub call; `NUDGE_MCP_SERVERS` (a JSON registry of server names → configs) validates it at call time (unknown server fails fast) and `tool.call` records gain an additive `server` field — real MCP transport still lands post-MVP. §6: with `NUDGE_OTEL=<path>` every trace record is additionally written as an OTel-shaped JSON-lines span (`traceId` per process, `spanId` per record, record fields as attributes, `status.code` 1/2) — file export only, OTLP transport post-MVP. §14: **TypeScript backend** — `nudgec build-ts` emits `out/<name>.ts` importing `./nudge_runtime.ts` (shipped in `runtime/`). The TS emitter covers type aliases, tools (with server routing), fns, llm calls, merge, and test blocks; `agent` blocks, `stream let` streaming, and par scheduling are deferred to the full TS backend and emit warning comments (`par map` lowers to a sequential `.map` at MVP). Annotations are limited to simple TS names. The TS runtime mirrors the fake provider, replay, budget walls, render/merge/USD; streaming, agent state, par pools, and OTel are Python-only at MVP.
>
> **Changelog v1.10:** the `| merge` reducer landed (v0.3 first item). §7: `l | merge r` is an infix expression (`merge` is contextual, only special directly after `|` — `|a|` lambdas and variables named `merge` are unaffected) lowering to `rt.merge(l, r)`: dicts union (right wins on key conflicts), lists append items the left side does not already hold (grow-only set), anything else is overwritten by the right side. The type checker requires two records or two lists (E0201 otherwise; list elements must be assignable). State reducer writes compose with checkpoints: `state.found = state.found | merge [f]` is an ordinary checkpointed write whose value is the join — deterministic replay keeps resume exact. The E0402 shared-state check and parallel state writes stay deferred (par-branch bodies are expressions at MVP, so state writes cannot occur inside them yet).
>
> **Changelog v1.9:** agent state + checkpoint/resume landed (v0.2 third item — v0.2 complete). §7 concrete MVP mechanism: `agent` blocks parse with a `state` section (typed fields with defaults) and plain `fn` members; `state.x = v` / `state.x += v` are statements (4-token lookahead keeps bare `state.x` reads expressions), and every write checkpoints the full state to `.nudge/runs/<run_id>/checkpoint.json` (JSON files — the SQLite/Postgres stores are post-MVP). The run directory registers `program` + `trace`, so `nudge resume <run_id>` re-executes the emitted program replaying the recorded trace prefix (`NUDGE_RESUME=1`): LLM and tool calls consume the recorded prefix, then go **live** and append to the same trace (exhaustion without resume still raises `ReplayMismatch`); the first `writes` state writes of the re-execution are suppressed because the checkpoint already reflects them — deterministic re-execution makes the write sequence identical. New diagnostic **E0701** (state write outside an agent block / unknown state field); `=` writes are type-checked against the declared field, `+=` is list-concat/numeric-add. Reducers (`| merge`, E0402) and parallel writes stay deferred.
>
> **Changelog v1.8:** streaming landed (v0.2 second item). §4.5 concrete MVP mechanism: `stream let` lowers to `rt.llm_stream` — the provider answer streams in deterministic chunks and every prefix is validated **incrementally** (`_PrefixValidator`): a literal of the wrong type starting, a number completing outside `minimum`/`maximum` or non-integral under `integer`, an invalid `format: uri` string, an object closing without a `required` key, or malformed JSON aborts the stream at that chunk and counts as a schema violation, so the §4.2 repair loop applies unchanged. §6.1: `llm.call` records gain additive `streamed` / `chunks` / `early_abort` fields. Chunk-by-chunk consumption (`for chunk in report.chunks()`) lands with `for` loops post-MVP — today's binding yields the final validated value; replay consumes the recorded final value like a plain call.
>
> **Changelog v1.7:** hybrid replay landed (v0.2 first item). §6.2 concrete MVP mechanism: `NUDGE_REPLAY_MODE=all` (default, full replay) now also **mocks tool calls from the trace** — per-tool recorded outputs, `[]` when the trace holds none; `NUDGE_REPLAY_MODE=llm` is the hybrid mode — LLM calls replay, tools execute live and are traced, so tool drift is visible by diffing traces. §6.1: `tool.call` records (`tool`, `input`, `output`) are now emitted in live and hybrid runs; trace emission is serialized under a lock so `par` branches can never interleave records or duplicate `seq` numbers. Codegen: tool stubs now receive their argument list (`rt.tool_stub("name", [args])`) so `tool.call` records carry real inputs.
>
> **Changelog v1.6:** budget enforcement + the parallel scheduler landed (roadmap day 11–12). §4.3 concrete MVP semantics: the fake provider charges a flat, deterministic **$0.001 per call** (every repair round charges; replay charges nothing); `NUDGE_BUDGET=<usd>` is the env form of `nudge run --budget` and arms a run-level counter shared by all `par` branches — a precheck refuses to start a call whose inherited budget is already gone, and a post-call charge raises `BudgetExceeded` when spent crosses the limit (the trace stays complete up to the crash point). Money literals now keep their unit through the compiler, so a non-USD budget is **E0501** (USD only in v0.1). §5 MVP scheduler: `par map` runs on a thread pool (`concurrency` or `min(32, n)` workers), results keep input order, and zip pair-unpacking spreads tuple elements across multi-parameter lambdas; `par race` takes the first completed branch and cancels the losers best-effort (budget refunds post-MVP).
>
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

MVP mechanism (v1.6): the fake provider prices every call at a flat **$0.001** (deterministic, not a model price — it exists so budget walls are testable at zero token cost; every repair round charges, replay charges nothing). `NUDGE_BUDGET=<usd>` arms the run-level counter, which is shared across `par` branches under a lock. A call whose inherited budget is already exhausted never starts (`_budget_precheck`); after each call the counter is charged (`_budget_charge`: per-call `budget:` wall first, then the run counter) and crossing the limit raises `BudgetExceeded`. Static cost reporting is deferred — the checker only validates the budget unit (non-USD → **E0501**).

### 4.4 Model Routing

```
model: route{
    cheap:  "openai:gpt-4.5-mini"  when confidence_not_needed,
    strong: "anthropic:sonnet-4.6" otherwise,
}
```

If routing is statically resolvable, the compiler reports cost ranges for both branches. User-defined routing functions are deferred (see Open Decisions).

MVP mechanism (v1.12): `route{…}` is an expression of type `string`, usable anywhere a model string is expected (typically `with { model: … }`). Each arm is `label: "model" when <bool expr>` or `label: "model" otherwise`; arms evaluate top-down and the first truthy condition wins. Exactly one `otherwise` arm is required — **E0702** otherwise (no model is chosen when every `when` is false), and a non-bool `when` condition is **E0201**. Codegen lowers to `rt.route((label, model, lambda: cond), …, (label, model, None))` (TS: `rt.route([label, model, () => cond], …, [label, model, null])`); the runtime threads the winning label to the next `llm.call` record as the additive `route` field (§6). `route` is contextual — only special directly before `{` — so it remains a usable identifier. Static per-branch cost ranges stay deferred.

### 4.5 Streaming

```
stream let report: Report = llm""" ... """ with { schema: Report }
for chunk in report.chunks() { ui.render(chunk) }   // chunk: Partial[Report]
```

Schema validation runs **incrementally** over partial JSON; a prefix that can no longer satisfy the schema aborts the stream early and triggers repair.

MVP mechanism (v1.8): `stream let` lowers to `rt.llm_stream`. The fake provider streams its answer in deterministic 14-character chunks; each prefix is checked by `_PrefixValidator` (wrong-type literal start, number out of range, invalid `uri`, object closing without a `required` key, malformed JSON ⇒ `_PrefixImpossible`). An abort counts as a schema violation — the §4.2 repair loop applies unchanged — and is traced with `early_abort: true` plus the consumed `chunks` count. `stream` is a contextual keyword (§12): only special directly before `let`, and `stream let` on a non-LLM binding degrades to a plain `let` with a codegen warning. `for chunk in report.chunks()` lands with `for` loops post-MVP.

---

## 4.6. Providers — fake, replay, and real (v1.14)

| Provider | Selected by | Cost | Use |
|---|---|---|---|
| `fake` | default | flat $0.001/call | offline dev, tests, replay |
| `replay` | `NUDGE_REPLAY=<trace>` | $0 | test blocks, CI |
| `openai` / `gemini` / `groq` / `ollama` | model prefix (`gemini:gemini-2.5-flash`) or `NUDGE_PROVIDER` | pricing table; free/local = $0 | real runs |

Mechanism: one OpenAI-compatible HTTP adapter (stdlib `urllib`, no deps). `NUDGE_BASE_URL` overrides the endpoint (proxies, local Ollama); keys come from `NUDGE_API_KEY` or provider-specific envs (`OPENAI_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`). Schema'd calls JSON-extract the answer (``` fences → first balanced span → raw text, which fails validation and enters the §4.2 repair loop). Trace records carry the real provider name, real usage tokens, and priced `cost_usd` — every additive-free v1 field. Streaming falls back to non-streaming against real providers; TS is fake/replay-only until async codegen. Unknown `NUDGE_PROVIDER` values fail fast. Conformance: mock-server e2e in the compiler suite + the manual `provider-smoke` GitHub workflow against the Gemini free tier.

## 5. Parallelism

Concurrency primitives sit on top of the effect system; data races are compile errors.

```
let results = par map plan.steps |s| -> execute(s)        // fan-out
let (a, b)  = par all (fetch_x(), fetch_y())              // barrier
let fastest = par race [ask_a(q), ask_b(q)]               // first wins; losers cancelled, budgets refunded
let done    = par map(tasks, concurrency = 8) |t| -> run(t)
```

Compiler guarantee: no shared mutable state inside `par` (mutable state exists only in `state` blocks, §7). Two parallel branches writing the same `state` field is error E0402 unless that field declares a `merge` reducer.

MVP scheduler (v1.6): `par map` runs on a thread pool with `concurrency` workers (default `min(32, n)`), and results keep input order. A lambda with more than one parameter spreads tuple elements (e.g. from `zip`) across its parameters. `par all` is a barrier over thunks; `par race` returns the first completed branch and cancels the losers best-effort — a call already in flight keeps its spend (budget refunds are post-MVP). All branches share the run budget counter (§4.3), so a wall hit surfaces as `BudgetExceeded` from an in-flight branch. The E0402 shared-state check is deferred (no `state` blocks at MVP).

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

**Tool records (v1.7):** `tool.call` records (`tool`, `input`, `output`) are emitted in live and hybrid runs (never in full replay — tools are mocked there, §6.2). Trace emission is serialized under a lock, so `par` branches cannot interleave records or duplicate `seq` numbers.

**Additive fields (v1.8):** streamed `llm.call` records carry `streamed: true`, `chunks` (consumed chunk count), and `early_abort: true` on attempts aborted by incremental validation (§4.5).

**Additive fields (v1.12):** when the model string comes from a `route{}` block (§4.4), the `llm.call` record carries `route: "<label>"` — the chosen arm's label.

**Frozen schema (v1.13):** the v1 record schema is now frozen — kinds `llm.call` / `tool.call` / `fn.return` with their required fields (`llm.call`: `model`, `params`, `input`, `output`, `tokens`, `cost_usd`, `repair_round`, `outcome`, `provider`; `tool.call`: `tool`, `input`, `output`; `fn.return`: `fn`, `output`) plus every field above. New fields may only be additive; removals/renames require a `v: 2` schema and `nudge trace migrate`. `nudgec trace-check <trace.jsonl>` validates conformance: JSON-per-line, `v: 1` (E0601 otherwise), sequential `seq`, per-kind required fields.

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

Hybrid mode (v1.7): `NUDGE_REPLAY_MODE=all` (default) additionally mocks tool calls from the trace — per-tool recorded outputs in call order, `[]` when the trace holds none — and writes no `tool.call` records. `NUDGE_REPLAY_MODE=llm` is the hybrid: LLM calls replay as above while tools execute live and emit fresh `tool.call` records, so tool drift shows up as a trace diff. Live runs (no `NUDGE_REPLAY`) record both `llm.call` and `tool.call`.

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

MVP mechanism (v1.9): an `agent` block holds one `state` section (typed fields with defaults) and plain `fn` members. State writes are the statements `state.x = v` and `state.x += v` (a 4-token parser lookahead keeps bare `state.x` reads expressions); `=` values are type-checked against the declared field and unknown fields are **E0701**, as is any state write outside an `agent` block. Codegen binds each agent's state to `_state_<Agent> = rt.AgentState("<Agent>", {defaults})`; every attribute write persists the full state to `.nudge/runs/<run_id>/checkpoint.json` (JSON — SQLite/Postgres land post-MVP), and the run directory registers `program` (the emitted entry file) and `trace`. `nudge resume <run_id>` re-executes the program with `NUDGE_RESUME=1` replaying the recorded trace: LLM/tool calls consume the recorded prefix, then go live and append to the same trace; the first `writes` state writes of the re-execution are suppressed because the checkpoint already reflects them (deterministic re-execution reproduces the identical write sequence). The `| merge` reducer landed in v1.10 (`rt.merge`: dict union, list append-dedup); the E0402 shared-state check and parallel state writes remain deferred.

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

MVP mechanism (v1.13): `nudgec a2a <file.ndg>` emits A2A agent cards to `out/<name>.agent.json` — one card per `agent` block, or a single card wrapping the top-level fns when the file declares none. A card carries `name`, `description`, `version` (`1.0.0`), `url` (placeholder), `capabilities` (`stateTransitionHistory: true` — checkpoint/resume; streaming/push off), `defaultInputModes`/`defaultOutputModes` (`["text"]`), and `skills`: one per fn, with the typed signature as the description and the fn's effects as tags. Cards validate as plain JSON. The `export agent … at a2a://…` syntax and serving cards over HTTP land post-v1.0.

## 10. Compiler and Runtime Architecture

```
.ndg source
   │
   ▼
Lexer + Parser (hand-rolled, zero dependencies at MVP)
   │
   ▼
AST → HIR (desugar: llm""" """ → llm_call node)
   │
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

**LSP (v1.13):** `nudgec lsp` serves the Language Server Protocol over stdio — dependency-free JSON-RPC with Content-Length framing. Implemented: `initialize` (full-document sync), `textDocument/didOpen`/`didChange`/`didClose` → `publishDiagnostics` backed by the real lex→parse→check pipeline (stable E-codes attached; lexer/parser diagnostics point at the exact line/character, checker diagnostics point at file start until the spanned AST lands), `shutdown`/`exit`; unimplemented requests receive `-32601`. Hover/completion/code-actions land post-v1.0.

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
| E0701 | state write outside an agent block / unknown state field | `state.round = 1` in a plain fn |
| E0702 | `route{}` block without an `otherwise` arm | every `when` false ⇒ no model chosen |

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

Shipped at v1.0: `nudgec trace-check <trace.jsonl>` (validate against the frozen v1 schema, §6) · `nudgec a2a <file.ndg>` (emit agent cards, §9) · `nudgec lsp` (stdio language server, §10).

Exit codes: `0` ok · `1` compile/runtime error · `2` budget exceeded · `3` replay mismatch.

`nudge cost` mechanism (v1.12): static — the compiler walks each fn's AST and counts llm call sites at flat fake pricing ($0.001/call). Each fn gets a line (`name: N llm call site(s), min $X, max $Y`) plus a `total` line; `retry: N with repair` raises the worst case to 1+N calls (retry without repair does not multiply), and sites inside `par map` bodies are marked `(× collection size inside par map — runtime-dependent)`. Test blocks replay recorded traces and count as zero live cost. Real provider pricing lands post-MVP.

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

stream let p: Plan = llm"""...""" →     p = rt.llm_stream(prompt=…, schema=…)
                                        # chunks validated incrementally (§4.5)

state.x += v                      →     _state_<Agent>.x += v   # AgentState store checkpoints (§7)
l | merge r                       →     rt.merge(l, r)           # reducer join (§7)

route{ c: "m1" when b,            →     rt.route(("c", "m1", lambda: b),
       s: "m2" otherwise }              ("s", "m2", None))      # model routing (§4.4)

; TypeScript backend (v1.11): the same items emit `./nudge_runtime.ts` calls —
; rt.llmCall({...}) / rt.toolStub(name, args, {server}) / rt.merge(l, r)
; / rt.route([label, model, () => cond], …) (v1.12).

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

## 20. Prompt Quality Lints (Prompt Clippy)

Non-fatal **W-code** warnings emitted on `nudgec check` / `build` / `build-ts`
(stderr; the build never fails on a warning). Rationale: prompt engineering
is a compiler concern — the schema and budget are declared in the language,
so the language can check the prompt against them.

- **W0001 no-budget** — an `llm` call with no `budget` option in its
  with-block (uncapped cost).
- **W0002 vague-prompt** — prompt body under 4 words, counting
  `{interpolation}` holes as zero words.
- **W0003 schema-silence** — a record `schema: T` (declared in-file) whose
  field names never appear in the prompt text (word-boundary matching; the
  warning suggests telling the model the output contract explicitly).
- **W0004 schema-without-repair** — `schema` set but no `retry: N with
  repair`: a validation failure raises at runtime instead of entering the
  repair loop.

Every warning carries its context (`in fn name`, `agent X / fn f`,
`test "name"`). Identical warnings collapse into one line with a `×N`
repetition count. W0004 applies to `stream let` too —
streaming shares the §4.2 repair loop (an early abort counts as a
violation). Lints also surface in the editor: the LSP server attaches
them as severity-2 diagnostics to otherwise-clean files (positioned at the
file start until the spanned AST lands, same caveat as check diagnostics).

Out of scope (for now): cross-file schema lookup, severity configuration,
`allow(w0002)` attributes. Conformance: unit tests in `lint.rs` cover all
four rules, word-boundary matching, context strings, and the
interpolation-word-count rule; an LSP test covers editor surfacing.
