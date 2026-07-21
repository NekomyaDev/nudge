"""nudge_runtime — the Nudge Python runtime (roadmap day 4–10).

Ships today:
- ``Schema`` / ``schema`` / ``extend`` — JSON-Schema-ish dicts; record values
  are plain dicts validated at runtime (dataclasses land post-MVP)
- ``validate`` — dependency-free schema validator (objects, arrays, scalars,
  ``minimum``/``maximum``, ``format: uri``)
- ``llm_call`` — typed LLM call with the design §4.2 repair loop:
  schema violation → validation errors are fed back, up to ``retry`` rounds,
  then ``SchemaFailure``. Every attempt is its own trace record.
- trace store — JSONL, ``v: 1`` records (design §6.1). ``llm.call`` records
  carry inline ``input``/``output`` at MVP (the content-addressed payload
  store lands post-MVP); ``@effectful`` fns emit ``fn.return`` records.
- ``replay(path)`` → ``Trace`` (design §6.3): ``.cost_usd`` is Σ llm.call
  cost, ``.output`` is the last ``fn.return`` value as an ``AttrDict``.
  Wrong record versions raise ``ReplayMismatch``.
- replay mode — set ``NUDGE_REPLAY=<trace.jsonl>`` and ``llm_call`` reads
  outputs from the trace in order instead of calling any provider: full
  replay burns zero tokens (design §6.2). Repair rounds are replayed
  faithfully (each attempt consumes its record). Running out of records
  raises ``ReplayMismatch``. Default mode ``all`` also mocks tool calls
  from the trace; ``NUDGE_REPLAY_MODE=llm`` is the hybrid mode — LLM from
  the trace, tools executed live (and traced, so drift is visible).
- tool calls — ``tool_stub`` executes the stub and emits ``tool.call``
  trace records in live/hybrid runs (design §6.1/§8); real MCP wiring
  lands post-MVP.
- budget enforcement (design §4.3) — fake pricing is a flat $0.001/call
  (deterministic, not a model price); per-call walls via ``budget=`` and the
  run-level counter via ``NUDGE_BUDGET`` (shared by all ``par`` branches);
  overruns raise ``BudgetExceeded`` and the trace stays complete
- fake provider — deterministic, schema-driven (synthesizes conforming
  values), zero tokens. ``NUDGE_FAKE_FAIL_FIRST=k`` forces k initial schema
  violations so repair paths are testable in CI
- ``render``, ``USD``, ``effectful``, ``tool_stub``, ``AttrDict``,
  thread-pooled ``par_map`` / ``par_all`` / ``par_race`` (order-preserving,
  shared budget counter)

Env: ``NUDGE_PROVIDER=fake`` (default; real providers land post-MVP),
``NUDGE_TRACE`` (trace path, default ``trace.jsonl``), ``NUDGE_REPLAY``
(trace to replay from instead of calling a provider), ``NUDGE_BUDGET``
(run-level USD budget, §4.3).
"""

from __future__ import annotations

import functools
import json
import os
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
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
            return [f"{path}: expected null, got {_kind(value)}"]
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
        # 3 items: enough for fan-out shapes (par map over model-planned
        # subtasks) to actually exercise their cardinality in tests
        return [_synth(sch.get("items", {})) for _ in range(3)]
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
    """The budget wall was hit (design §4.3): either a single call cost more
    than its own ``budget``, or the run-level counter (``NUDGE_BUDGET``,
    shared by all ``par`` branches) ran out. The trace is complete up to the
    crash point."""


