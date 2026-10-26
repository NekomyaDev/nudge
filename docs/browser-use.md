# Nudge v1.6 — Browser Use: the web as a typed, replayable effect

**Status:** Plan (not yet implemented). Phases: §11. Design-review
log: §12.

## 0. Hard constraints

1. **Zero dependencies.** No third-party browser binary, no Node/Python
   bridge, no MCP/REST provider, no external JS engine, no WebSocket or
   JSON library. The distribution is `nudgec` itself; the browser's only
   dependency is the language.
2. **A real, complete browser — ours.** The end state is Nudge's own
   in-house browser engine: real HTTP(S), real HTML/CSS handling,
   real JS execution, real rendering, real TLS. Not an HTTP client
   pretending to be a browser, and not a disguised third-party engine.
3. **Everything in Nudge.** All engine logic — parser, DOM, JS
   interpreter, policy, cookies, forms, journal, drift, layout,
   compositor — is `.ndg`, compiled to native bytecode (self-hosted),
   shipped embedded in the compiler the way recipes and lessons already
   are.
4. **What the language lacks is added to the language.** Byte buffers,
   transport, sockets: kernel-level primitives (§3). Everything above
   the kernel is produced in `.ndg` (§5).
5. **No providers, no fakes.** Real requests, real engine. Tests run
   against `file://` fixtures through the real engine, plus NTF replay.
6. **Neutral default locale.** `en-US`. `tr` — or any host locale — is
   never a default; locale/timezone/device arrive only as explicit
   options (§7).
7. **Completion criteria are measurable** and include the compounding
   loop the browser exists to serve: free replay, scheduled
   watch-and-report flows (§1, §10).

## 1. The compounding loop (why this feature exists)

Browser use is not an isolated effect; it is the missing input side of
Nudge's core promise applied to the web — **every run is a deterministic
$0 test of the next run**:

```
record ──▶ replay ($0) ──▶ schedule ──▶ watch ──▶ report
explore    regression test  timer job    read-only   ok | error
                                        drift check
                                        escalate on change
```

- **Record.** Every live session writes an NTF trace
  (`browser.snapshot` / `browser.act`) with signature hashes.
- **Replay is free.** Any recorded session replays at $0: no network
  bytes, no live LLM/Decision calls, actions dry-run, observations from
  the trace with strict exhaustion. The trace *is* the regression test.
- **Agents learn from traces.** Trees and outcomes are plain data: a
  fresh agent — or the same agent on its next task — can be handed prior
  observations to `decide{}` / `llm"""` at zero cost and thereby knows
  the flow before it ever touches the network. Even an agent that does
  not know the task advances by learning from it.
- **Replay-clean flows get scheduled.** A flow that replays without
  mismatch is promoted to a timer: the program runs in replay + drift
  mode, re-observes the live web **read-only**, diffs against the
  recorded expectation, and its job shrinks to watching and reporting
  `ok | error`. Actions never re-fire.
- **Drift is the only escalation trigger.** No change → no spend.
  Change → budgeted `decide{}` / `llm"""` re-decides, or the report says
  so. For automation builders: confidence (regression), cost control
  (spend only on change), continuity (tomorrow's agent inherits today's
  trace).

Everything in the surface below feeds this loop from day one; the replay
and schedule gates in §10 are hard completion criteria.

## 2. Thesis and division of labor

Nudge already treats uncertainty as an effect: `llm"""` calls a model,
`decide{}` asks a decision engine, `computer.*` observes the machine.
Browser use completes the machine-facing picture: `browser.goto(url)` and
the action primitives make **the web** a typed, traced, replayable
effect.

- **The language owns control.** Every navigation and action flows
  through the runtime: effect inference (`uses Browser`), domain
  scoping (`allow`), deadlines, kill switch, snapshot freshness
  (fail-closed), one NTF record per step. A `pure` function provably
  cannot touch the network at compile time.
- **The model owns intelligence.** What a page means, whether it
  drifted, what to click: perception. The program hands the observation
  (tree) to `llm"""` / `decide{}` and acts on the typed answer.

