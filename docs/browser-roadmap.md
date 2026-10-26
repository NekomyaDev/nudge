# Nudge Browser Engine — Multi-Agent Execution Roadmap

**Target Version:** Nudge v1.6 (Flagship Track)
**Parent Specification:** [`docs/browser-use.md`](browser-use.md)
**Coordination Model:** Modular, Swarm-Friendly, Agent-Isolated Work Units
**Global Invariant:** Zero external dependencies; 100% first-party engine logic written in `.ndg` over a minimal kernel.

---

## Agent Coordination & Execution Rules

1. **Isolation of Work Units:** Each task below is strictly scoped to specific files and interfaces. An agent assigned to one task MUST NOT touch files owned by another task without recording an explicit interface dependency.
2. **Replay & Determinism First:** Any runtime or language change must preserve offline deterministic replay (`NUDGE_REPLAY=<trace>` at $0).
3. **No Phantom Syntax:** Every `.ndg` code artifact must parse and evaluate using verified Nudge grammar (`route`, `decide`, `Header` records, `schema = T`).
4. **Task Lifecycle Protocol:**
   - `[ ] PENDING`: Awaiting prerequisites.
   - `[>] IN PROGRESS`: Actively locked by an agent.
   - `[X] COMPLETED`: Merged, tests green, conformance validated.
   - `[!] BLOCKED`: Awaiting upstream contract resolution.

---

## Phase F1: Kernel Transport & Effect Registration (Active Phase)
**Goal:** Expose raw byte transfer (`browser.fetch`), register the `Browser` effect, and add compile-time safety lints.

- [X] **Task F1.1 — Effect & AST Registration**
  - **Owner / Agent:** `agent:compiler-core`
  - **Files:** `crates/nudgec/src/check.rs`, `crates/nudgec/src/ast.rs`, `docs/ntf-spec.md`
  - **Contract:**
    - Expand `KNOWN_EFFECTS` from 12 to 13 (`"Browser"`).
    - Register `"browser"` in `RESERVED_BINDINGS` so user variables cannot shadow the namespace.
    - Add NTF trace record specifications for `browser.snapshot` and `browser.act`.
  - **Acceptance Gate:** `cargo test -p nudgec test_known_effects` passes. (PASSED: 580/580 tests green)

- [X] **Task F1.2 — VM Runtime Kernel Primitive (`browser.fetch`)**
  - **Owner / Agent:** `agent:runtime-vm`
  - **Files:** `crates/nudge-runtime/src/stdlib/browser.rs`, `crates/nudge-runtime/src/stdlib/mod.rs`, `crates/nudgec/src/eval/interpreter.rs`
  - **Contract:**
    - Expose native `browser.fetch(url, opts) -> { status: int, headers: [Header], body: string }`.
    - Reuse existing TLS stack (no new dependencies or crates).
    - Enforce fail-closed TLS verification and `max_body_bytes = 10 MB` streaming limit with `BodyTooLarge` error.
    - Hook interpreter dotted field resolution for `browser.*`.
  - **Acceptance Gate:** Unit test fetching local fixture server over HTTP/HTTPS/file returns valid status and body. (PASSED: unit & e2e green)

- [ ] **Task F1.3 — Standalone Target Parity (Python & TypeScript)**
  - **Owner / Agent:** `agent:codegen`
  - **Files:** `crates/nudgec/src/codegen.rs`, `crates/nudgec/src/codegen_ts.rs`
  - **Contract:**
    - Python target: Emit kernel `browser.fetch` mapping to standard library `urllib`/`ssl` (zero third-party packages).
    - TypeScript target: Emit kernel `browser.fetch` mapping to standard `fetch` API.
  - **Acceptance Gate:** Standalone build tests (`nudgec build --target python` & `ts`) pass fixture checks.

- [X] **Task F1.4 — Safety Lint W0008 (Browser Unscoped)**
  - **Owner / Agent:** `agent:compiler-lint`
  - **Files:** `crates/nudgec/src/lint.rs`
  - **Contract:**
    - Implement `W0008`: Warn at every `browser.*` network call lacking an `allow` domain pattern list.
    - Warn if `allow: []` is passed (empty list denies everything).
    - Update `W0007` to warn if `browser.*` calls occur within `par race` branches under Python target.
  - **Acceptance Gate:** `cargo test -p nudgec lint` validates W0008 triggers and fixes. (PASSED: 16/16 tests green)

---

## Phase F2: Pure `.ndg` Codecs & Network Wire Framing
**Goal:** Implement essential wire framing and parsing entirely in `.ndg` source code.

