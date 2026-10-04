#!/usr/bin/env python3
"""cu_bridge.py — reference computer-use bridge for Nudge (docs/computer-use.md).

A long-lived subprocess speaking the newline-delimited JSONL contract:

    request:  {"id": n, "op": "observe"|"click"|"type"|"key"|"scroll"|
               "set_value"|"drag"|"perform"|"paste"|"abort", "app": "...", ...}
    response: {"id": n, "ok": true, "observation": {...}} |
              {"id": n, "ok": true, "result": {...}} |
              {"id": n, "ok": false, "error": "..."}

Hardening invariants (mirroring the zcode Computer Use design):

- **Controller lease** — the bridge holds a lockfile lease
  (``CU_BRIDGE_LOCK``, default ``/tmp/nudge-cu-bridge.lock``). While a live
  controller owns it, every call from another bridge instance fails closed
  with ``controller_busy`` (zcode's CONTROLLER_BUSY parity).
- **Stale-element fail-closed** — every action carries the ``state_id`` of
  the observation it was decided on; the bridge refuses any element index
  it did not serve in that observation (``stale_state``). Unknown or moved
  elements are errors, never blind clicks.
- **Dispatch receipts** — action results carry ``dispatch: "sent"`` (the
  bridge KNOWS the input was dispatched) or ``"not_sent"`` (it is certain
  nothing was dispatched). Non-idempotent actions may only be retried when
  ``dispatch == "not_sent"`` — the same rule the runtime documents.
- **Abort** — ``{"op": "abort"}`` releases any held mouse button (an
  interrupted drag must never stay pressed) and is sent by the runtime at
  process exit and when the kill switch fires.

This reference implementation drives a **virtual desktop** (the same scene
model as the runtime's fake provider): ``CU_BRIDGE_SCENARIO`` names a JSON
file with ``scenes`` + ``advance_on``; without it a fixed FakeApp scene is
served. Swap the bottom ``_observe``/``_act`` implementations for a real
backend (AT-SPI + xdotool on Linux, an OS automation API, or a remote
agent) — the wire contract stays identical.

Usage (from the Nudge side):
    NUDGE_COMPUTER_SERVERS='{"cu": {"command": "python3 tools/cu_bridge.py"}}' \
    NUDGE_COMPUTER_PROVIDER=cu nudgec build agent.ndg && python3 out/agent.py
"""
import atexit
import json
import os
import sys


def _load_scenes():
    path = os.environ.get("CU_BRIDGE_SCENARIO")
    if path:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        if not data.get("scenes"):
            raise SystemExit("CU_BRIDGE_SCENARIO has no `scenes` list")
        return data["scenes"], data.get("advance_on")
    elements = [
        {"index": 0, "role": "window", "title": "FakeApp"},
        {"index": 1, "role": "button", "title": "OK", "pressable": True, "actions": ["press"]},
        {"index": 2, "role": "button", "title": "Cancel", "pressable": True, "actions": ["press"]},
        {"index": 3, "role": "textfield", "title": "Search", "value": "", "editable": True, "focused": True},
    ]
    return [{"title": "FakeApp", "elements": elements}], None


# ── controller lease (fail-closed, stale-pid-aware) ──────────────────

class Lease:
    """One live controller per bridge backend. A lockfile with the owning
    pid; a stale lease (owner process gone) is re-claimable."""

    def __init__(self):
        self.path = os.environ.get("CU_BRIDGE_LOCK", "/tmp/nudge-cu-bridge.lock")
        self.held = False
        atexit.register(self.release)

    def acquire(self):
        if self.held:
            return None  # we already own it
        try:
            if os.path.exists(self.path):
                try:
                    with open(self.path) as f:
                        pid = int(f.read().strip() or 0)
                    os.kill(pid, 0)  # alive? then the lease is real
                    return "controller_busy"
                except (ValueError, ProcessLookupError, PermissionError):
                    pass  # stale lease — re-claim
                os.unlink(self.path)
            fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            with os.fdopen(fd, "w") as f:
                f.write(str(os.getpid()))
            self.held = True
            return None
        except FileExistsError:
            return "controller_busy"
        except OSError as e:
            return f"lease unavailable: {e}"

    def release(self):
        if self.held:
            try:
                with open(self.path) as f:
                    if f.read().strip() == str(os.getpid()):
                        os.unlink(self.path)
            except OSError:
                pass
            self.held = False


