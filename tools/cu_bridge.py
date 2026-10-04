#!/usr/bin/env python3
"""cu_bridge.py — reference computer-use bridge for Nudge (docs/computer-use.md).

A long-lived subprocess speaking the newline-delimited JSONL contract:

    request:  {"id": n, "op": "observe"|"click"|"type"|"key"|"scroll"|
               "set_value"|"drag", "app": "...", ...}
    response: {"id": n, "ok": true, "observation": {...}} |
              {"id": n, "ok": true, "result": {...}} |
              {"id": n, "ok": false, "error": "..."}

This reference implementation drives a **virtual desktop** (the same scene
model as the runtime's fake provider): `CU_BRIDGE_SCENARIO` names a JSON
file with `scenes` + `advance_on`; without it a fixed FakeApp scene is
served. Swap the bottom `_observe`/`_act` implementations for a real
backend (AT-SPI + xdotool on Linux, an OS automation API, or a remote
agent) — the wire contract stays identical.

Usage (from the Nudge side):
    NUDGE_COMPUTER_SERVERS='{"cu": {"command": "python3 tools/cu_bridge.py"}}' \
    NUDGE_COMPUTER_PROVIDER=cu nudgec run agent.ndg
"""
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


_SCENES, _ADVANCE_ON = _load_scenes()
_SCENE = 0
_STATE_IDS = 0


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
        # a 1-byte placeholder raster: real bridges attach a PNG data URL
        obs["screenshot"] = {"data_url": "data:image/png;base64,iVBORw0KGgo=",
                             "sha256": f"sha256:scene{_SCENE}"}
    return obs


def _act(op, params):
    global _SCENE
    if _ADVANCE_ON and ":" in _ADVANCE_ON:
        want_action, want_label = _ADVANCE_ON.split(":", 1)
        scene = _SCENES[_SCENE % len(_SCENES)]
        target = params.get("target") or {}
        hit = next((el for el in scene.get("elements", [])
                    if el.get("index") == target.get("index")), {})
        if op == want_action and hit.get("title") == want_label:
            _SCENE += 1
    return {"ok": True, "outcome": "ok", "latency_ms": 1, "error": ""}


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            rid = req.get("id")
            op = req.get("op")
            if op == "observe":
                out = {"id": rid, "ok": True,
                       "observation": _observe(req.get("app", ""),
                                               bool(req.get("include_screenshot")))}
            elif op in ("click", "type", "key", "scroll", "set_value", "drag"):
                out = {"id": rid, "ok": True, "result": _act(op, req)}
            else:
                out = {"id": rid, "ok": False, "error": f"unknown op '{op}'"}
        except Exception as e:  # never crash the pipe — report and continue
            out = {"id": req.get("id") if isinstance(req, dict) else None,
                   "ok": False, "error": f"{type(e).__name__}: {e}"}
        sys.stdout.write(json.dumps(out, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