- [ ] **Task F2.1 — Deterministic Content Hashing & Base64**
  - **Owner / Agent:** `agent:ndg-codecs`
  - **Files:** `prelude/codec/hash.ndg`, `prelude/codec/b64.ndg`
  - **Contract:**
    - `codec/hash.ndg`: djb2-family modular arithmetic over integers for deterministic `state_id` generation.
    - `codec/b64.ndg`: Pure scalar table-based base64 encoder and decoder.
  - **Acceptance Gate:** Conformance test matching known hashes and RFC 4648 test vectors.

- [ ] **Task F2.2 — JSON Tokenizer & Parser**
  - **Owner / Agent:** `agent:ndg-json`
  - **Files:** `prelude/codec/json.ndg`
  - **Contract:**
    - Recursive-descent parser and serializer for nested objects, arrays, strings, numbers, and booleans.
    - Maximum recursion depth limit = 64 frames to prevent stack exhaustion.
  - **Acceptance Gate:** JSON round-trip tests on complex payloads pass in interpreter.

- [ ] **Task F2.3 — URL Parsing & Normalization**
  - **Owner / Agent:** `agent:ndg-net`
  - **Files:** `prelude/net/url.ndg`
  - **Contract:**
    - Scheme, host, port, path, query, and fragment decomposition.
    - Relative URL resolution against a base document URL.
  - **Acceptance Gate:** WhatWG URL resolution test vector subset passes.

- [ ] **Task F2.4 — HTTP/1.1 Message Framing**
  - **Owner / Agent:** `agent:ndg-net`
  - **Files:** `prelude/net/httpc.ndg`
  - **Contract:**
    - Build canonical HTTP/1.1 requests with deterministic header ordering.
    - Parse HTTP response status line, headers into `[Header]`, handle `Content-Length` and chunked transfer decoding.
  - **Acceptance Gate:** Mock socket HTTP/1.1 message parsing succeeds.

---

## Phase F3: In-House DOM, Tree Construction & Policy Engine
**Goal:** Parse HTML into a token-efficient accessibility tree and enforce strict navigation policies.

- [ ] **Task F3.1 — HTML Tokenizer & DOM Builder**
  - **Owner / Agent:** `agent:ndg-html`
  - **Files:** `prelude/web/html.ndg`, `prelude/web/dom.ndg`
  - **Contract:**
    - Parse HTML5 tokens (elements, attributes, text, comments, self-closing tags, character entities).
    - Construct lightweight DOM tree node records with sequential integer `ref` assignment.
  - **Acceptance Gate:** Parses real-world HTML pages into DOM trees within latency limits (< 5 ms for < 100 KB).

- [ ] **Task F3.2 — CSS Selector Query Engine**
  - **Owner / Agent:** `agent:ndg-dom`
  - **Files:** `prelude/web/dom.ndg`
  - **Contract:**
    - Support tag names (`div`), classes (`.btn`), IDs (`#main`), attribute selectors (`[name="q"]`), and compound descendants.
  - **Acceptance Gate:** `browser.query(page, ".item")` accurately returns matching `[Element]` array.

- [ ] **Task F3.3 — Security & Scope Policy Engine**
  - **Owner / Agent:** `agent:ndg-policy`
  - **Files:** `prelude/web/policy.ndg`
  - **Contract:**
    - Validate every request and subresource against `allow: [DomainPattern]`.
    - Enforce scheme restrictions (`http`, `https`, `file:`); confine `file:` strictly to workspace path.
    - Enforce redirect hop ceiling (max 20 hops).
  - **Acceptance Gate:** Unauthorized cross-domain fetch triggers `BrowserDenied` fail-closed.

- [ ] **Task F3.4 — Snapshot & Token-Efficient Tree Formatter**
  - **Owner / Agent:** `agent:ndg-snapshot`
  - **Files:** `prelude/web/snapshot.ndg`
  - **Contract:**
    - Render DOM into indented, token-efficient text representation readable by LLMs (role, name, ref, value).
    - Generate monotonic `state_id` (`s-1`, `s-2`) bound to `(url, content_hash)`.
  - **Acceptance Gate:** Snapshot output matches compact representation fixture.

---

## Phase F4: Interaction, Forms, Cookies & Compound Primitives
**Goal:** Enable active user simulation (clicks, typing, forms) with strict one-round-trip compound contracts.

- [ ] **Task F4.1 — Cookie Jar & Profile Isolation**
  - **Owner / Agent:** `agent:ndg-cookie`
  - **Files:** `prelude/web/cookie.ndg`, `prelude/web/profile.ndg`
  - **Contract:**
    - Parse `Set-Cookie` headers; match domain, path, and expiry.
    - Persist cookies and header signatures in Nudge `Store` with quota protection.
  - **Acceptance Gate:** Profile-isolated cookie sessions survive reloads.