class ReplayMismatch(Exception):
    """Trace ↔ program disagreement (design §11): unsupported record
    version, missing trace, or the program made more LLM calls than the
    replayed trace holds."""


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
    compiler's job) and record every return as a ``fn.return`` trace
    record — that is what ``Trace.output`` replays in tests (§6.3)."""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            out = fn(*args, **kwargs)
            _emit_trace({"kind": "fn.return", "fn": fn.__name__, "output": _jsonable(out)})
            return out
        wrapper.__nudge_effects__ = frozenset(effects)
        return wrapper
    return deco


def _replay_mode():
    """None (live), ``"all"`` (full replay) or ``"llm"`` (hybrid: LLM from
    the trace, tools live) — design §6.2 run modes."""
    if not os.environ.get("NUDGE_REPLAY"):
        return None
    return os.environ.get("NUDGE_REPLAY_MODE", "all")


_REPLAY_TOOL_STATE = {"outputs": None, "idx": {}}


def _replay_tool_output(name):
    """Full-replay tool mock: the recorded output for this tool's next call,
    or ``[]`` when the trace holds none (design §6.2 mock default)."""
    if _REPLAY_TOOL_STATE["outputs"] is None:
        trace = Trace(os.environ["NUDGE_REPLAY"])
        by_tool = {}
        for r in trace.tool_calls():
            by_tool.setdefault(r.get("tool"), []).append(r.get("output"))
        _REPLAY_TOOL_STATE["outputs"] = by_tool
    outputs = _REPLAY_TOOL_STATE["outputs"]
    idx = _REPLAY_TOOL_STATE["idx"].get(name, 0)
    recorded = outputs.get(name, [])
    if idx < len(recorded):
        _REPLAY_TOOL_STATE["idx"][name] = idx + 1
        return recorded[idx]
    return []


def tool_stub(name, args=None):
    """Tool call while the MCP client is unbuilt (design §8).

    Live + hybrid replay: executes (stub result ``[]``) and records a
    ``tool.call`` trace record. Full replay: mocked from the trace — the
    recorded output for this tool's next call, no record written.
    """
    if _replay_mode() == "all":
        return _replay_tool_output(name)
    result = []
    _emit_trace({
        "kind": "tool.call",
        "tool": name,
        "input": _jsonable(list(args) if args is not None else []),
        "output": _jsonable(result),
    })
    return result


def python(module):
    """`import python(...)` escape hatch — lands post-MVP (v0.2)."""
    raise NotImplementedError("python() interop lands post-MVP (v0.2)")


def mcp(server):
    """`mcp("server")` tool implementations — land with the MCP client."""
    raise NotImplementedError("mcp() lands with the MCP client (post-MVP)")


# ── dynamic record values ──────────────────────────────────────────

class AttrDict(dict):
    """dict with attribute access, so generated Python can use Nudge's
    ``record.field`` syntax verbatim (``t.output.findings``)."""

    def __getattr__(self, name):
        try:
            return self[name]
        except KeyError:
            raise AttributeError(name) from None


def _attr(value):
    """Recursively wrap dicts in AttrDict (lists keep their shape)."""
    if isinstance(value, dict) and not isinstance(value, AttrDict):
        return AttrDict({k: _attr(v) for k, v in value.items()})
    if isinstance(value, list):
        return [_attr(v) for v in value]
    return value


def _jsonable(value):
    """Best-effort JSON serialization for trace payloads."""
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(v) for v in value]
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


# ── trace ────────────────────────────────────────────────────────────

def _trace_path() -> Path:
    return Path(os.environ.get("NUDGE_TRACE", "trace.jsonl"))


_TRACE_LOCK = threading.Lock()


def _emit_trace(record: dict) -> None:
    path = _trace_path()
    # serialized: par branches emit concurrently and seq must stay unique
    with _TRACE_LOCK:
        seq = 1
        if path.exists():
            seq += sum(1 for _ in path.open("r", encoding="utf-8"))
        line = {"v": 1, "seq": seq, **record}
        with path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line, ensure_ascii=False) + "\n")


def _trace_call(model, prompt, out, repair_round, outcome):
    # MVP: input/output are inline (design §6.1 content-addressed payload
    # store lands post-MVP — v1-compatible additive fields)
    _emit_trace({
        "kind": "llm.call",
        "model": model or "default",
        "params": {"temperature": 0},
        "input": str(prompt),
        "output": _jsonable(out),
        "tokens": {"in": len(str(prompt).split()), "out": len(str(out).split())},
        "cost_usd": FAKE_CALL_COST,
        "repair_round": repair_round,
        "outcome": outcome,
        "provider": "fake",
    })


# ── replay (design §6.2, §6.3) ──────────────────────────────────────

class Trace:
    """A recorded run, loaded from JSONL. Property-test input (§6.3)."""

    def __init__(self, path):
        self.path = Path(path)
        if not self.path.exists():
            raise ReplayMismatch(f"trace not found: {path}")
        self.records = []
        for line in self.path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                self.records.append(json.loads(line))
        for r in self.records:
            if r.get("v") != 1:
                raise ReplayMismatch(
                    f"unsupported trace record version {r.get('v')!r} "
                    f"(this runtime speaks v1; run `nudge trace migrate`)"
                )

    @property
    def cost_usd(self):
        return sum(r.get("cost_usd", 0.0) for r in self.llm_calls())

    @property
    def output(self):
        """The last ``fn.return`` value (dot-accessible via AttrDict)."""
        for r in reversed(self.records):
            if r.get("kind") == "fn.return":
                return _attr(r.get("output"))
        return None

    def llm_calls(self):
        return [r for r in self.records if r.get("kind") == "llm.call"]

    def tool_calls(self):
        return [r for r in self.records if r.get("kind") == "tool.call"]


def replay(path):
    """Load a recorded trace (design §6.3). IO effect at the call site."""
    return Trace(path)


_REPLAY_STATE = {"outputs": None, "idx": 0}


def _replay_outputs():
    if _REPLAY_STATE["outputs"] is None:
        trace = Trace(os.environ["NUDGE_REPLAY"])
        _REPLAY_STATE["outputs"] = [r.get("output") for r in trace.llm_calls()]
    return _REPLAY_STATE["outputs"]


# ── budget (design §4.3) ─────────────────────────────────────────────

# Fake-provider pricing: flat $0.001 per call. Deterministic, NOT a model
# price — it exists so budget walls are testable at zero token cost.
FAKE_CALL_COST = 0.001

_BUDGET_STATE = {"spent": 0.0, "lock": threading.Lock()}


def _budget_limit():
    raw = os.environ.get("NUDGE_BUDGET")
    return float(raw) if raw else None


def _budget_precheck():
    """A call whose inherited budget is already gone never starts."""
    limit = _budget_limit()
    if limit is not None:
        with _BUDGET_STATE["lock"]:
            spent = _BUDGET_STATE["spent"]
        if spent >= limit:
            raise BudgetExceeded(
                f"run budget exhausted: ${spent:.4f} spent of ${limit:.4f}"
            )


def _budget_charge(cost, call_budget):
    """Charge one call: per-call wall first, then the shared run counter."""
    if call_budget is not None and cost > float(call_budget):
        raise BudgetExceeded(
            f"call cost ${cost:.4f} exceeds its declared budget ${float(call_budget):.4f}"
        )
    limit = _budget_limit()
    if limit is not None:
        with _BUDGET_STATE["lock"]:
            _BUDGET_STATE["spent"] += cost
            spent = _BUDGET_STATE["spent"]
        if spent > limit:
            raise BudgetExceeded(
                f"run budget exceeded: ${spent:.4f} spent of ${limit:.4f}"
            )


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
    replaying = os.environ.get("NUDGE_REPLAY")
    if replaying:
        provider = "replay"
    else:
        provider = os.environ.get("NUDGE_PROVIDER", "fake")
        if provider != "fake":
            raise RuntimeError(
                "nudge_runtime MVP ships the fake provider only; set NUDGE_PROVIDER=fake"
            )

    attempts = 1 + (retry if repair and schema is not None else 0)
    last_errors, last_raw = [], None
    if provider != "replay":
        _budget_precheck()
    for round_no in range(attempts):
        if provider == "replay":
            outputs = _replay_outputs()
            if _REPLAY_STATE["idx"] >= len(outputs):
                raise ReplayMismatch(
                    "program made more llm calls than the trace holds "
                    f"({len(outputs)} records)"
                )
            out = outputs[_REPLAY_STATE["idx"]]
            _REPLAY_STATE["idx"] += 1
        else:
            out = _fake_answer(prompt, model, schema)
        if schema is None:
            if provider != "replay":
                _trace_call(model, prompt, out, 0, "ok")
                _budget_charge(FAKE_CALL_COST, budget)
            return out
        errors = validate(schema, out)
        if not errors:
            if provider != "replay":
                _trace_call(model, prompt, out, round_no, "ok")
                _budget_charge(FAKE_CALL_COST, budget)
            return out
        last_errors, last_raw = errors, out
        if provider != "replay":
            _trace_call(model, prompt, out, round_no, "schema_violation")
            _budget_charge(FAKE_CALL_COST, budget)
        # design §4.2 step 1: feed raw output errors back to the model
        prompt = _REPAIR_HINT.format(errors="; ".join(errors)) + "\n" + str(prompt)
    raise SchemaFailure(last_errors, last_raw)


# ── parallelism (design §5) ──────────────────────────────────────────


def _call_unpacked(fn, x):
    """Nudge's pair-unpacking: when the lambda takes more than one parameter
    and the element is a tuple of that arity (e.g. produced by ``zip``), it
    is spread across the parameters — ``|(a, h)| -> f(a, h)``."""
    try:
        argc = fn.__code__.co_argcount
    except AttributeError:
        argc = 1
    if argc > 1 and isinstance(x, tuple) and len(x) == argc:
        return fn(*x)
    return fn(x)


def par_map(coll, fn, concurrency=None):
    """Thread-pool fan-out. Results keep input order (map semantics); the
    budget counter is shared across branches, so a wall hit surfaces as
    ``BudgetExceeded`` from an in-flight branch (design §4.3/§5)."""
    items = list(coll)
    if not items:
        return []
    workers = concurrency or min(32, len(items))
    with ThreadPoolExecutor(max_workers=workers) as pool:
        return list(pool.map(lambda x: _call_unpacked(fn, x), items))


def par_all(items):
    """Barrier: run all branches concurrently, return results in order."""
    items = list(items)
    if not items:
        return []
    with ThreadPoolExecutor(max_workers=len(items)) as pool:
        return list(pool.map(lambda f: f() if callable(f) else f, items))


def par_race(items):
    """First completed branch wins; losers are cancelled best-effort
    (a call already in flight keeps its spend — design §5 budget refund
    is post-MVP)."""
    items = list(items)
    if not items:
        raise ValueError("par race needs at least one candidate")
    with ThreadPoolExecutor(max_workers=len(items)) as pool:
        futures = [pool.submit(lambda f: f() if callable(f) else f, it) for it in items]
        for done in as_completed(futures):
            for other in futures:
                if other is not done:
                    other.cancel()
            return done.result()
    raise ValueError("par race found no result")
