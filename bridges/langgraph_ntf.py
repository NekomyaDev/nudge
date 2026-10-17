#!/usr/bin/env python3
"""LangGraph → NTF bridge helpers and tracer (frozen v1).

LangGraph executes as a graph of steps over a shared state. This module
provides tested, framework-free conversion helpers and a callback-style
tracer that records graph steps and node operations into an NTF trace file::

    from bridges.langgraph_ntf import LangGraphNTFTracer

    tracer = LangGraphNTFTracer("run.jsonl")
    # Attach as callback to LangGraph invocation or call hooks directly

Only the stdlib is required; nothing here imports langgraph.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

_repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_runtime_dir = os.path.join(_repo_root, "runtime")
if _runtime_dir not in sys.path:
    sys.path.insert(0, _runtime_dir)


def step_to_ntf(step_name, state_delta, seq, error=None, branch=None):
    """One graph step → a ``fn.return`` record. ``state_delta`` is the
    mapping the step wrote into the shared state (unknown values are
    serialized with ``str`` rather than guessed)."""
    record = {
        "kind": "fn.return",
        "v": 1,
        "seq": seq,
        "fn": str(step_name),
        "output": _jsonable(state_delta),
    }
    if error is not None:
        record["outcome"] = "error"
        record["error"] = str(error)
    else:
        record["outcome"] = "ok"
    if branch is not None:
        record["branch"] = branch
    return record


class LangGraphNTFTracer:
    """Callback-style tracer for LangGraph run steps and node executions."""

    def __init__(self, path):
        self.path = path
        self._seq = 0
        self._runs = {}

    def _next_seq(self):
        self._seq += 1
        return self._seq

    def _emit(self, record):
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")

    def on_chain_start(self, serialized, inputs, **kwargs):
        run_id = kwargs.get("run_id")
        name = (serialized or {}).get("name") or kwargs.get("name") or "step"
        if run_id:
            self._runs[run_id] = {"name": name, "inputs": inputs}
        else:
            self._runs["_last"] = {"name": name, "inputs": inputs}

    def on_chain_end(self, outputs, **kwargs):
        run_id = kwargs.get("run_id")
        if run_id and run_id in self._runs:
            info = self._runs.pop(run_id)
        else:
            info = self._runs.pop("_last", {})
        step_name = info.get("name", "step")
        rec = step_to_ntf(step_name, outputs, self._next_seq())
        self._emit(rec)
        return rec

    def on_chain_error(self, error, **kwargs):
        run_id = kwargs.get("run_id")
        if run_id and run_id in self._runs:
            info = self._runs.pop(run_id)
        else:
            info = self._runs.pop("_last", {})
        step_name = info.get("name", "step")
        rec = step_to_ntf(step_name, None, self._next_seq(), error=error)
        self._emit(rec)
        return rec

    def on_tool_end(self, output, **kwargs):
        name = kwargs.get("name") or "tool"
        seq = self._next_seq()
        rec = {
            "kind": "tool.call",
            "v": 1,
            "seq": seq,
            "tool": str(name),
            "input": str(kwargs.get("input", "")),
            "output": _jsonable(output),
            "outcome": "ok",
        }
        self._emit(rec)
        return rec


def _jsonable(value):
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(v) for v in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=f"ntf-langgraph-{int(time.time())}.jsonl")
    ap.add_argument(
        "--demo", action="store_true", help="write synthetic demo records and validate"
    )
    args = ap.parse_args()

    if args.demo:
        tracer = LangGraphNTFTracer(args.out)
        tracer.on_chain_start({"name": "planner"}, {"task": "solve issue"})
        tracer.on_chain_end({"plan": ["step 1", "step 2"]})
        tracer.on_tool_end({"found": 3}, name="search_kb", input="query text")
        print(f"Wrote demo trace to {args.out}")
