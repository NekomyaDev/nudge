# Nudge v1.5 — Computer Use: the machine as a typed effect

## 1. Thesis

Nudge already treats uncertainty as an effect: `llm"""` calls a model,
`decide{}` asks a decision engine, tools reach MCP servers. Computer use
completes the picture: `computer.observe(app)` and the action primitives
(`click`, `type`, `key`, `scroll`, `set_value`, `drag`) make the machine
itself a **typed, traced, replayable effect**.

The division of labor is deliberate:

- **The language owns control.** Every observation and every action flows
  through the runtime — effect inference (`uses Computer`), app/action
  scoping (`allow`), deadlines, a kill switch, and one NTF record per step.
  The compiler proves at check time that a function can move the mouse;
  nothing sneaks past a `pure` signature.
- **The model owns intelligence.** What an element tree *means*, whether a
  screen has drifted, and what to click next are perception problems. The
  program hands the observation (AX text tree, screenshot) to an LLM via
  `llm"""` or `decide{}` and acts on the typed answer. Nudge never
  "understands" a screen; it guarantees that whatever the model decided is
  executed, recorded, and re-executable under rules.

This mirrors how the zcode Computer Use capability works in practice
(accessibility-tree first, element-index addressing, observe → decide →
act → re-observe), and adopts its proven invariants: element indices over
pixel coordinates whenever possible, observations are read-only and safe
to repeat, actions are side effects that replay must never re-fire.

## 2. Language surface

`computer` is a contextual receiver: `computer.<method>(...)` is special
only as `computer . <known-method> (`; a bare `computer` stays an ordinary
identifier, and binding it is E0103. The methods (frozen v1):

```nudge
fn fix_note(app: string, target_index: int) -> bool uses Computer, Decision {
    // 1. observe — read-only, always safe to repeat
    let obs = computer.observe(app, allow = [app], screenshot = true, deadline = 30000)

    // 2. decide — the model reads the AX tree (text) and the screenshot
    let pick = decide {
        sane: "is this screen the expected one?" yes/no
    } on obs.tree
    with { model: "llm:claude" }

    // 3. act — the runtime executes what the policy chose
    let r = route {
        act:  computer.click(target_index, allow = [app]) when pick.sane.p > 0.5,
        wait: computer.key("Escape", allow = [app]) otherwise
    }
    r.ok
}
```

### Methods

| Call | Arguments | Result |
|:---|:---|:---|
| `computer.observe(app, ...)` | app name | `Observation` |
| `computer.click(target, ...)` | element index, or `{x: int, y: int}` | `ActionResult` |
| `computer.type(text, ...)` | text to type into the focused element | `ActionResult` |
| `computer.key(key, ...)` | key name, e.g. `"Return"`, `"Control_L+a"` | `ActionResult` |
| `computer.scroll(target, direction, pages?, ...)` | target, `"up"/"down"/...` | `ActionResult` |
| `computer.set_value(index, value, ...)` | element index + text | `ActionResult` |
| `computer.drag(from, to, ...)` | two targets | `ActionResult` |
| `computer.perform(index, action, ...)` | element index + one of the element's OWN advertised `.actions` | `ActionResult` |
| `computer.paste(text, format?, ...)` | text (`"text"/"md"/"html"`) — the bridge borrows the user's clipboard and restores it | `ActionResult` |

### Options (every method, frozen v1)

- `allow: [AppName, ...]` — the apps this call may touch (W0006 warns when
  a computer call carries no `allow`; the runtime refuses apps outside the
  list with `ComputerDenied`).
- `deadline: ms` — soft by default: an overrun annotates the record
  `deadline_missed`; `NUDGE_COMPUTER_STRICT=1` makes it raise
  `ComputerTimeout` (the decide-deadline semantics, unchanged).
- `screenshot: bool` (observe only) — include a screenshot; the result
  carries it as a data URL in `.screenshot` plus its sha256 in
  `.screenshot_hash`.

### Result types

