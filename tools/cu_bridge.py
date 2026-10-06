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

This reference implementation has two backends:

- **virtual** (default) — the same scene model as the runtime's fake
  provider: ``CU_BRIDGE_SCENARIO`` names a JSON file with ``scenes`` +
  ``advance_on``; without it a fixed FakeApp scene is served.
- **real** (``CU_BRIDGE_BACKEND=real``) — a live X11 desktop: the
  accessibility tree comes from AT-SPI (pyatspi), actions run through
  ``xdotool``, screenshots via ImageMagick ``import``, the clipboard via
  ``xclip``. The wire contract and every hardening invariant are identical
  on both backends.

Swap the bottom ``_observe``/``_act`` implementations for any other backend
(an OS automation API, or a remote agent) — the contract stays identical.

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
                except ProcessLookupError:
                    pass  # owner gone — stale lease, re-claim
                except PermissionError:
                    # EPERM means the owner EXISTS but we cannot signal it —
                    # that is NOT a stale lease; reclaiming would steal a
                    # live controller. Fail closed.
                    return "controller_busy (owner alive, unverifiable)"
                except ValueError:
                    pass  # unparseable pid — stale lease, re-claim
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
_LAST_OBS = {"state_id": None, "elements": [], "window": None}
# held mouse button parity (a real backend tracks the physical state)
_BUTTON_HELD = False

# ── real X11 backend (pyatspi + xdotool) ─────────────────────────────

_BACKEND = os.environ.get("CU_BRIDGE_BACKEND", "virtual").lower()
# live element table of the last real observation: index -> atspi object
_REAL_ELS = {}
_MAX_ELEMENTS = 200


def _sh(cmd, input_bytes=None):
    import subprocess
    r = subprocess.run(cmd, input=input_bytes, capture_output=True, timeout=10)
    if r.returncode != 0:
        raise RuntimeError(f"{cmd[0]} failed: {r.stderr.decode(errors='replace').strip()[:200]}")
    return r.stdout


def _real_app_names():
    import pyatspi
    names = []
    for app in pyatspi.Registry.getDesktop(0):
        try:
            names.append(app.name or "")
        except Exception:
            continue
    return names


def _real_resolve_app(app_filter):
    """Exactly ONE application must match: an exact (case-insensitive)
    name match wins, otherwise a UNIQUE substring match. Anything else is
    a fail-closed error — allow-scoping a substring over multiple apps is
    not a security boundary."""
    import pyatspi
    names = _real_app_names()
    if not app_filter:
        if len(names) == 1:
            return names[0]
        raise RuntimeError(
            f"fail-closed: no app named and {len(names)} applications are present — name the app")
    exact = [n for n in names if n and n.lower() == app_filter.lower()]
    if len(exact) == 1:
        return exact[0]
    if len(exact) > 1:
        raise RuntimeError(f"fail-closed: app '{app_filter}' is ambiguous: {exact}")
    subs = [n for n in names if n and app_filter.lower() in n.lower()]
    if len(subs) == 1:
        return subs[0]
    if not subs:
        raise RuntimeError(
            f"fail-closed: no application matches '{app_filter}' (seen: {names})")
    raise RuntimeError(f"fail-closed: app '{app_filter}' is ambiguous: {subs}")


def _real_window_id(app_name):
    """Resolve the app's visible X window id — the scope for screenshots,
    focus checks and coordinate targets."""
    for flag in ("--class", "--name"):
        try:
            out = _sh(["xdotool", "search", "--onlyvisible", flag, app_name])
            ids = [i for i in out.decode().split() if i.strip()]
            if ids:
                return ids[0]
        except Exception:
            continue
    return None


