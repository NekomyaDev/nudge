# Changelog

All notable changes to Nudge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- **Output guards (C6)**: `NUDGE_GUARD=pii` masks secrets (`sk-…`/`ghp_…`/`AKIA…`), emails, IP addresses and long digit runs in every string an LLM produced — applied before the value reaches the program and before the trace record is written, with the additive `guard: [...]` field recording what ran (both backends)
- **Tool capability grants (C5, first v1.5 'safe' increment)**: `NUDGE_TOOL_GRANTS` (JSON) enforces an execution-layer policy — keys are tool names, `server/tool`, `server/*` or `*`; values are glob rules. Ungranted calls raise `ToolDenied` and the denial lands in the trace (`outcome: "denied"`); a present policy fails closed; no policy = today's behavior. Injected instructions cannot invoke ungranted tools because the runtime refuses — not because a prompt asked (both backends)
- **Transient-error retry with backoff (C4)**: all provider transports share one `_urlopen_retry` loop — 429 **and 5xx** back off exponentially (`NUDGE_BACKOFF_BASE`, default 5s → 25s → 125s) up to `NUDGE_RETRY_TRANSIENT` (default 3, `0` disables); 4xx and unreachable hosts fail immediately; previously only 429 was retried, openai/anthropic/SSE each had a private copy
- **`NUDGE_PRICING` (C3)**: user-supplied pricing table for real providers — JSON `{"model": [usd_per_1M_in, usd_per_1M_out]}`; extends the built-in table so new models record real costs instead of `$0` + W9001; invalid JSON is ignored with a warning (Python runtime; TS runtime is fake-only today)
- **`nudgec compare <results_a.jsonl> <results_b.jsonl>`**: two eval runs side by side — accuracy delta plus the rows that improved or regressed (the "what changed when I switched models?" report)
- **`nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8]`**: score a program over a dataset — per-row pass/fail against expected values (dotted path extraction for records), accuracy report, failure listing with actual outputs, and a `--min-accuracy` CI gate (exit 1 below threshold); runs with whatever provider is configured, fake for $0
- **Version compatibility (B6)**: generated programs now stamp the real nudgec version (was hardcoded `0.1.0`) and call a guarded compatibility hook — `rt.require_runtime("x.y.z")` (Python, via `hasattr`) / `rt.compatibilityCheck?.("x.y.z")` (TS, optional call). Runtimes older than the compiler warn loudly ("some features may be missing; upgrade") instead of failing subtly; old runtimes without the hook skip it silently
- **`nudgec check <file.ndg> --watch`**: re-check on every save — a dependency-free 300 ms mtime poll that spawns the real `nudgec check` per change, so flags, lints and error hints stay identical to a plain run; Ctrl-C stops
- **`nudgec fmt <file.ndg> [--check]`**: safe formatter — re-indents from the token stream's brace depth, trims trailing whitespace, collapses blank-line runs; never reorders or re-flows code, keeps `llm"""` bodies verbatim; idempotent, and `--check` exits 1 for CI
- **`nudgec trace-html <trace.jsonl> [--out file.html]`**: exports the trace viewer as a static single-file HTML — trace data inlined as a JS string, no server, no network, no external assets; open it anywhere (e-mail, PR review, docs)
- **`nudgec explain <trace.jsonl>`**: human report over a recorded run — totals (calls, tokens, cost, decision latency, cache hits, repairs) plus the records a human should look at: failures, deadline misses and the weakest typed answers under 0.5; an explicit "nothing to review" line when the run is clean
- **docs/recipes/ — the recipe book**: six copy-pasteable patterns (typed extraction, fallback-model routing, human escalation, injection guard, cost-capped call with repair, par-map fan-out); every recipe is type-checked by a compiler test so the book can never drift from the grammar
- **Human compiler errors**: every E-code now prints a one-line plain-language `hint:` under the message, and unknown identifiers/types/fields get `did you mean 'x'?` suggestions (prefix-aware edit distance) — the compiler helps instead of scolding
- **`nudgec learn [lesson]`**: the language in six terminal lessons (hello → types → decide{} → route{} → $0 tests → traces/replay) — every lesson program is type-checked by the test suite against the compiler shipping it, so lessons can never drift from the grammar
- **`nudgec init <name> [--template <t>] [--force]`**: scaffold a project from the repo's own examples (hello, triage, chatbot, classifier, code-reviewer, data-analyzer, rag-agent, research-agent, translator, property-fuzz) — writes `<name>.ndg` + a README with the check/build/test/run loop; `--list` browses templates
- **Batch multi-state decisions: `rt.predict_batch(questions, states, opts)` (both backends)** — one transport call for a whole list of states: Valen runs a single subprocess over multi-record JSONL (model-load amortized), HTTP fans out per-state requests with bounded concurrency; the decision cache is consulted per state; returns answers in input order and writes one `decision.call` record per state with additive `batch: {size, wall_ms}` (per-record `latency_ms` is the batch average)
- **Valen adapter (subprocess JSONL transport, both backends)**: `NUDGE_DECISION_SERVERS='{"valen": {"command": "python -m valen.inference --checkpoint ..."}}'` — one-record JSONL per decide call per the documented contract, `targets.*.probabilities` validated with the same adapter rules as HTTP (exact criteria coverage, NaN/range rejection, explicit renormalization); noul maps to a yes/no vote, score candidates map to rubric indices; validated e2e against a mock `valen.inference`
- **Decision cache** (`NUDGE_DECISION_CACHE=<path>`): persists validated answers for real providers across runs — same state + question shape + model = same decision, zero calls; replay always takes precedence; served records carry the additive NTF field `cache: "hit"`; missing/corrupt cache files are cold starts, never errors (both backends). TS runtime now also writes `decision.call` records on the live HTTP path (parity with Python).
- `nudgec policy-sweep <trace.jsonl> --question <q> [--metric confidence|p] [--thresholds 0.5,0.8]`: re-cuts decision thresholds over recorded distributions — the "change a threshold, see the effect before deploy" loop with zero model calls
- examples/triage-agent: batched decide{} + route{} confidence policy; examples CI matrix extended
- **NTF `decision.call` records**: batched decisions land in traces (`model/provider/questions/answers/latency_ms/outcome` + additive `deadline_ms`, `level`); full replay consumes them in order with strict exhaustion (llm parity); `trace-diff` reports decision counts + total latency and `--fail-on-regression` gates latency growth and decision failures; conformance corpus extended to 25 cases
- **`decide{}` — typed decisions as a language primitive (v1.4 "Decision", design §11 / `docs/decision.md`)**: `decide { q: "prompt" choose [..] / yes/no / score [..] } on <state> with { model, deadline }` compiles to one batched call against a JEV-family decision model (Laya / Jev share the `/v1/systemone` wire contract); the fake provider synthesizes deterministic seeded distributions so tests stay $0; answers carry winner/p/distribution/confidence (+ additive `level`, `deadline_missed`); new `Decision` effect with full inference/purity support (E0806/E0807, W0005 option-count lint)
- `route{}` generalization: arm values are lazy expressions — bare strings keep model-routing semantics, any other value makes route a value-level policy switch (confidence thresholds)
- deadline semantics: soft `deadline_missed` annotation by default, `NUDGE_DECISION_STRICT=1` makes overruns fatal; `NUDGE_DECISION_SERVERS` registry for live HTTP decision providers