```nudge
Observation  = { app: string, state_id: string, title: string,
                 elements: [Element], tree: string,
                 screenshot_hash: string, screenshot: string, drift: Drift }
Element      = { index: int, role: string, title: string, value: string,
                 pressable: bool, editable: bool, focused: bool,
                 enabled: bool, actions: [string] }
ActionResult = { ok: bool, outcome: string, latency_ms: int, error: string,
                 action_sent: bool }
Drift        = { changed: bool, screenshot_changed: bool, added, removed,
                 summary: string }
```

`Element.index` is the addressing currency: the model picks an index from
the tree, the program clicks that index. Targets may also be `{x, y}`
raster pixel coordinates of the **latest returned screenshot** — the
language never converts coordinate systems; the provider owns the frame
authority (zcode rule: element/window bounds are diagnostics, never
coordinates).

## 3. Providers

Provider resolution mirrors `NUDGE_DECISION_SERVERS`: a JSON registry,
selected by `NUDGE_COMPUTER_PROVIDER` (default `fake`).

```sh
# subprocess JSONL bridge (long-lived; one request per line on stdin)
NUDGE_COMPUTER_SERVERS='{"cu": {"command": "python3 tools/cu_bridge.py"}}' \
NUDGE_COMPUTER_PROVIDER=cu nudgec build agent.ndg && python3 out/agent.py

# HTTP transport
NUDGE_COMPUTER_SERVERS='{"remote": {"base_url": "http://localhost:9333"}}' \
NUDGE_COMPUTER_PROVIDER=remote nudgec build agent.ndg && python3 out/agent.py
```

The reference bridge has two backends behind the same contract:
`CU_BRIDGE_BACKEND=real` (X11 only) drives the **live desktop** — the
accessibility tree comes from AT-SPI (pyatspi), clicks/keys/scrolls/drag
run through `xdotool`, screenshots via ImageMagick `import`, the clipboard
via `xclip`. Everything else — the lease, `state_id` staleness fail-closed,
dispatch receipts, `abort` — is enforced identically on the virtual and
the real backend (verified live: a Nudge program observed a real GTK
dialog and its click closed it). On Wayland only the screenshot half may
work, depending on the compositor's X11 compatibility.

Wire contract (both transports, one round trip per call):

- request: `{"id": n, "op": "observe"|"click"|"type"|"key"|"scroll"|"set_value"|"drag"|"perform"|"paste"|"abort", "app": "...", ...params}`
  — observe takes `include_screenshot: bool`; acts take `target`
  (`{"index": i}` or `{"x": x, "y": y}`) plus their payload (`text`, `key`,
  `direction`, `pages`, `value`, `to`, `action`) and the `state_id` of the
  observation they were decided on. `abort` (no other fields) releases any
  held mouse button — the runtime sends it at process exit and when the
  kill switch fires, because an interrupted drag must never stay pressed.
- response: `{"id": n, "ok": true, "observation": {...}}` /
  `{"id": n, "ok": true, "result": {...}}`, or
  `{"id": n, "ok": false, "error": "..."}`. Action results carry a
  **dispatch receipt**: `dispatch: "sent"` (the bridge KNOWS the input was
  dispatched), `"not_sent"` (certain nothing was dispatched), or
  `"unknown"` — surfaced in the language as `result.action_sent`. A
  non-idempotent action may only be retried when `action_sent` is false;
  otherwise the safe move is re-observe and re-decide.

Hardening invariants every bridge MUST enforce (fail closed):

- **Controller lease** — one live controller per backend. The reference
  bridge holds a lockfile lease (`CU_BRIDGE_LOCK`); calls from a second
  controller get `controller_busy` (abort is always allowed).
- **Stale-element refusal** — an action whose `target.index` was not in the
  element table of the `state_id` it references is refused with
  `stale_state` (and `dispatch: "not_sent"`). Elements move; blind clicks
  are errors, not gambles.
- **perform_action parity** — `perform` is only legal for actions the
  element itself advertises in `.actions`; anything else is
  `not_actionable`.
- Element observations may carry `bounds: [x, y, w, h]` — diagnostic
  geometry only, never a click-coordinate source (the provider owns the
  frame authority).

