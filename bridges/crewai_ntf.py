#!/usr/bin/env python3
"""CrewAI → NTF bridge helpers (frozen v1) — community entry point.

CrewAI organizes agents, tasks and tools. The natural mapping: each task
execution becomes an ``llm.call``-shaped record (agent as model) and each
tool use a ``tool.call`` record. The tested, framework-free helpers below
do the field mapping; the hook integration that observes a live crew run
is the open first contribution (NekomyaDev/nudge#69).

Only the stdlib is required; nothing here imports crewai.
"""


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
        "output": output if isinstance(output, (str, int, float, list, dict)) or output is None else str(output),
        "tokens": {"in": 0, "out": 0},
        "cost_usd": 0.0,
        "repair_round": 0,
        "outcome": "error" if error else "ok",
        "provider": "crewai",
        "agent": agent_role,
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
        "tool": tool_name,
        "input": str(tool_input),
        "output": str(tool_output),
    }
    if error is not None:
        record["outcome"] = "error"
        record["error"] = str(error)
    return record