def _real_collect(app_name):
    """Walk the AT-SPI tree of the RESOLVED application only and flatten it
    into element rows. Windows/frames are always kept; leaf nodes are kept
    when they advertise actions, editable text, or a value."""
    import pyatspi

    def rows(node, depth):
        if node is None or depth > 12 or len(_REAL_ELS) >= _MAX_ELEMENTS:
            return
        try:
            role = node.getRoleName()
            name = node.name or ""
        except Exception:
            return
        actions = []
        try:
            act = node.queryAction()
            actions = [act.getName(i) for i in range(act.nActions)]
        except Exception:
            pass
        editable = False
        value = None
        try:
            text = node.queryText()
            editable = bool(node.getState().contains(pyatspi.STATE_EDITABLE))
            value = text.getText(0, -1)
        except Exception:
            try:
                value = str(node.queryValue().currentValue)
            except Exception:
                value = None
        focused = False
        try:
            focused = bool(node.getState().contains(pyatspi.STATE_FOCUSED))
        except Exception:
            pass
        interesting = (actions or editable or role in
                       ("window", "frame", "dialog", "push button",
                        "text", "entry", "check box", "radio button",
                        "page tab", "menu item", "combo box", "slider"))
        if interesting:
            bounds = []
            try:
                ext = node.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
                if ext.width and ext.height:
                    bounds = [float(ext.x), float(ext.y),
                              float(ext.width), float(ext.height)]
            except Exception:
                pass
            el = {"index": 0, "role": role, "title": name}
            if value is not None:
                el["value"] = value
            el["pressable"] = "press" in actions or "click" in actions
            if editable:
                el["editable"] = True
            if focused:
                el["focused"] = True
            if actions:
                el["actions"] = actions
            if bounds:
                el["bounds"] = bounds
            _REAL_ELS[len(_REAL_ELS)] = (node, el)
        for child in node:
            rows(child, depth + 1)

    # walk ONLY the resolved application's subtree — a substring filter
    # over the whole desktop could merge two apps into one authority
    for app in pyatspi.Registry.getDesktop(0):
        try:
            app_node_name = app.name or ""
        except Exception:
            continue
        if app_node_name == app_name:
            rows(app, 0)
            break


def _real_screenshot(window_id=None):
    import base64
    import hashlib
    # scoped: capture the observed window, never the root desktop — a root
    # grab under an allow scope would leak other apps' content
    target = str(window_id) if window_id else "root"
    png = _sh(["import", "-window", target, "png:-"])
    # canonical hash: SHA-256 of the raw PNG bytes
    digest = hashlib.sha256(png).hexdigest()
    return {"data_url": "data:image/png;base64,"
            + base64.b64encode(png).decode(),
            "sha256": f"sha256:{digest}"}


def _real_observe(app, include_screenshot):
    global _STATE_IDS
    # fail-closed: an unknown or ambiguous app is an error, never an
    # empty-but-successful observation
    resolved = _real_resolve_app(app)
    _STATE_IDS += 1
    _REAL_ELS.clear()
    _real_collect(resolved)
    window = _real_window_id(resolved)
    elements = []
    for _, el in _REAL_ELS.values():
        el = dict(el)
        el["index"] = len(elements)
        elements.append(el)
    obs = {"state_id": f"b-{_STATE_IDS}",
           "title": resolved, "elements": elements}
    if include_screenshot:
        if not window:
            raise RuntimeError(
                f"fail-closed: cannot scope a screenshot to '{resolved}' — no visible window found")
        obs["screenshot"] = _real_screenshot(window)
    _LAST_OBS["state_id"] = obs["state_id"]
    _LAST_OBS["elements"] = elements
    # the observed window is the action authority for focus/coordinate checks
    _LAST_OBS["window"] = window
    return obs


def _window_geometry(window_id):
    """(x, y, w, h) of an X window, or None."""
    import re
    try:
        out = _sh(["xdotool", "getwindowgeometry", str(window_id)]).decode()
    except Exception:
        return None
    pos = re.search(r"Position:\s*(-?\d+),(-?\d+)", out)
    geo = re.search(r"Geometry:\s*(\d+)x(\d+)", out)
    if not (pos and geo):
        return None
    return (int(pos.group(1)), int(pos.group(2)),
            int(geo.group(1)), int(geo.group(2)))


def _focus_moved_reason():
    """type/key/paste go to the FOCUSED window — if focus left the
    observed app since the observation, dispatching blind input would
    write into whatever app the user happens to be in."""
    window = _LAST_OBS.get("window")
    if not window:
        return "no observed window to verify focus against — re-observe"
    try:
        active = _sh(["xdotool", "getactivewindow"]).decode().strip()
    except Exception as e:
        return f"could not verify focus: {e}"
    if active != str(window):
        return (f"focus moved since the observation: active window is {active}, "
                f"observed window is {window} — re-observe before typing")
    return None