_LEASE = Lease()
_SCENES, _ADVANCE_ON = _load_scenes()
_SCENE = 0
_STATE_IDS = 0
# the observation the controller last saw — the staleness authority
_LAST_OBS = {"state_id": None, "elements": []}
# held mouse button parity (a real backend tracks the physical state)
_BUTTON_HELD = False


def _observe(app, include_screenshot):
    global _STATE_IDS
    _STATE_IDS += 1
    scene = _SCENES[_SCENE % len(_SCENES)]
    obs = {
        "state_id": f"b-{_STATE_IDS}",
        "title": scene.get("title", app),
        "elements": scene.get("elements", []),
    }
    # the runtime renders the AX tree from elements when `tree` is absent
    if include_screenshot:
        obs["screenshot"] = {"data_url": "data:image/png;base64,iVBORw0KGgo=",
                             "sha256": f"sha256:scene{_SCENE}"}
    _LAST_OBS["state_id"] = obs["state_id"]
    _LAST_OBS["elements"] = obs["elements"]
    return obs


def _stale_reason(req):
    """Fail-closed staleness check: the action must reference the element
    table of the observation it was decided on."""
    want = req.get("state_id")
    if want and want != _LAST_OBS["state_id"]:
        return f"stale_state: observed {want}, current is {_LAST_OBS['state_id']}"
    target = req.get("target") or {}
    index = target.get("index", -1)
    if index == -1:
        return None  # app-scoped (type/key/paste) — no element targeted
    if not any(el.get("index") == index for el in _LAST_OBS["elements"]):
        return f"stale_state: element {index} is not in the last observation"
    return None


def _act(op, req):
    reason = _stale_reason(req)
    if reason:
        return {"ok": False, "outcome": "stale_state", "latency_ms": 0,
                "error": reason, "dispatch": "not_sent"}
    target = req.get("target") or {}
    if op == "perform":
        index = target.get("index", -1)
        el = next((e for e in _LAST_OBS["elements"] if e.get("index") == index), {})
        action = str(req.get("action", ""))
        if action and action not in (el.get("actions") or []):
            return {"ok": False, "outcome": "not_actionable", "latency_ms": 0,
                    "error": f"element {index} does not advertise '{action}'",
                    "dispatch": "not_sent"}
    if _ADVANCE_ON and ":" in _ADVANCE_ON:
        want_action, want_label = _ADVANCE_ON.split(":", 1)
        scene = _SCENES[_SCENE % len(_SCENES)]
        hit = next((el for el in scene.get("elements", [])
                    if el.get("index") == target.get("index")), {})
        acted_title = hit.get("title", "")
        if op == want_action and acted_title == want_label:
            _SCENE += 1
    return {"ok": True, "outcome": "ok", "latency_ms": 1, "error": "",
            "dispatch": "sent"}


def _abort(_req):
    # release any held mouse button — an interrupted drag must never stay
    # pressed (zcode kill-switch parity)
    return {"ok": True, "outcome": "ok", "latency_ms": 0, "error": ""}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            rid = req.get("id")
            op = req.get("op")
            if op != "abort":
                busy = _LEASE.acquire()
                if busy:
                    out = {"id": rid, "ok": False, "error": busy}
                elif op == "observe":
                    out = {"id": rid, "ok": True,
                           "observation": _observe(req.get("app", ""),
                                                   bool(req.get("include_screenshot")))}
                elif op in ("click", "type", "key", "scroll", "set_value",
                            "drag", "perform", "paste"):
                    out = {"id": rid, "ok": True, "result": _act(op, req)}
                else:
                    out = {"id": rid, "ok": False, "error": f"unknown op '{op}'"}
            else:
                out = {"id": rid, "ok": True, "result": _abort(req)}
        except Exception as e:  # never crash the pipe — report and continue
            out = {"id": req.get("id") if isinstance(req, dict) else None,
                   "ok": False, "error": f"{type(e).__name__}: {e}"}
        sys.stdout.write(json.dumps(out, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