Capability baseline: the CamoFox MCP surface (47 tools) was examined as
the reference agent-browser. Three lessons are promoted to invariants:

1. **Snapshot-first.** The primary read path is the tree (token-
   efficient, deterministic refs); screenshots come when the renderer
   lands (F7), and never replace the tree.
2. **One call, one round trip.** Its compound tools exist because
   multi-call latency kills agent loops. In Nudge those are contract
   (§10).
3. **Engine ownership is the only complete answer to stealth.**
   Camoufox is complete because it owns the engine. Imitation is not an
   option; neither is hiding behind "we have no engine". Nudge builds
   **its own** real engine — complete by construction, and unknown to
   detector ecosystems by construction (§8).

## 3. The kernel: one channel now, a growth path by design

The language currently has **no byte-level surface at all**: `.ndg`
code cannot reach the network, and TLS cannot be produced in `.ndg`
(cryptography is not closed under scalar arithmetic). The kernel is
therefore the only native surface, and it is deliberately minimal:

**v1 — `browser.fetch(url, opts) -> Response`.** Moves bytes and
nothing else: no redirects, no cookies, no rules (all in `.ndg`, §5).
`opts = { method, headers, body, timeout }`. TLS reuses the runtime's
existing stack — **no new crate**. Carries the `Browser` effect
(`KNOWN_EFFECTS` 12 → 13). Returns `{ status, headers, body }`.

**Growth path (designed now, shipped in F8).** A request/response
primitive cannot carry HTTP/2 or WebSocket, and a real engine needs
both. The kernel therefore grows a **stream channel** —
`browser.connect(host, port, tls, opts) -> conn` +
`browser.read/write/close(conn)` — with framing (h2, ws) implemented in
`.ndg` on top, and the engine switching per profile/ALPN. `fetch`
stays as the simple channel; nothing above the kernel changes shape.

`browser.fetch` is the only native function in the `browser.*` namespace
in v1; everything else resolves to the `.ndg` prelude (the interpreter
already lowers `Field { obj: Ident(prefix), name }` to `"prefix.name"`).
`W0008` (browser-unscoped, sibling of W0006) warns at every `browser.*`
call site without `allow`.

## 4. Language surface

### Methods (frozen v1, all `.ndg`)

| Call | Arguments | Result |
|:---|:---|:---|
| `browser.goto(url, ...)` | **compound**: fetch + parse + tree (one round trip) | `Snapshot` |
| `browser.request(url, opts, ...)` | lower level: one policy-aware request | `Response` |
| `browser.snapshot(page, ...)` | rebuild/serve the tree | `Snapshot` |
| `browser.query(page, selector, ...)` | minimal CSS subset (`#id`, `.class`, `tag`, `[attr=val]`, compounds) | `[Element]` |
| `browser.click(page, target, ...)` | ref or `{selector}` — follows links / triggers form actions | `ActResult` |
| `browser.clicks(page, targets, ...)` | up to 10 targets, sequential batch | `[ActResult]` |
| `browser.type(page, target, text, ...)` | bind a value to a form field | `ActResult` |
| `browser.fill(page, fields, submit?, ...)` | up to 20 `{ref \| selector, text}` + optional submit | `ActResult` |
| `browser.submit(page, form, ...)` | encode fields (urlencoded / multipart subset) and POST/GET the form action | `Snapshot` |
| `browser.key(page, key, ...)` | form-state keys: `"Enter"`, `"Escape"`, `"Tab"` | `ActResult` |
| `browser.back(page)` / `browser.forward(page)` / `browser.refresh(page)` | in-process history stack | `Snapshot` |
| `browser.wait(page, target, ...)` | `{ text }` \| `{ selector }` — deadline-bounded re-fetch; no poll loops in the language | `WaitResult` |
| `browser.extract(page, schema, ...)` | a Nudge **type** — compile-time-verified structured extraction | `[T]` |
| `browser.links(page, scope?, extension?, ...)` | | `[Link]` |
| `browser.close(page)` | flush the profile journal | `ActResult` |