### Added (pre-1.4)
- **Property-based agent tests** (design §6.4): `for_all x in gen.int/str/injection/bool { ... }` inside test blocks — deterministic seeded case sweep, shrink-to-minimal-counterexample, and checker rules E0801 (test-only), E0802 (generator validity), E0804 (property purity: no llm/tool/effectful calls). Python + TypeScript runtimes ship `rt.for_all`/`rt.forAll` with identical semantics; new example `examples/property-fuzz`
- **NTF open standard**: `docs/ntf-spec.md` (frozen v1 spec page), a conformance corpus (`conformance/` — 17 cases with expected verdicts, wired into `cargo test` and a vendor-neutral `conformance/run.py` runner), and a LangChain → NTF bridge (`bridges/langchain_ntf.py`)
- **`nudge-ci` GitHub Action** (`action.yml`): agent regression CI in any repo — `nudgec check` + `nudgec test` (replay, $0) over a glob of `.ndg` programs, with optional `trace-diff` regression gate and `version`/`check-only` inputs; the repository now dogfoods it via `.github/workflows/agent-ci.yml`
- `nudgec trace-diff a b --fail-on-regression`: exits 1 when the candidate trace costs more, uses more tokens, performs more repair rounds, or turns a passing llm call into a failure — turning trace diffing into a CI gate
- `install.sh`: `NUDGE_VERSION` environment variable pins an exact release (or `main`) instead of the per-platform default

