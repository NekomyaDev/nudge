"""nudge_runtime — the Nudge Python runtime (roadmap day 4–6).

Ships today:
- ``Schema`` / ``schema`` / ``extend`` — JSON-Schema-ish dicts; record values
  are plain dicts validated at runtime (dataclasses land post-MVP)
- ``validate`` — dependency-free schema validator (objects, arrays, scalars,
  ``minimum``/``maximum``, ``format: uri``)
- ``llm_call`` — typed LLM call with the design §4.2 repair loop:
  schema violation → validation errors are fed back, up to ``retry`` rounds,
  then ``SchemaFailure``. Every attempt is its own trace record.
- fake provider — deterministic, schema-driven (synthesizes conforming
  values), zero tokens. ``NUDGE_FAKE_FAIL_FIRST=k`` forces k initial schema
  violations so repair paths are testable in CI.
- ``render``, ``USD``, ``effectful``, ``tool_stub``, sequential
  ``par_map`` / ``par_all`` / ``par_race`` stubs.

Env: ``NUDGE_PROVIDER=fake`` (default; real providers land post-MVP),
``NUDGE_TRACE`` (trace path, default ``trace.jsonl``).
"""

from __future__ import annotations

import json
import os
from pathlib import Path

__version__ = "0.1.0"


# ── schemas ──────────────────────────────────────────────────────────

class Schema(dict):
    """A JSON-Schema-ish dict. Being a dict lets aliases nest freely."""

    def __init__(self, d=(), name=None):
        super().__init__(d)
        self.name = name


def schema(d, name=None):
    return d if isinstance(d, Schema) else Schema(d, name)


def extend(base, extra):
    """Merge refinement keys onto an existing (alias) schema."""
    merged = Schema(dict(base))
    merged.update(extra)
    return merged


def validate(sch, value, path="$"):
    """Return a list of validation errors ([] means the value conforms)."""
    errs = []
    if not isinstance(sch, dict) or not sch:
        return errs
    t = sch.get("type")
    if t == "object":
        if not isinstance(value, dict):
            return [f"{path}: expected object, got {_kind(value)}"]
        for req in sch.get("required", []):
            if req not in value:
                errs.append(f"{path}.{req}: missing required field")
        for key, sub in sch.get("properties", {}).items():
            if key in value:
                errs += validate(sub, value[key], f"{path}.{key}")
    elif t == "array":
        if not isinstance(value, list):
            return [f"{path}: expected array, got {_kind(value)}"]
        for i, item in enumerate(value):
            errs += validate(sch.get("items", {}), item, f"{path}[{i}]")
    elif t == "string":
        if not isinstance(value, str):
            return [f"{path}: expected string, got {_kind(value)}"]
        if sch.get("format") == "uri":
            from urllib.parse import urlparse
            parsed = urlparse(value)
            if not (parsed.scheme and parsed.netloc):
                errs.append(f"{path}: not a valid uri: {value!r}")
    elif t == "number":
        if not isinstance(value, (int, float)) or isinstance(value, bool):
            return [f"{path}: expected number, got {_kind(value)}"]
        if "minimum" in sch and value < sch["minimum"]:
            errs.append(f"{path}: {value} < minimum {sch['minimum']}")
        if "maximum" in sch and value > sch["maximum"]:
            errs.append(f"{path}: {value} > maximum {sch['maximum']}")
    elif t == "integer":
        if not isinstance(value, int) or isinstance(value, bool):
            return [f"{path}: expected integer, got {_kind(value)}"]
    elif t == "boolean":
        if not isinstance(value, bool):
            return [f"{path}: expected boolean, got {_kind(value)}"]
    elif t == "null":
        if value is not None:
            errs.append(f"{path}: expected null, got {_kind(value)}")
    return errs


def _kind(value):
    return type(value).__name__


def _synth(sch):
    """Synthesize a schema-conforming value (the fake provider's answer)."""
    if not isinstance(sch, dict):
        return None
    t = sch.get("type")
    if t == "object":
        return {k: _synth(s) for k, s in sch.get("properties", {}).items()}
    if t == "array":
        return [_synth(sch.get("items", {}))]
    if t == "string":
        if sch.get("format") == "uri":
            return "https://example.com/fake"
        return "fake"
    if t == "number":
        lo, hi = sch.get("minimum"), sch.get("maximum")
        if lo is not None and hi is not None:
            return (lo + hi) / 2
        if lo is not None:
            return float(lo)
        if hi is not None:
            return float(hi)
        return 0.5
    if t == "integer":
        lo = sch.get("minimum")
        return int(lo) if lo is not None else 1
    if t == "boolean":
        return True
    if t == "null":
        return None
    return {}