JS-gated behavior (F6+): after the in-language interpreter lands,
`goto` additionally executes page scripts under policy (§5) and the
snapshot reflects the post-execution DOM. The surface does not change.

**Target addressing.** Actions take an element `ref: int` from the
latest `Snapshot`/`query` of that page, or a `{selector}` escape hatch.
No pixel targets until the renderer lands (F7); pixel targets then bind
to the latest screenshot frame only.

**Freshness (fail-closed).** Every observation binds a `state_id`
(url + tree hash). Actions implicitly carry the page's latest state_id;
a ref not in that snapshot, or a stale state_id, is refused
`stale_state` with `action_sent = false`.

**Options (every method).** `allow: [DomainPattern]` — exact host or
`*.suffix`; ABSENT is unscoped (W0008), `[]` allows nothing; enforced on
every redirect hop and on every subresource; violations raise
`BrowserDenied` and mark the page out-of-scope. `deadline: ms` — soft by
default (`deadline_missed`), `NUDGE_BROWSER_STRICT=1` raises.
`goto`/`request` also take `headers`, `profile` (persistent cookie jar +
header signature + device identity, Store-backed with a size quota),
`locale`, `viewport`/`device` (F7).

### Result types

```nudge
Page      = { page: string, url: string, title: string, depth: int }
Snapshot  = { page: string, url: string, title: string, state_id: string,
              elements: [Element], tree: string, count: int, drift: Drift }
Element   = { ref: int, role: string, name: string, value: string,
              tag: string, href: string, clickable: bool, editable: bool }
Response  = { status: int, headers: [(string, string)], body: string,
              url: string, latency_ms: int }
ActResult = { ok: bool, outcome: string, latency_ms: int, error: string,
              action_sent: bool, snapshot: Snapshot }
WaitResult= { ok: bool, timed_out: bool, latency_ms: int }
Link      = { href: string, text: string, download: bool }
Drift     = { changed: bool, added: int, removed: int, summary: string }
```

### A taste

```nudge
type Product = { name: string, price: float, url: Url }

fn cheapest(query: string) -> [Product] uses Browser, LLM {
    // ONE network round trip: fetch + parse + snapshot tree
    let snap = browser.goto("https://shop.example.com/search?q=" + query,
                            allow = ["*.example.com"], locale = "en-US",
                            deadline = 20000)
    let sane = decide { ok: "are results shown?" yes/no } on snap.tree
               with { model: "llm:claude" }
    let r = route {
        open: browser.click(snap.page, 3, allow = ["*.example.com"],
                            snapshot = true) when sane.ok.p > 0.5,
        scan: browser.clicks(snap.page, [1, 2], allow = ["*.example.com"],
                             snapshot = true) otherwise
    }
    let items = browser.extract(r.snapshot.page, [Product])
    sort(items, by = x.price)[0..3]
}
```

The same program, run with `NUDGE_REPLAY=<trace>`, replays for $0; run
with `NUDGE_REPLAY=<trace> NUDGE_BROWSER_DRIFT=1`, it becomes the
scheduled watcher of §1 — read-only, report-only.

## 5. Everything above the kernel is produced in `.ndg`

Built only from Nudge primitives (`str.*`, `math.*`, integer/float
arithmetic, lists/records, `store.*`, `io.*`, `time.*`, `env.*`):

