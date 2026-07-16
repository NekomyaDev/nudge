"""nudge_runtime — the Nudge Python runtime (MVP, roadmap day 1–3).

Ships today: ``llm_call`` (fake provider + JSONL trace), ``render``, ``USD``,
``effectful``, and sequential ``par_map`` / ``par_all`` / ``par_race`` stubs.

Providers: ``NUDGE_PROVIDER=fake`` (the default) returns a deterministic
string and costs nothing — that is how CI runs with zero tokens. Real
provider clients land with the config layer after the MVP.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

__version__ = "0.1.0"


def USD(x) -> float:
    """Budget literal. Real budget enforcement lands on roadmap day 11–12."""
    return float(x)


def render(template: str, mapping: dict) -> str:
    """Fill ``{name}`` / ``{dotted.path}`` holes in a prompt template."""
    out = template
    for key, value in mapping.items():
        out = out.replace("{" + key + "}", str(value))
    return out


def effectful(effects):
    """Attach the declared effect set as metadata (verification is the
    compiler's job; this just keeps it visible at runtime)."""
    def deco(fn):
        fn.__nudge_effects__ = frozenset(effects)
        return fn
    return deco


def _trace_path() -> Path:
    return Path(os.environ.get("NUDGE_TRACE", "trace.jsonl"))


def _emit_trace(record: dict) -> None:
    path = _trace_path()
    seq = 1
    if path.exists():
        seq += sum(1 for _ in path.open("r", encoding="utf-8"))
    line = {"v": 1, "seq": seq, **record}
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(line, ensure_ascii=False) + "\n")


def llm_call(prompt, model=None, schema=None, retry=0, repair=False,
             budget=None, cache=None, tags=None):
    """One typed LLM call. MVP: fake provider only; budget/retry/repair are
    accepted and recorded but not yet enforced (days 4–6 / 11–12)."""
    provider = os.environ.get("NUDGE_PROVIDER", "fake")
    if provider != "fake":
        raise RuntimeError(
            "nudge_runtime MVP ships the fake provider only; set NUDGE_PROVIDER=fake"
        )
    output = f"[fake:{model or 'default'}] {prompt[:80]}"
    _emit_trace({
        "kind": "llm.call",
        "model": model or "default",
        "params": {"temperature": 0},
        "tokens": {"in": len(prompt.split()), "out": len(output.split())},
        "cost_usd": 0.0,
        "repair_rounds": 0,
        "provider": "fake",
    })
    return output


def par_map(coll, fn, concurrency=None):
    """Sequential stub. The real scheduler lands on roadmap day 11–12."""
    return [fn(x) for x in coll]


def par_all(items):
    return list(items)


def par_race(items):
    items = list(items)
    if not items:
        raise ValueError("par race needs at least one candidate")
    return items[0]