Adapter validation is strict (the decision-adapter rules apply): unknown
operations, missing fields, non-finite numbers and unnormalized trees are
rejected — a provider never gets to silently "invent" an element index the
observation didn't show.

### Fake provider (default)

A deterministic in-memory desktop: a fixed element tree, deterministic
`latency_ms = 0..2`, sha256-stable fake screenshots. `nudgec test` and CI
run the whole observe → decide → act loop at $0 with no API keys.
`NUDGE_COMPUTER_SCENARIO=<file.json>` loads a scene list (each scene a
full `Observation`) advanced by a matching action (`{"advance_on":
"click:OK"}`) — this drives the drift and multi-step e2e tests.

## 4. Replay and drift (the "did anything change?" story)

Computer actions are real side effects: replaying a trace must **never**
re-fire a click. The three modes:

1. **Record (live).** Observations and actions execute and are traced.
2. **`NUDGE_REPLAY=all`.** Observations come from the trace (in `seq`
   order, strict exhaustion like LLM replay); actions are dry-run — the
   recorded `ActionResult` is returned, nothing executes, nothing costs.
3. **Drift check (`NUDGE_REPLAY=all` + `NUDGE_COMPUTER_DRIFT=1`).**
   Observations are re-taken **live** (read-only, safe) and compared
   against the recorded ones: element tree diff (added/removed rows) and
   screenshot hash. The live observation carries the `drift` record —
   `changed`, `screenshot_changed`, `added`, `removed`, and a one-line
   `summary`. Actions stay dry-run.

The drift record is *evidence, not judgment*: the language computes the
mechanical difference; what it means for the task is the model's call.
Feed it back:

```nudge
let obs = computer.observe(app, allow = [app])
let verdict = route {
    ask:  llm"""The screen changed during replay: {obs.drift.summary}
Recorded tree was: {obs.tree}
Should the recorded plan still run? Answer replan or abort."""
           with { model: "llm:claude", schema: string } when obs.drift.changed,
    skip: "no change" otherwise
}
```

## 5. NTF

Two additive record kinds (docs/ntf-spec.md; consumers MUST accept them):

- `computer.observe` — `app`, `state_id`, `element_count`, `outcome`,
  `latency_ms`; additive: `title`, `tree`, `screenshot_hash`, `drift`,
  `branch`, `deadline_ms`.
- `computer.act` — `action`, `app`, `target`, `outcome`, `latency_ms`;
  additive: `ok`, `error`, `value`, `dry_run`, `deadline_missed`,
  `deadline_ms`, `branch`.

`trace-check` validates required fields and types; the conformance corpus
covers both kinds. `trace-diff` counts them like other kinds.

## 6. Safety model

- **Compile time:** the `Computer` effect is visible in every signature —
  a `pure`/no-`uses` function is provably unable to touch the machine;
  `for_all` purity (E0804) rejects computer calls in properties; W0006
  nudges every call site toward an explicit `allow` scope.
- **Run time:** app allowlists (`ComputerDenied` on violation), the
  `NUDGE_COMPUTER_KILL=1` kill switch (checked before every call, refuses
  all further computer work for the process), soft deadlines, and a
  single-controller lease per process (nested bridges are refused, mirroring
  `CONTROLLER_BUSY`).
- **Replay time:** actions never re-execute; drift mode is read-only.

## 7. Multimodal LLM input

`llm"""` gains an `images` option (additive): a list of data URLs or base64
PNG strings appended to the message as image content blocks. The fake
provider ignores them; OpenAI-compatible and Anthropic transports render
them natively. Text-only providers receiving images raise a clear error
instead of silently dropping them. This is what lets a program send
`obs.screenshot` to the model alongside the AX tree.

## 8. Non-goals (v1)

No pixel-level vision in the language (the model does perception); no
remote/agent-swarm device orchestration; no recording of raw screenshot
bytes in traces (hash + data URL in the live result only); no OS-level
sandboxing of bridges (the bridge is a trusted local process, like an MCP
server); no high-level `computer{goal}` delegation loop — that layer can
ship later on top of these primitives.