| Module | Contents |
|:---|:---|
| `codec/json.ndg` | JSON tokenizer + recursive-descent parser + encoder; depth limit (`str.parse` is scalar-only) |
| `codec/b64.ndg` | base64 encode/decode (table + mod/div arithmetic) |
| `codec/hash.ndg` | deterministic content hash (djb2-family modular multiply over `int`; state_id needs determinism, not cryptography) |
| `net/url.ndg` | URL parse/normalize, relative resolution |
| `net/httpc.ndg` | HTTP/1.1 message building and response parsing (status line, headers, `Content-Length` / chunked decode) — framing lives in `.ndg`; F8 adds h2/ws framing on the stream channel |
| `web/html.ndg` | HTML tokenizer + tree builder (tags/attrs/text/comments, void elements, raw-text containers) |
| `web/dom.ndg` | tree navigation, minimal selector subset, ref enumeration |
| `web/cookie.ndg` | cookie jar: `Set-Cookie` parse, domain/path/expiry matching; persisted per `profile` in Store with a fail-closed size quota |
| `web/policy.ndg` | domain allowlist (exact / `*.suffix`), redirect engine (max-hop, checked every hop), scheme allowlist (`http`/`https`; `file:` for fixtures/tests), **subresource policy** (default: same-origin scripts only after F6; cross-origin requires explicit allowlist) |
| `web/form.ndg` | form state, submit encoding (urlencoded + multipart subset) |
| `web/history.ndg` | in-process back/forward stack |
| `web/snapshot.ndg` | tree render (token-efficient), drift summary |
| `web/journal.ndg` | NTF record emission, NUDGE_REPLAY reading (strict exhaustion, `request_hash` mismatch → `ReplayMismatch`), drift diff |
| `web/js/*` (F6) | **in-language JavaScript**: tokenizer, parser, tree-walking bytecode interpreter, ES subset (closures, objects/arrays, promises/microtask queue, common builtins), DOM bindings (`querySelector`, event listeners, `fetch`/XHR hooked through engine policy), **step budget + deadline** (a runaway script is killed like any hung effect), pure `.ndg` data — no native escape, sandboxed by construction |
| `web/layout.ndg` + `web/compositor.ndg` (F7) | box model + inline/block layout, rasterization **on Nudge's existing in-house font/UI stack** (font_engine, ui window system — already first-party), screenshot output into `Snapshot.screenshot` |

Reused existing machinery: `store.*` (profiles, quota), `io.*`
(journal), `time.*` (deadlines, pacing), `env.*` (kill switch:
`NUDGE_BROWSER_KILL=1`, checked before every kernel call), `par map`
(parallel independent pages; race losers halt at the effect boundary —
existing cooperative cancellation).

## 6. Architecture

```
agent.ndg                       ← the user's program
────────── engine (.ndg) ───────
  prelude: web/* + codec/*      ← parser, DOM, JS interpreter (F6),
                                  layout/compositor (F7), policy,
                                  cookies, forms, journal, drift
────────── kernel (native) ─────
  browser.fetch                 ← v1: raw HTTP(S) bytes (existing TLS)
  connect/read/write/close      ← F8: stream channel (h2/ws framing in .ndg)
```

The engine is not a separate process: **the browser is the program**,
compiled to native bytecode. `goto` is one kernel call plus `.ndg`
parsing; with F6 it is kernel call(s) + parse + script execution, all
in-process.

## 7. Locale policy

The default is **neutral**: `locale = "en-US"`, engine-default timezone,
default viewport/device profile. `tr` — or any host locale — is never
inherited or defaulted. Locale, timezone, geo, and device values arrive
only as explicit options; derived headers (`Accept-Language`) follow the
declared locale, not the host. Presets are opt-in data, never implicit.

## 8. Privacy and anti-detection: complete, real, ours

Nudge does not imitate Camoufox — it matches what makes it *complete*
(engine ownership) and beats it on what detectors actually see:

1. **A real browser, not a fetcher.** JS execution (F6), layout and
   screenshots (F7), real TLS with ALPN — modern flows (consent walls,
   SPA pagination, bot-gated storefronts) work because the engine is
   real, not because we fake headers around missing capabilities.
2. **Zero automation residue — by construction.** The engine never
   implements WebDriver, CDP, or remote-debugging protocols; there is no
   automation surface to leak, no `navigator.webdriver`, no headless
   launch markers, because we wrote none. The automation interface is
   the typed `.ndg` surface and nothing else.
3. **An unknown fingerprint — by construction.** Detector ecosystems
   match *known* clients: known TLS ClientHello shapes, known header
   orders, known HTTP/2 SETTINGS, known canvas/font/audio signals. An
   in-house engine has none of these signatures in any database; its
   ClientHello and header signature are ours, consistent per profile,
   and unique by construction. Because we own the TLS stack we can keep
   a profile byte-stable across runs — which is exactly what
   continuity-based detection checks for — and vary per profile when
   isolation is desired.