def _center(target):
    """Resolve a target to desktop pixels: an observed element's bounds
    center, or an explicit {x, y} raster point (validated against the
    observed window's geometry — coordinates may not reach other apps).
    FAIL CLOSED: if the window geometry cannot be proven, a coordinate
    target is refused — inability to prove containment is never
    permission."""
    index = target.get("index", -1)
    if index == -1 and isinstance(target.get("x"), (int, float)):
        x, y = float(target["x"]), float(target["y"])
        window = _LAST_OBS.get("window")
        geo = _window_geometry(window) if window else None
        if not geo:
            raise RuntimeError(
                "could not verify the observed window's geometry — refusing "
                f"the coordinate target ({x}, {y}) fail-closed (re-observe)")
        gx, gy, gw, gh = geo
        if not (gx <= x < gx + gw and gy <= y < gy + gh):
            raise RuntimeError(
                f"coordinate ({x}, {y}) is outside the observed window's "
                f"geometry {geo} — coordinate targets may not reach other apps")
        return x, y
    entry = _REAL_ELS.get(index)
    if entry is None:
        raise RuntimeError(f"element {index} is not in the last observation")
    bounds = entry[1].get("bounds")
    if not bounds or len(bounds) != 4:
        raise RuntimeError(f"element {index} has no usable bounds")
    x, y, w, h = bounds
    return x + w / 2.0, y + h / 2.0


def _real_act(op, req):
    import time
    started = time.monotonic()
    # dispatch tracking: "not_sent" is only honest when NOTHING reached
    # the desktop yet — a multi-stage op (scroll clicks, drag press/move,
    # paste clipboard write) that failed mid-way has already had a partial
    # external effect, and that must be reported as "unknown", never as
    # the retry-safe "not_sent"
    dispatched = [False]

    def result(outcome, dispatch, error=""):
        return {"ok": outcome == "ok", "outcome": outcome,
                "latency_ms": int((time.monotonic() - started) * 1000),
                "error": error, "dispatch": dispatch}

    try:
        target = req.get("target") or {}
        if op == "click":
            # prefer the element's OWN semantic action (AT-SPI doAction) —
            # it is bound to the element's authority, not to wherever the
            # global X pointer/focus happens to be; fall back to a physical
            # pointer click for elementless/coordinate targets
            entry = _REAL_ELS.get(target.get("index", -1))
            if entry is not None:
                actions = entry[1].get("actions") or []
                for name in ("click", "press", "AXPress"):
                    if name in actions:
                        try:
                            entry[0].queryAction().doAction(actions.index(name))
                            return result("ok", "sent")
                        except Exception:
                            break
            x, y = _center(target)
            _sh(["xdotool", "mousemove", str(int(x)), str(int(y)),
                 "click", "1"])
            return result("ok", "sent")
        if op == "type":
            moved = _focus_moved_reason()
            if moved:
                return result("stale_state", "not_sent", moved)
            _sh(["xdotool", "type", "--delay", "20", "--", str(req.get("text", ""))])
            return result("ok", "sent")
        if op == "key":
            moved = _focus_moved_reason()
            if moved:
                return result("stale_state", "not_sent", moved)
            _sh(["xdotool", "key", "--", str(req.get("key", ""))])
            return result("ok", "sent")
        if op == "scroll":
            direction = str(req.get("direction", ""))
            button = {"up": "4", "down": "5", "left": "6", "right": "7"}.get(direction)
            if not button:
                return result("not_actionable", "not_sent",
                              f"unknown scroll direction '{direction}' "
                              f"(up/down/left/right)")
            try:
                pages = int(req.get("pages", 1))
            except (TypeError, ValueError):
                return result("not_actionable", "not_sent",
                              f"pages must be an integer, got {req.get('pages')!r}")
            if pages <= 0 or pages > 20:
                return result("not_actionable", "not_sent",
                              f"pages must be 1..=20 (got {pages}) — cap bounds "
                              f"the dispatch volume")
            x, y = _center(target) if target.get("index", -1) != -1 else (0, 0)
            if x or y:
                _sh(["xdotool", "mousemove", str(int(x)), str(int(y))])
            clicks = pages * 3
            done = 0
            try:
                for _ in range(clicks):
                    _sh(["xdotool", "click", button])
                    done += 1
            except RuntimeError as e:
                if done > 0:
                    # some wheel clicks landed — a partial external effect
                    # already happened, "not_sent" would be a lie
                    return result("error", "unknown",
                                  f"{done}/{clicks} wheel clicks dispatched before "
                                  f"failure: {e}")
                raise
            return result("ok", "sent")
        if op == "set_value":
            entry = _REAL_ELS.get(target.get("index", -1))
            if entry is None:
                return result("stale_state", "not_sent",
                              f"element {target.get('index')} is not in the last observation")
            try:
                entry[0].queryEditableText().setTextContents(str(req.get("value", "")))
                return result("ok", "sent")
            except Exception:
                # never claim ok with error set — an element without an
                # editable interface is a FAILURE, not a successful no-op
                return result("not_actionable", "not_sent",
                              f"element {target.get('index')} has no editable text interface")
        if op == "perform":
            index = target.get("index", -1)
            entry = _REAL_ELS.get(index)
            if entry is None:
                return result("stale_state", "not_sent",
                              f"element {index} is not in the last observation")
            action = str(req.get("action", ""))
            actions = entry[1].get("actions") or []
            if action not in actions:
                return result("not_actionable", "not_sent",
                              f"element {index} does not advertise '{action}'")
            entry[0].queryAction().doAction(actions.index(action))
            return result("ok", "sent")
        if op == "paste":
            moved = _focus_moved_reason()
            if moved:
                return result("stale_state", "not_sent", moved)
            text = str(req.get("text", ""))
            # borrow/restore: save the user's clipboard, paste ours, then
            # put theirs back — best-effort on the restore, never skip it
            old_clip = b""
            try:
                old_clip = _sh(["xclip", "-selection", "clipboard", "-o"])
            except Exception:
                pass
            clipboard_written = [False]
            try:
                try:
                    _sh(["xclip", "-selection", "clipboard"], text.encode())
                    clipboard_written[0] = True
                    _sh(["xdotool", "key", "--", "ctrl+v"])
                    return result("ok", "sent")
                except RuntimeError as e:
                    if clipboard_written[0]:
                        # the clipboard was already overwritten — a partial
                        # external effect happened, never "not_sent"
                        return result("error", "unknown",
                                      f"clipboard written but the paste did not "
                                      f"complete: {e}")
                    raise
            finally:
                try:
                    _sh(["xclip", "-selection", "clipboard"], old_clip)
                except Exception:
                    pass
        if op == "drag":
            x1, y1 = _center(target)
            x2, y2 = _center(req.get("to") or {})
            pressed = [False]
            try:
                _sh(["xdotool", "mousemove", str(int(x1)), str(int(y1)),
                     "mousedown", "1"])
                pressed[0] = True
                _sh(["xdotool", "mousemove", str(int(x2)), str(int(y2))])
                return result("ok", "sent")
            except RuntimeError as e:
                if pressed[0]:
                    # mousedown landed — the button was pressed on the
                    # desktop, an effect already occurred
                    return result("error", "unknown",
                                  f"drag failed after mousedown: {e}")
                raise
            finally:
                # a failed drag must NEVER leave the button held
                _sh(["xdotool", "mouseup", "1"])
        return result("error", "not_sent",
                      f"op '{op}' is not supported by the real backend")
    except RuntimeError as e:
        # xdotool/import/xclip failures before any dispatch: "not_sent" is
        # honest; once any stage reached the desktop it is "unknown"
        return result("error", "unknown" if dispatched[0] else "not_sent", str(e))