- [ ] **Task F4.2 — Form State, Encoding & Submissions**
  - **Owner / Agent:** `agent:ndg-form`
  - **Files:** `prelude/web/form.ndg`
  - **Contract:**
    - `browser.type(page, target, text)` updates field values in-memory within active observation epoch.
    - `browser.fill(page, fields, submit?)` applies up to 20 field inputs in one compound call.
    - `browser.submit(page, form)` serializes fields (`application/x-www-form-urlencoded` / multipart) and executes action.
  - **Acceptance Gate:** Multi-field form submission successfully reaches test endpoint.

- [ ] **Task F4.3 — Compound Actions & Wait Primitives**
  - **Owner / Agent:** `agent:ndg-action`
  - **Files:** `prelude/web/action.ndg`
  - **Contract:**
    - `browser.clicks(page, targets)` executes up to 10 sequential clicks in a single compound operation.
    - `browser.wait(page, target)` executes deadline-bounded re-fetches and returns `WaitResult` containing the final `Snapshot`.
  - **Acceptance Gate:** Meets C1 (4 kernel commands for goto-click-fill-click) and C2 compound requirements.

---

## Phase F5: Trace Journaling, $0 Replay & Scheduled Automation
**Goal:** Complete the compounding loop: live execution recording, offline zero-cost replay, and drift detection.

- [ ] **Task F5.1 — NTF Trace Emission & Sidecar Asset Management**
  - **Owner / Agent:** `agent:trace-journal`
  - **Files:** `prelude/web/journal.ndg`, `crates/nudgec/src/eval/interpreter.rs`
  - **Contract:**
    - Emit valid `browser.snapshot` and `browser.act` NTF records.
    - Store screenshots in `<trace>.assets/<hash>.png`, referencing via `screenshot_asset` (never inline in JSONL).
  - **Acceptance Gate:** `nudgec trace-check` validates browser trace conformance.

- [ ] **Task F5.2 — Offline Deterministic $0 Replay Engine**
  - **Owner / Agent:** `agent:replay-engine`
  - **Files:** `prelude/web/journal.ndg`, `crates/nudge-runtime/src/provider/http.rs`
  - **Contract:**
    - Under `NUDGE_REPLAY=<trace>`, intercept all browser calls: serve snapshots from trace, dry-run actions (`dry_run: true`).
    - Guarantee $0 network cost and zero live provider/LLM invocations.
    - Strict exhaustion: raise `ReplayMismatch` if program deviates from recording.
  - **Acceptance Gate:** Complete browser flow replays with network interfaces disabled in CI.

- [ ] **Task F5.3 — Drift Detection & Scheduled Watcher Pipeline**
  - **Owner / Agent:** `agent:drift-pipeline`
  - **Files:** `prelude/web/drift.ndg`, `examples/browser-agent/`
  - **Contract:**
    - Compute structural diffs between live observations and recorded baselines (`added`, `removed`, `summary`).
    - Provide scheduled execution example reporting `ok | error` without firing side-effecting actions.
  - **Acceptance Gate:** Synthetic HTML modification triggers structured drift alert without firing downstream clicks.

---

## Phase F6: Sandboxed In-Language JavaScript Interpreter
**Goal:** Execute page JavaScript in pure `.ndg` bytecode interpreter to handle modern SPAs and bot challenges.

- [ ] **Task F6.1 — JavaScript Tokenizer & AST Parser**
  - **Owner / Agent:** `agent:js-parser`
  - **Files:** `prelude/web/js/lexer.ndg`, `prelude/web/js/parser.ndg`
  - **Contract:** Parse ES subset (closures, arrays, objects, functions, basic operators).
  - **Acceptance Gate:** Parses standard SPA micro-framework script snippets without syntax errors.

- [ ] **Task F6.2 — Sandboxed Interpreter Runtime & 3-Tier Shield**
  - **Owner / Agent:** `agent:js-runtime`
  - **Files:** `prelude/web/js/eval.ndg`
  - **Contract:**
    - Tier 1: Step budget (hard cap of 50,000 bytecode operations).
    - Tier 2: Deadline timer check.
    - Tier 3: Heap object cap (8 MB equivalent) and call stack depth limit (64 frames).
  - **Acceptance Gate:** Infinite loops (`while(true)`) and recursion bombs terminate cleanly with typed error.