4. **Complete identity ownership.** A `profile` = cookie jar + header
   signature + locale/device identity, persisted in Store (quota-
   enforced), inspectable, replayable. Identity stability is a design
   goal, not a side effect.
5. **Minimal egress.** A commercial browser phones home in the
   background (safebrowsing, OCSP, updates, prefetch, telemetry). Ours
   performs exactly the fetches the program declares — every one traced.
   The privacy surface is the trace, not a black box.
6. **Humanized pacing.** Optional, seeded jitter between requests via
   `time.*` — policy-controlled, deterministic under replay, traced.
7. **Trace hygiene.** Redaction at the single choke point; cookies and
   `Authorization` never reach the JSONL; replay corpora are shareable
   without leaking secrets; profiles are workspace-confined.
8. **Policy is the legality surface.** Allowlists, scheme and
   subresource policy are fail-closed and enforced on every hop;
   Content-Security-Policy is respected for script scoping. Operation
   remains program policy, and policy is enforced by the language.

## 9. Feature compatibility (the whole language × browser)

| Capability | Browser integration |
|:---|:---|
| Typed effects | `uses Browser` composes with LLM/Decision/Tool/IO/Process/Store/Workspace; `for_all` purity rejects browser calls |
| `decide{}` / `llm"""` | trees are observation state; drift summaries feed re-decision |
| `par map` | independent pages in one program; per-page journal seq; race losers halt at the browser effect boundary |
| Budget & deadlines | per-call deadline; command-level fetch counters in the journal; hard budget contract (§10) |
| `nudgec test` | browser tests against `file://` fixtures at $0 through the real engine |
| Replay | free, signature-verified, strict exhaustion — first-class for browser traces, including post-JS DOM snapshots |
| Drift | read-only live re-observation diffed against the recording |
| NTF traces | additive kinds `browser.snapshot` / `browser.act`; trace-check/trace-diff; redaction choke point |
| Conformance corpus | browser flows included; compound budgets and fingerprint-consistency locked by tests |
| `store` / profiles | cookie jar + header signature + device identity, quota-enforced |
| Grants / kill switch | `allow` scoping, `NUDGE_BROWSER_KILL=1`, JS step budget |
| Recipes / learn | browser recipes shipped as `.ndg`; a learn lesson added |
| Standalone targets (Python/TS) | the `.ndg` prelude is shared by all targets; only the kernel is implemented per target runtime (full parity) |

## 10. Completion criteria (measurable)

- **C1 — Command round-trip counting.** `goto → click → fill → click`
  costs exactly **4 kernel commands** (subresource traffic excluded and
  separately journaled), enforced by a journal counter in conformance.
  One `browser.*` network function = one kernel round trip.
- **C2 — Compound is mandatory.** `goto` (fetch+parse+tree in 1),
  `fill` (≤20 fields + submit in 1), `clicks` (≤10 in 1),
  `snapshot = true` (act + observe in 1). A flow that could be compound
  and is not, is a bug.
- **C3 — Snapshot-first.** The tree is the read path; bodies never flow
  into context unchecked (size limits + tree render).
- **C4 — Wait = deadline-bounded re-fetch.** No poll loops in the
  language.
- **C5 — Zero launch.** In-process engine: no child process, no IPC,
  browser startup cost = 0.
- **C6 — Replay gate.** Any recorded session replays at **$0**: zero
  network bytes, zero live LLM/Decision calls, actions dry-run, strict
  exhaustion, `request_hash` verified — including F6 sessions (DOM
  snapshots from the trace, script output never re-executed). Enforced
  in CI.
- **C7 — Schedule gate.** A replay-clean flow runs as a timer in
  replay + drift mode: actions never fire, the report is `ok | error`,
  escalation only on drift. Demonstrated by `examples/browser-agent/`.
- **C8 — Latency baselines.** Warm `file://` goto p99 < 5 ms (pre-F6);
  every `Response`/`ActResult` carries `latency_ms`; the journal tracks
  regressions.
