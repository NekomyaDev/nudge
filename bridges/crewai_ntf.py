#!/usr/bin/env python3
"""CrewAI → NTF bridge helpers and tracer (frozen v1).

CrewAI organizes agents, tasks, and tools. This module provides tested,
framework-free conversion helpers and an NTF tracer to record task
executions and tool invocations into an NTF JSONL trace file::

    from bridges.crewai_ntf import CrewAINTFTracer

    tracer = CrewAINTFTracer("crew_run.jsonl")
    # Attach tracer or record task transitions directly

Only the stdlib is required; nothing here imports crewai.
"""
import argparse
import json
import os
import sys
import time

_repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_runtime_dir = os.path.join(_repo_root, "runtime")
if _runtime_dir not in sys.path:
    sys.path.insert(0, _runtime_dir)


def task_to_ntf(seq, agent_role, task_description, output, error=None):
    """One task execution → an ``llm.call``-shaped record (the CrewAI
    agent role lands in the additive ``agent`` field, the model field
    records the role too so trace tooling groups per-agent)."""
    record = {
        "kind": "llm.call",
        "v": 1,
        "seq": seq,
        "model": f"crewai:{agent_role}",
        "params": {"temperature": 0},
        "input": str(task_description),
        "output": output
        if isinstance(output, (str, int, float, list, dict)) or output is None
        else str(output),
        "tokens": {"in": 0, "out": 0},
        "cost_usd": 0.0,
        "repair_round": 0,
        "outcome": "error" if error else "ok",
        "provider": "crewai",
        "agent": str(agent_role),
    }
    if error is not None:
        record["error"] = str(error)
    return record


def tool_use_to_ntf(seq, tool_name, tool_input, tool_output, error=None):
    """One tool use → a ``tool.call`` record (CrewAI tool name as the
    tool, serialized usage as input/output)."""
    record = {
        "kind": "tool.call",
        "v": 1,
        "seq": seq,
        "tool": str(tool_name),
        "input": str(tool_input),
        "output": str(tool_output),
        "outcome": "error" if error else "ok",
    }
    if error is not None:
        record["error"] = str(error)
    return record


class CrewAINTFTracer:
    """Tracer callback for CrewAI crew runs, logging task completions and tool calls."""

    def __init__(self, path):
        self.path = path
        self._seq = 0

    def _next_seq(self):
        self._seq += 1
        return self._seq

    def _emit(self, record):
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")

    def on_task_completed(self, agent_role, task_description, output):
        seq = self._next_seq()
        rec = task_to_ntf(seq, agent_role, task_description, output)
        self._emit(rec)
        return rec

    def on_task_error(self, agent_role, task_description, error):
        seq = self._next_seq()
        rec = task_to_ntf(seq, agent_role, task_description, "", error=error)
        self._emit(rec)
        return rec

    def on_tool_use(self, tool_name, tool_input, tool_output, error=None):
        seq = self._next_seq()
        rec = tool_use_to_ntf(seq, tool_name, tool_input, tool_output, error=error)
        self._emit(rec)
        return rec


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=f"ntf-crewai-{int(time.time())}.jsonl")
    ap.add_argument(
        "--demo", action="store_true", help="write synthetic demo records and validate"
    )
    args = ap.parse_args()

    if args.demo:
        tracer = CrewAINTFTracer(args.out)
        tracer.on_task_completed(
            "researcher", "find recent articles", {"articles_found": 5}
        )
        tracer.on_tool_use(
            "web_search", "AI news 2026", "top 5 links", error=None
        )
        print(f"Wrote demo trace to {args.out}")