- [ ] **Task F6.3 — DOM & Network Bindings**
  - **Owner / Agent:** `agent:js-bindings`
  - **Files:** `prelude/web/js/dom_bridge.ndg`
  - **Contract:**
    - Implement `document.querySelector`, `addEventListener`, `fetch`/`XMLHttpRequest` hooked through `web/policy.ndg`.
  - **Acceptance Gate:** DOM mutations from script execution reflect in subsequent `browser.snapshot`.

---

## Phase F7: Native Font/UI Rendering & Screenshots
**Goal:** Generate pixel-accurate screenshots using Nudge's first-party UI/font engine.

- [ ] **Task F7.1 — Box Model & Text Layout**
  - **Owner / Agent:** `agent:render-layout`
  - **Files:** `prelude/web/layout.ndg`
  - **Contract:** Calculate inline and block layout geometries, wrapping, and element bounding boxes.
  - **Acceptance Gate:** Layout engine calculates expected bounding box for styled sample pages.

- [ ] **Task F7.2 — Rasterization & Headless Screenshots**
  - **Owner / Agent:** `agent:render-compositor`
  - **Files:** `prelude/web/compositor.ndg`
  - **Contract:**
    - Rasterize layout to memory buffer using Nudge's existing `font_engine` and offscreen surface.
    - Output PNG data to content-addressed sidecar asset and populate `Snapshot.screenshot`.
  - **Acceptance Gate:** `browser.goto(url, screenshot = true)` generates valid PNG viewable on disk.

---

## Phase F8: Low-Level Stream Channels (HTTP/2 & WebSocket)
**Goal:** Expand kernel transport from request/response to raw duplex stream for modern protocols.

- [ ] **Task F8.1 — Kernel Stream Channel**
  - **Owner / Agent:** `agent:runtime-stream`
  - **Files:** `crates/nudge-runtime/src/provider/stream.rs`, `crates/nudgec/src/eval/interpreter.rs`
  - **Contract:** Expose `browser.connect`, `browser.read`, `browser.write`, `browser.close`.
  - **Acceptance Gate:** Bidirectional raw TCP/TLS echo test succeeds.

- [ ] **Task F8.2 — HTTP/2 Framing in `.ndg`**
  - **Owner / Agent:** `agent:ndg-h2`
  - **Files:** `prelude/net/h2.ndg`
  - **Contract:** Implement HTTP/2 connection preface, SETTINGS exchange, and HEADERS/DATA frame multiplexing.
  - **Acceptance Gate:** Fetches web page over HTTP/2 protocol.

- [ ] **Task F8.3 — WebSocket Protocol in `.ndg`**
  - **Owner / Agent:** `agent:ndg-ws`
  - **Files:** `prelude/net/ws.ndg`
  - **Contract:** RFC 6455 handshake (SHA-1 hashing in `.ndg`) and frame masking.
  - **Acceptance Gate:** Completes full echo cycle against a standard WebSocket endpoint.

---

## Phase F9: Documentation, Learn Track & Tooling Polish
**Goal:** Publish documentation, recipes, and editor integration.

- [ ] **Task F9.1 — Documentation & Learn Lesson**
  - **Owner / Agent:** `agent:docs`
  - **Files:** `docs/learn/browser-use.md`, `README.md`
  - **Contract:** Expand effect table from 12 to 13; write beginner-to-advanced browser agent tutorial.

- [ ] **Task F9.2 — Browser Agent Recipes & CLI Examples**
  - **Owner / Agent:** `agent:recipes`
  - **Files:** `recipes/browser_search.ndg`, `recipes/price_tracker.ndg`
  - **Contract:** Ship ready-to-run recipes demonstrating `goto`, `decide`, and `extract`.

---

## Verification Matrix & Completion Checklist

| Invariant | Verifying Task / Gate | Target Metric |
|:---|:---|:---|
| Zero Third-Party Dependencies | F1.2, F1.3 | 0 external crates, 0 npm/pip packages |
| Command Round-Trip Counting | F4.3 (C1) | Exactly 4 kernel calls for goto-click-fill-click |
| Deterministic $0 Replay | F5.2 (C6) | $0 USD, 0 network bytes in CI replay |
| Neutral Default Locale | F3.3, §7 | `en-US` default, no host leakage |
| JS Sandbox Confinement | F6.2 (C10) | Hard abort at 50k ops / 8 MB heap / 64 frames |
| Streaming Body Protection | F1.2 (C11) | Fail-closed abort at 10 MB limit |
| NTF Specification Parity | F5.1 (D24) | Asset sidecars, zero inline JSONL pixel bloat |