- **C9 — Fingerprint consistency.** Two runs with the same profile
  produce byte-identical request heads (header order + TLS parameters
  recorded in the journal); two profiles differ. Locked in conformance.
- **C10 — JS sandbox budget.** Every script execution is step-budgeted
  and deadline-bounded; an infinite loop is killed with a typed error,
  never a hung program. Fuzz corpus required before F6 exits.

## 11. Implementation phases

- **F1 — Kernel + effect.** `browser.fetch` in all three target runtimes
  (VM, Python, TS — one each, existing TLS stack, no new crate);
  `Browser` effect; W0008; prelude mapping; ntf-spec record kinds.
- **F2 — Codecs & net (`.ndg`).** `codec/json`, `codec/b64`,
  `codec/hash`, `net/url`, `net/httpc` — validated on `file://` and
  `http://`.
- **F3 — Engine core (`.ndg`).** `web/html`, `web/dom`, `web/snapshot`,
  `web/policy` — goto → snapshot → query → click chain against live
  sites.
- **F4 — Interaction.** `web/form`, `web/cookie`, `web/history`,
  `wait`, `extract`, `clicks`; compound budgets (C1–C4) locked.
- **F5 — Journal, replay, drift, scheduling.** NTF emission, replay,
  drift diff, the §1 loop; `examples/browser-agent/`; `file://`
  fixtures; CI with no network, $0, real engine.
- **F6 — In-language JavaScript.** `web/js/*`: interpreter, DOM
  bindings, policy-gated script execution, subresource policy, CSP
  respect, step budget (C10), fuzzing (repo has the precedent);
  snapshots reflect post-execution DOM.
- **F7 — Rendering.** `web/layout.ndg` + `web/compositor.ndg` on the
  in-house font/UI stack; screenshots into `Snapshot`; viewport/device
  emulation (per-profile device identity).
- **F8 — Transport growth.** Kernel stream channel
  (`connect/read/write/close`); HTTP/2 and WebSocket framing in `.ndg`;
  ALPN/profile-driven negotiation.
- **F9 — Polish.** README effect table 12 → 13, learn lesson, VS Code
  syntax, browser recipes, CHANGELOG.

## 12. Design-review log (defects found and resolved)

| # | Defect found in review | Resolution |
|:---|:---|:---|
| D1 | "No real browser" was presented as an anti-detection *advantage*; it is actually a completeness hole (JS-walled sites fail) | Real in-house engine promoted to the north star (§0.2); JS interpreter (F6) and renderer (F7) are core phases, not footnotes |
| D2 | Request/response kernel cannot carry HTTP/2 or WebSocket | Stream-channel growth path designed now, shipped F8 (§3) |
| D3 | "4 fetches" was unmeasurable once a real engine loads subresources | C1 redefined as **command-level** counting; subresources journaled separately; subresource policy added (§5) |
| D4 | Existing TLS stack has a non-browser fingerprint | Claim narrowed and owned: unknown-by-construction + per-profile byte-stability (C9, §8.3); in-house TLS control on the F8 path |
| D5 | Screenshot required a renderer that did not exist | F7 reuses the first-party font/UI stack — still zero dependencies (§5) |
| D6 | Profile data in Store could grow without bound | Fail-closed size quota per profile (§4, §5) |
| D7 | `par map` racing browser ops needed explicit cancellation semantics | Race losers halt at the effect boundary (existing cooperative cancellation), stated in §5/§9 |
| D8 | Page JS could read cross-origin content or run forever | Subresource policy default-deny cross-origin; CSP respected; step budget + deadline (C10) |
| D9 | Parser and interpreter are new attack surface | Fuzzing required before F6 exits (C10, repo precedent `fuzz.rs`) |

## 13. Non-goals (v1) and evolution path

No HTTP/2 or WebSocket before F8 (framing designed, shipped later); no
external browser or CDP/WebDriver driving — forbidden by §0, and
unneeded because the engine is ours; no provider/fake/bridge of any
kind; `tr` — or any host locale — is never a default locale; JS
execution begins at F6 (before that, scripts are inert text, and the
plan says so honestly rather than pretending); rendering begins at F7.