def _real_abort():
    try:
        _sh(["xdotool", "mouseup", "1"])
    except Exception:
        pass  # best-effort: never crash the pipe on abort
    return {"ok": True, "outcome": "ok", "latency_ms": 0, "error": ""}



def _observe(app, include_screenshot):
    if _BACKEND == "real":
        return _real_observe(app, include_screenshot)
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
    """Fail-closed staleness check: every action MUST carry the state_id of
    the observation it was decided on — a missing or empty state_id is an
    invalid request, not a bypass (acting without a proven observation is
    exactly what the fail-closed contract exists to prevent)."""
    want = str(req.get("state_id") or "")
    current = _LAST_OBS["state_id"]
    if not current or not want:
        return ("invalid_request: action missing state_id — observe the app, "
                "act on the state_id that observation returned")
    if want != current:
        return f"stale_state: observed {want}, current is {current}"
    target = req.get("target") or {}
    index = target.get("index", -1)
    if index == -1:
        return None  # app-scoped (type/key/paste) — no element targeted
    if not any(el.get("index") == index for el in _LAST_OBS["elements"]):
        return f"stale_state: element {index} is not in the last observation"
    return None


def _act(op, req):
    if _BACKEND == "real":
        reason = _stale_reason(req)
        if reason:
            return {"ok": False, "outcome": "stale_state", "latency_ms": 0,
                    "error": reason, "dispatch": "not_sent"}
        return _real_act(op, req)
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
    if _BACKEND == "real":
        return _real_abort()
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
