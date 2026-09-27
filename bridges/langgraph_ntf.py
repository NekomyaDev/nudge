#!/usr/bin/env python3
"""LangGraph → NTF bridge helpers (frozen v1) — community entry point.

LangGraph executes as a graph of steps over a shared state. This module
provides tested, framework-free conversion helpers; the full callback
tracer that hooks a live graph run is the open first contribution
(NekomyaDev/nudge#69) — use ``langchain_ntf.py`` as the reference.

A graph step maps naturally onto two NTF records::

    fn.return  (fn=step name, output=the state delta)
    tool.call  (when the step delegated to a tool)

Only the stdlib is required; nothing here imports langgraph.
"""
import json
import time


def step_to_ntf(step_name, state_delta, seq, error=None, branch=None):
    """One graph step → a ``fn.return`` record. ``state_delta`` is the
    mapping the step wrote into the shared state (unknown values are
    serialized with ``str`` rather than guessed)."""
    record = {
        "kind": "fn.return",
        "v": 1,
        "seq": seq,
        "fn": step_name,
        "output": _jsonable(state_delta),
    }
    if error is not None:
        record["outcome"] = "error"
        record["error"] = str(error)
    if branch is not None:
        record["branch"] = branch
    return record


def _jsonable(value):
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(v) for v in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)