## [1.2.1] - 2026-09-26

### Open source
- **Nudge is now open source.** The compiler (`crates/nudgec`), the bytecode VM experiment (`crates/nudge-runtime`), the Python runtime (`runtime/nudge_runtime`), its TypeScript port (`runtime/nudge_runtime.ts`), and the VS Code extension source (`editors/vscode/`) now live in this repository
- Language design documentation: `docs/design.md` (language spec) and `docs/roadmap.md`
- Additional example agents: `hello_llm.ndg`, `checkpoint_agent.ndg`, `smoke_provider.ndg`, `research_agent.ndg` (with replay fixtures under `examples/traces/`)
- Rust CI workflow (`cargo test` + `rustfmt` + `clippy -D warnings`) and the tag-driven release workflow moved into this repository
- License changed from proprietary (closed source, binaries under a separate distribution license) to **Apache-2.0** — the repository and the distributed binaries now share the same license

### Added
- `nudgec --version` / `-V` prints the version and exits 0 (previously printed the usage banner and exited 64)
- Named-argument MCP calls: tool stubs now send MCP `tools/call` arguments as an object keyed by parameter name, so FastMCP-style servers work out of the box (previously a positional `{"args": [...]}` list that most servers reject)
- Live tool results support `.field` access (AttrDict-wrapped), so branching on a tool result works
- Map types: `{string: T}` lowers to `{type: object, additionalProperties: T}` and accepts any field access in the checker
- README now documents the MCP transport contract and the full `NUDGE_*` environment variable reference

### Fixed
- Replay record index is reserved atomically under the replay lock — `par` fan-out no longer double-consumes records
- Docker image builds `nudgec` from source and vendors the Python runtime from `runtime/` (the broken `COPY runtime/` step is gone)

### Notes
- Linux release assets and the Docker image (`nekomyadev/nudge:1.2.1`, `:latest`) ship the new compiler; the macOS/Windows tarballs on this release are unchanged from v1.2.0 and will be refreshed in a later release. macOS/Windows users can use Docker or `pip install nudge-runtime` meanwhile.
- PyPI: `nudge-runtime` 1.1.0

## [1.2.0] - 2026-07-29

### Added
- Trace viewer: `nudgec trace-view <trace.jsonl>` opens a local web UI
- Trace diff: `nudgec trace-diff a.jsonl b.jsonl` compares two traces
- DAP support: `nudgec debug <trace.jsonl>` for trace debugging
- VS Code extension on Marketplace
- Real providers: OpenAI, Gemini, Groq, MiMo, Mistral, Anthropic, Ollama
- Streaming with early-abort schema validation
- MCP stdio transport
- LSP hover, definition, completion
- Prompt Clippy linter (W0001-W0004)
- Tag-driven release workflow with prebuilt binaries

### Changed
- Design frozen at v1.24
- Trace format frozen at v1

## [1.1.0] - 2026-07-20

### Added
- Real provider support (OpenAI-compatible)
- Binary distribution (Linux, macOS, Windows)
- VS Code extension (syntax highlighting, snippets)
- LSP server (`nudgec lsp`)
- A2A agent-card export

## [1.0.0] - 2026-07-15

### Added
- Initial release
- Lexer, parser, type checker, codegen
- Python and TypeScript backends
- Trace and replay system
- Budget enforcement
- Parallel execution (par map/race/all)
- Agent state and checkpoint/resume
- Effect system
- MCP interop