# ── errors ───────────────────────────────────────────────────────────

class SchemaFailure(Exception):
    """Retries exhausted (design §4.2). Carries all validation errors and
    the last raw output; the trace keeps every attempt."""

    def __init__(self, errors, raw):
        self.errors = errors
        self.raw = raw
        first = errors[0] if errors else "validation failed"
        more = f" (+{len(errors) - 1} more)" if len(errors) > 1 else ""
        super().__init__(f"SchemaFailure: {first}{more}")


class BudgetExceeded(Exception):
    """Reserved for day 11–12 budget enforcement."""


# ── small helpers ────────────────────────────────────────────────────

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


def tool_stub(name):
    """Fake tool result while the MCP client is unbuilt (day 8–10)."""
    return []


# ── trace ────────────────────────────────────────────────────────────

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


def _trace_call(model, prompt, out, repair_round, outcome):
    _emit_trace({
        "kind": "llm.call",
        "model": model or "default",
        "params": {"temperature": 0},
        "tokens": {"in": len(str(prompt).split()), "out": len(str(out).split())},
        "cost_usd": 0.0,
        "repair_round": repair_round,
        "outcome": outcome,
        "provider": "fake",
    })


# ── the LLM call ─────────────────────────────────────────────────────

_FAKE_STATE = {"fail_left": int(os.environ.get("NUDGE_FAKE_FAIL_FIRST", "0"))}

_REPAIR_HINT = (
    "Your previous output failed validation. Errors: {errors}. "
    "Emit corrected output only."
)


def _fake_answer(prompt, model, sch):
    if _FAKE_STATE["fail_left"] > 0:
        _FAKE_STATE["fail_left"] -= 1
        return {"__invalid__": True} if sch is not None else "fake failure"
    if sch is not None:
        return _synth(sch)
    return f"[fake:{model or 'default'}] {str(prompt)[:80]}"


def llm_call(prompt, model=None, schema=None, retry=0, repair=False,
             budget=None, cache=None, tags=None):
    """One typed LLM call (design §4).

    MVP: fake provider only. With ``schema`` set, output is validated; a
    violation triggers the §4.2 repair loop for up to ``retry`` rounds when
    ``repair`` is set, then raises :class:`SchemaFailure`.
    """
    provider = os.environ.get("NUDGE_PROVIDER", "fake")
    if provider != "fake":
        raise RuntimeError(
            "nudge_runtime MVP ships the fake provider only; set NUDGE_PROVIDER=fake"
        )

    attempts = 1 + (retry if repair and schema is not None else 0)
    last_errors, last_raw = [], None
    for round_no in range(attempts):
        out = _fake_answer(prompt, model, schema)
        if schema is None:
            _trace_call(model, prompt, out, 0, "ok")
            return out
        errors = validate(schema, out)
        if not errors:
            _trace_call(model, prompt, out, round_no, "ok")
            return out
        last_errors, last_raw = errors, out
        _trace_call(model, prompt, out, round_no, "schema_violation")
        # design §4.2 step 1: feed raw output errors back to the model
        prompt = _REPAIR_HINT.format(errors="; ".join(errors)) + "\n" + str(prompt)
    raise SchemaFailure(last_errors, last_raw)


# ── parallelism (sequential stubs; real scheduler lands day 11–12) ───

def par_map(coll, fn, concurrency=None):
    """Sequential stub (real scheduler lands day 11–12).

    Honors Nudge's pair-unpacking: when the lambda takes more than one
    parameter and the element is a tuple of that arity (e.g. produced by
    ``zip``), it is spread across the parameters — ``|(a, h)| -> f(a, h)``.
    """
    def call(x):
        try:
            argc = fn.__code__.co_argcount
        except AttributeError:
            argc = 1
        if argc > 1 and isinstance(x, tuple) and len(x) == argc:
            return fn(*x)
        return fn(x)
    return [call(x) for x in coll]


def par_all(items):
    return list(items)


def par_race(items):
    items = list(items)
    if not items:
        raise ValueError("par race needs at least one candidate")
    return items[0]
