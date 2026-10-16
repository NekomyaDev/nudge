# Design: Async TS codegen + real model providers (#129)

Status: **PLANNED** (not yet implemented — scoped here so it can be built as a
focused task rather than rushed). Scope: make `llm` calls in the TypeScript
backend reach real model providers (OpenAI-compatible, Anthropic, Gemini), which
requires making the generated TS async.

## Why this is a real feature, not a bug fix

`runtime/nudge_runtime.ts` `llmCall`/`llmStream` are **synchronous** and throw on
real providers ("real providers run on the Python runtime"). Real model HTTP is
inherently async (`await fetch`), so the generated TypeScript must become async.
That is the "async codegen" the finding said the adapter "lands with".

The generated program is currently fully synchronous and its runner invokes
`main()` synchronously (`crates/nudgec/src/codegen_ts.rs` ~line 274:
`const _result = main();`). Making `llm` async therefore ripples through the
compiler.

## Key constraint: async contagion must NOT touch the sync UI surface

The UI tree helpers (`column`, `row`, `text`, `button`, `input`, emitted in the
codegen preamble) and `view` blocks **build trees synchronously**. If they were
naively made async, tree construction would return Promises and break. So async
must spread ONLY to functions that actually reach an `llm` call — the classic
async-effect inference / contagion problem.

## Implementation plan

1. **Runtime (`runtime/nudge_runtime.ts`):**
   - Make `llmCall` / `llmStream` `async` (return `Promise<string>`). Fake and
     replay paths stay deterministic (resolve immediately).
   - Add a `fetch`-based `_realProviderCall(provider, bareModel, prompt)`,
     mirroring the Python `_openai_chat` / `_anthropic_chat`:
     - OpenAI-compatible (`openai`, `gemini` at `/v1beta/openai`, `groq`, `mimo`,
       `ollama`, `mistral`): POST `{base}/chat/completions`,
       `Authorization: Bearer`, parse `choices[0].message.content` + `usage`.
     - Anthropic: POST `{base}/v1/messages`, `x-api-key` +
       `anthropic-version: 2023-06-01`, parse `content[].text` + `usage`.
     - Transient 429/5xx backoff + retry (`NUDGE_RETRY_TRANSIENT`,
       `NUDGE_BACKOFF_BASE`), mirroring Python `_urlopen_retry`.
   - `_realProviderFor(model)` resolves provider from `NUDGE_PROVIDER` / model
     prefix (`_splitModel`), mirroring Python `_real_provider_for`.

2. **Async-effect analysis (`crates/nudgec/src/codegen_ts.rs`):**
   - Compute `async_fns`: least fixed point of
     `{f : f's body contains LlmCall}` ∪ `{f : f calls some g ∈ async_fns}`.
   - Pragmatic detector (avoids writing a full AST walker): emit each function
     body once with `emit_body` (async set empty → no awaits), then test the
     emitted text for `rt.llmCall(` / `rt.llmStream(` (has llm) and for a
     word-boundary call to an already-async user fn. Iterate to a fixed point.
   - Store `async_fns` in a `thread_local!` so `ts()` can consult it without
     threading a new parameter through every recursive call.

3. **Emission:**
   - `ExprKind::LlmCall` (codegen_ts.rs ~570): emit `await rt.llmCall(...)` (and
     `rt.llmStream`).
   - `ExprKind::Call` (~575): emit `await <fn>(...)` when the callee is in
     `async_fns`.
   - Function-emit sites (~89 plain fns, ~169 agent fns): emit `async function`
     and a `Promise<…>` return type when the fn is in `async_fns`. Leave `view`
     blocks, tools, and the UI helpers synchronous.
   - Runner (~274): `const _result = await main();` (requires the emitted file to
     be ESM — the runtime already uses `import`/`export`, so top-level `await`
     is valid).

4. **Tests:** the ~4 assertions that match `function main(): string {` become
   `async function main(): Promise<string> {` for llm-reaching fns. Update
   `runtime/nudge_runtime.test.mjs` calls to `await`. Add a unit test with a
   mocked `fetch` asserting the OpenAI and Anthropic request shapes.

## Risk

The contagion + harness change touches the whole TS backend. Do it as a focused
task with the full gate suite (clippy `--all-features`, `cargo test`, the
node-based TS tests under `--experimental-strip-types`, offline replay gate) and
keep it in one reviewable change. Do NOT rush it into an unrelated cleanup.
