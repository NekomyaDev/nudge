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
- streaming (design §4.5) — ``llm_stream`` feeds provider chunks through an
  incremental schema validator; a prefix that can no longer satisfy the
  schema aborts the stream early (tokens saved) and counts as a schema
  violation, so the §4.2 repair loop applies. Trace records gain additive
  ``streamed`` / ``chunks`` / ``early_abort`` fields.
- agent state + checkpoints (design §7) — ``AgentState`` persists every
  state write to ``.nudge/runs/<run_id>/checkpoint.json`` and registers
  ``program``/``trace`` for the run. ``nudge resume <run_id>`` re-executes
  the program replaying the recorded prefix (``NUDGE_RESUME=1``): replayed
  state writes are suppressed (the checkpoint already reflects them), and
  once the recorded llm/tool records run out, calls go live and append to
  the same trace. Reducer writes use ``merge``: dicts union (right wins),
  lists append-dedup.
- multi-server MCP routing (design §8) — tool stubs carry their
  ``impl: mcp("server").…`` server; ``NUDGE_MCP_SERVERS`` (JSON registry)
  validates it and ``tool.call`` records gain a ``server`` field.
- OTel span export (design §6) — with ``NUDGE_OTEL=<path>`` every trace
  record is also written as an OTel-shaped JSON-lines span (file export;
  OTLP transport post-MVP).
- model routing (design §4.4) — ``route((label, model, cond), ...)`` picks
  the first arm whose condition holds (``otherwise`` is the ``None``
  fallback); the chosen arm lands as an additive ``route`` field on the
  next llm call's trace record.
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

Env: ``NUDGE_PROVIDER=fake`` (default) or a real provider — the model
string prefix (``gemini:gemini-2.5-flash``) or the env itself selects one
of ``openai | gemini | groq | mimo | mistral | anthropic | ollama`` (design
§4.6); ``NUDGE_BASE_URL``/``NUDGE_API_KEY`` (+ provider-specific key envs)
configure it,
``NUDGE_TRACE`` (trace path, default ``trace.jsonl``), ``NUDGE_REPLAY``
(trace to replay from instead of calling a provider), ``NUDGE_BUDGET``
(run-level USD budget, §4.3), ``NUDGE_REPAIR_BUDGET`` (cumulative ceiling on
repair-round spend across the run — repair is valuable, but not unbounded),
``NUDGE_RUN_ID`` (checkpoint store key,
§7), ``NUDGE_RESUME`` (with ``NUDGE_REPLAY``: continue past the recorded
prefix instead of raising ``ReplayMismatch``).
"""

from __future__ import annotations

import functools
import json
import os
import re
import sys
import threading
import time
import uuid
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
            rec = {"kind": "fn.return", "fn": fn.__name__, "output": _jsonable(out)}
            branch = _current_branch()
            if branch:
                rec["branch"] = branch
            _emit_trace(rec)
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


def _replay_tool_outputs():
    if _REPLAY_TOOL_STATE["outputs"] is None:
        trace = Trace(os.environ["NUDGE_REPLAY"])
        by_tool = {}
        for r in trace.tool_calls():
            by_tool.setdefault(r.get("tool"), []).append(r.get("output"))
        _REPLAY_TOOL_STATE["outputs"] = by_tool
    return _REPLAY_TOOL_STATE["outputs"]


def _replay_tool_output(name):
    """Full-replay tool mock: the recorded output for this tool's next call,
    or ``[]`` when the trace holds none (design §6.2 mock default)."""
    outputs = _replay_tool_outputs()
    idx = _REPLAY_TOOL_STATE["idx"].get(name, 0)
    recorded = outputs.get(name, [])
    if idx < len(recorded):
        _REPLAY_TOOL_STATE["idx"][name] = idx + 1
        return recorded[idx]
    return []


def _replay_tool_available(name):
    """True while the trace still holds an unconsumed output for this tool."""
    recorded = _replay_tool_outputs().get(name, [])
    return _REPLAY_TOOL_STATE["idx"].get(name, 0) < len(recorded)


def _mcp_registry():
    """Multi-server MCP registry (design §8, v0.3b): ``NUDGE_MCP_SERVERS``
    holds a JSON object mapping server names to their config, e.g.
    ``{"search": {"command": "python3 server.py", "tools": ["web_search"]}}``.
    v1.1d: entries with ``command`` get a real stdio JSON-RPC transport;
    entries without one keep the stub (``[]``) behavior."""
    raw = os.environ.get("NUDGE_MCP_SERVERS")
    if not raw:
        return None
    return json.loads(raw)


_MCP_SESSIONS = {}


def _mcp_call(server, name, args, cfg):
    """Real MCP transport (design §8, v1.1d): spawn the server over stdio and
    speak newline-delimited JSON-RPC (MCP stdio framing) — one persistent
    session per server: ``initialize`` → ``notifications/initialized`` →
    ``tools/call``. Registry entry needs ``"command"`` (string or argv list).
    Text content that parses as JSON is returned decoded; otherwise raw.
    Any transport or server error raises — never a silent fake result."""
    import shlex
    import subprocess

    sess = _MCP_SESSIONS.get(server)
    if sess is None:
        cmd = cfg.get("command")
        if not cmd:
            raise RuntimeError(
                f"MCP server '{server}' has no 'command' in NUDGE_MCP_SERVERS"
            )
        argv = shlex.split(cmd) if isinstance(cmd, str) else list(cmd)
        try:
            proc = subprocess.Popen(
                argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1
            )
        except OSError as e:
            raise RuntimeError(f"MCP server '{server}' failed to start ({argv[0]}): {e}")
        rid = [0]

        def request(method, params):
            rid[0] += 1
            proc.stdin.write(
                json.dumps({"jsonrpc": "2.0", "id": rid[0], "method": method, "params": params}) + "\n"
            )
            proc.stdin.flush()
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError(f"MCP server '{server}' closed the pipe during '{method}'")
            msg = json.loads(line)
            if "error" in msg:
                raise RuntimeError(f"MCP '{method}' on '{server}': {msg['error']}")
            return msg.get("result") or {}

        def notify(method, params):
            proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
            proc.stdin.flush()

        request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "nudge", "version": "1.1"},
            },
        )
        notify("notifications/initialized", {})
        sess = (proc, request)
        _MCP_SESSIONS[server] = sess

    _, request = sess
    result = request(
        "tools/call",
        {"name": name, "arguments": args if isinstance(args, dict) else {"args": list(args or [])}},
    )
    if result.get("isError"):
        raise RuntimeError(f"MCP tool '{name}' on '{server}' reported an error: {result.get('content')}")
    content = result.get("content", [])
    if len(content) == 1 and content[0].get("type") == "text":
        text = content[0].get("text", "")
        try:
            return json.loads(text)
        except ValueError:
            return text
    return content


def tool_stub(name, args=None, server=None):
    """Tool call (design §8).

    Live + hybrid replay: executes and records a ``tool.call`` trace record
    (with ``server`` when the tool declared ``impl: mcp("server").…``).
    Real transport (v1.1d): when the registry entry for ``server`` carries a
    ``command``, the call goes to the actual MCP server over stdio and the
    real output lands in the trace. Entries without ``command`` keep the
    stub result ``[]``. Full replay: mocked from the trace — no record
    written. Resume (design §7): consumes the recorded prefix, then runs
    live and records. An unknown server name fails fast.
    """
    registry = _mcp_registry() if server is not None else None
    if server is not None:
        if registry is not None and server not in registry:
            raise RuntimeError(
                f"unknown MCP server '{server}' for tool '{name}' "
                f"(registry has: {', '.join(sorted(registry))})"
            )
    if _replay_mode() == "all":
        if not os.environ.get("NUDGE_RESUME"):
            # design §6.2/v1.9: exhausting the recorded prefix WITHOUT resume
            # raises — a program that changed its tool-call pattern must fail
            # the replay, not silently mock [] (same strictness as llm calls)
            if not _replay_tool_available(name):
                raise ReplayMismatch(
                    f"program called tool '{name}' more times than the trace "
                    "holds (tool replay exhaustion raises like llm replay)"
                )
            return _replay_tool_output(name)
        if _replay_tool_available(name):
            return _replay_tool_output(name)
        # resume past the recorded prefix: fall through to a live call
    if registry is not None and registry[server].get("command"):
        result = _mcp_call(server, name, args, registry[server])
    else:
        result = []
    record = {
        "kind": "tool.call",
        "tool": name,
        "input": _jsonable(list(args) if args is not None else []),
        "output": _jsonable(result),
    }
    if server is not None:
        record["server"] = server
    branch = _current_branch()
    if branch:
        record["branch"] = branch
    _emit_trace(record)
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

# in-memory seq counter (v1.4 fix): the old code re-counted every line of
# the trace file on EVERY record — O(n²) for a run with n records, and a
# lock-held full scan bottleneck under par branches. Seeded once per path,
# then incremented in memory; empty lines no longer skew the sequence.
_SEQ = {"n": None, "path": None}


def _emit_trace(record: dict) -> None:
    path = _trace_path()
    # serialized: par branches emit concurrently and seq must stay unique
    with _TRACE_LOCK:
        if _SEQ["n"] is None or _SEQ["path"] != str(path):
            n = 0
            if path.exists():
                with path.open("r", encoding="utf-8") as f:
                    n = sum(1 for line in f if line.strip())
            _SEQ["n"], _SEQ["path"] = n, str(path)
        _SEQ["n"] += 1
        line = {"v": 1, "seq": _SEQ["n"], **record}
        with path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line, ensure_ascii=False) + "\n")
    _otel_export(line)


_OTEL_TRACE_ID = None


def _otel_export(record: dict) -> None:
    """OTel-compatible span export (design §6, v0.3d): when ``NUDGE_OTEL``
    names a path, every trace record also lands there as a JSON-lines span
    (trace_id per process, span_id per record, record fields as
    attributes). File export only — OTLP transport lands post-MVP."""
    path = os.environ.get("NUDGE_OTEL")
    if not path:
        return
    global _OTEL_TRACE_ID
    if _OTEL_TRACE_ID is None:
        _OTEL_TRACE_ID = uuid.uuid4().hex
    now_ns = time.time_ns()
    attributes = {k: v for k, v in record.items() if k not in ("v", "seq", "kind")}
    ok = record.get("outcome", "ok") == "ok"
    span = {
        "traceId": _OTEL_TRACE_ID,
        "spanId": uuid.uuid4().hex[:16],
        "name": record.get("kind", "span"),
        "kind": 3,  # SPAN_KIND_CLIENT
        "startTimeUnixNano": now_ns,
        "endTimeUnixNano": now_ns,
        "attributes": _jsonable(attributes),
        "status": {"code": 1 if ok else 2},
    }
    with _TRACE_LOCK:
        with open(path, "a", encoding="utf-8") as f:
            f.write(json.dumps(span, ensure_ascii=False) + "\n")


def _trace_call(model, prompt, out, repair_round, outcome, extra=None,
                provider="fake", tokens=None, cost=None):
    # MVP: input/output are inline (design §6.1 content-addressed payload
    # store lands post-MVP — v1-compatible additive fields)
    record = {
        "kind": "llm.call",
        "model": model or "default",
        "params": {"temperature": 0},
        "input": str(prompt),
        "output": _jsonable(out),
        "tokens": tokens or {"in": len(str(prompt).split()), "out": len(str(out).split())},
        "cost_usd": FAKE_CALL_COST if cost is None else cost,
        "repair_round": repair_round,
        "outcome": outcome,
        "provider": provider,
    }
    if extra:
        # additive v1 fields (design §6.1): streamed / chunks / early_abort
        record.update(extra)
    branch = _current_branch()
    if branch:
        record["branch"] = branch
    _emit_trace(record)


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


# ── real providers (design §4.6, v1.1a) ─────────────────────────────
# One OpenAI-compatible HTTP adapter, stdlib-only (urllib). The provider is
# chosen by the model string prefix (`gemini:gemini-2.5-flash`) or by
# NUDGE_PROVIDER; NUDGE_BASE_URL overrides the endpoint, and the key comes
# from NUDGE_API_KEY or the provider-specific env. Local/free-tier models
# price at $0 — budget walls keep working with real token counts.

_PROVIDER_BASE_URLS = {
    "openai": "https://api.openai.com/v1",
    "gemini": "https://generativelanguage.googleapis.com/v1beta/openai",
    "groq": "https://api.groq.com/openai/v1",
    "mimo": "https://token-plan-sgp.xiaomimimo.com/v1",
    "ollama": "http://localhost:11434/v1",
    "mistral": "https://api.mistral.ai/v1",
    # Anthropic speaks its own Messages API, not the OpenAI shape —
    # _complete dispatches it to _anthropic_chat below.
    "anthropic": "https://api.anthropic.com",
}

_PROVIDER_KEY_ENVS = {
    "openai": "OPENAI_API_KEY",
    "gemini": "GEMINI_API_KEY",
    "groq": "GROQ_API_KEY",
    "mimo": "MIMO_API_KEY",
    "mistral": "MISTRAL_API_KEY",
    "anthropic": "ANTHROPIC_API_KEY",
}

# USD per 1M tokens: (input, output). Models absent from the table —
# including free-tier quotas, subscription plans (e.g. MiMo token plans),
# and local Ollama models — price at $0.
_MODEL_PRICING = {
    "gemini-2.5-flash": (0.30, 2.50),
    "gemini-2.0-flash": (0.10, 0.40),
    "gpt-4o-mini": (0.15, 0.60),
    "llama-3.3-70b-versatile": (0.59, 0.79),
    "mistral-small-latest": (0.10, 0.30),
    "mistral-large-latest": (2.00, 6.00),
    "claude-haiku-4-5": (1.00, 5.00),
    "claude-sonnet-4-5": (3.00, 15.00),
}


def _split_model(model):
    """`gemini:gemini-2.5-flash` -> ("gemini", "gemini-2.5-flash");
    a bare name -> (None, model)."""
    if model and ":" in model:
        prefix, bare = model.split(":", 1)
        if prefix in _PROVIDER_BASE_URLS:
            return prefix, bare
    return None, model


def _real_provider_for(model):
    """(provider, bare_model) when a real provider should handle this call,
    else None (the fake provider handles it)."""
    prefix, bare = _split_model(model)
    if prefix:
        return prefix, bare
    env = os.environ.get("NUDGE_PROVIDER", "fake")
    if env != "fake":
        if env not in _PROVIDER_BASE_URLS:
            raise RuntimeError(
                f"unknown NUDGE_PROVIDER '{env}' "
                "(openai | gemini | groq | mimo | mistral | anthropic | ollama | fake)"
            )
        return env, bare
    return None


def _openai_chat(provider, model, prompt):
    """One non-streaming chat completion against an OpenAI-compatible API.
    Returns (text, prompt_tokens, completion_tokens)."""
    import urllib.error
    import urllib.request
    base = os.environ.get("NUDGE_BASE_URL", _PROVIDER_BASE_URLS[provider])
    key_env = _PROVIDER_KEY_ENVS.get(provider)
    key = os.environ.get("NUDGE_API_KEY") or (os.environ.get(key_env, "") if key_env else "")
    body = json.dumps({
        "model": model,
        "messages": [{"role": "user", "content": str(prompt)}],
    }).encode()
    req = urllib.request.Request(
        base.rstrip("/") + "/chat/completions", data=body,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {key}",
            # Cloudflare (error 1010) bans urllib's default UA on some providers
            "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36",
        },
    )
    data = None
    last_err = None
    # 429s are routine on free tiers — back off and retry (5s, 25s, 125s)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                data = json.loads(resp.read())
            break
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:500]
            last_err = RuntimeError(f"{provider} provider HTTP {e.code}: {detail}")
            if e.code == 429 and attempt < 3:
                time.sleep(5 * (5 ** attempt))
                continue
            raise last_err
        except urllib.error.URLError as e:
            raise RuntimeError(f"{provider} provider unreachable: {e.reason}")
    try:
        text = data["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError):
        raise RuntimeError(
            f"{provider} provider returned an unexpected payload: {str(data)[:500]}"
        )
    usage = data.get("usage") or {}
    return text, int(usage.get("prompt_tokens") or 0), int(usage.get("completion_tokens") or 0)


def _anthropic_chat(provider, model, prompt):
    """One non-streaming call against Anthropic's Messages API (not the
    OpenAI shape). Returns (text, input_tokens, output_tokens)."""
    import urllib.error
    import urllib.request
    base = os.environ.get("NUDGE_BASE_URL", _PROVIDER_BASE_URLS[provider])
    key = os.environ.get("NUDGE_API_KEY") or os.environ.get(
        _PROVIDER_KEY_ENVS[provider], "")
    body = json.dumps({
        "model": model,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": str(prompt)}],
    }).encode()
    req = urllib.request.Request(
        base.rstrip("/") + "/v1/messages", data=body,
        headers={
            "Content-Type": "application/json",
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
        },
    )
    data = None
    last_err = None
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                data = json.loads(resp.read())
            break
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:500]
            last_err = RuntimeError(f"{provider} provider HTTP {e.code}: {detail}")
            if e.code == 429 and attempt < 3:
                time.sleep(5 * (5 ** attempt))
                continue
            raise last_err
        except urllib.error.URLError as e:
            raise RuntimeError(f"{provider} provider unreachable: {e.reason}")
    try:
        blocks = [b.get("text", "") for b in data["content"] if b.get("type") == "text"]
        text = "".join(blocks)
    except (KeyError, TypeError, AttributeError):
        raise RuntimeError(
            f"{provider} provider returned an unexpected payload: {str(data)[:500]}"
        )
    usage = data.get("usage") or {}
    return text, int(usage.get("input_tokens") or 0), int(usage.get("output_tokens") or 0)


def _extract_json(text):
    """Best-effort JSON extraction from a real model's answer: ```json
    fences first, then the first balanced-looking {...} / [...] span. A
    failure returns the raw text — schema validation reports it, and the
    §4.2 repair loop gets its chance."""
    s = text.strip()
    if s.startswith("```"):
        s = re.sub(r"^```(?:json)?\s*", "", s)
        s = re.sub(r"\s*```$", "", s)
    try:
        return json.loads(s)
    except json.JSONDecodeError:
        pass
    for i, ch in enumerate(s):
        if ch in "{[":
            closer = "}" if ch == "{" else "]"
            end = s.rfind(closer)
            if end > i:
                try:
                    return json.loads(s[i:end + 1])
                except json.JSONDecodeError:
                    pass
            break
    return text


def _complete(provider, model, prompt, schema):
    """(output, in_tokens, out_tokens) — one completion on the given
    provider. Real-provider answers are JSON-extracted when a schema is
    set; the fake provider synthesizes as before."""
    if provider == "fake":
        return _fake_answer(prompt, model, schema), 0, 0
    bare = _split_model(model)[1]
    if provider == "anthropic":
        text, in_t, out_t = _anthropic_chat(provider, bare, prompt)
    else:
        text, in_t, out_t = _openai_chat(provider, bare, prompt)
    if schema is not None:
        return _extract_json(text), in_t, out_t
    return text, in_t, out_t


def _call_cost(provider, model, in_t, out_t):
    """USD cost of one call: flat fake pricing, or the pricing table for
    real providers (unknown/free/local models → $0)."""
    if provider == "fake":
        return FAKE_CALL_COST
    prices = _MODEL_PRICING.get(_split_model(model)[1])
    if prices is None:
        return 0.0
    return (in_t * prices[0] + out_t * prices[1]) / 1_000_000


_BUDGET_STATE = {"spent": 0.0, "lock": threading.Lock()}

_REPAIR_BUDGET_STATE = {"spent": 0.0, "lock": threading.Lock()}


def _repair_budget_limit():
    raw = os.environ.get("NUDGE_REPAIR_BUDGET")
    return float(raw) if raw else None


def _repair_budget_precheck():
    """Repair rounds share a cumulative, run-level ceiling. Reasoning models
    can make a single repair round cost more than the original call — the
    wall keeps 'fix it' from silently outspending the work itself."""
    limit = _repair_budget_limit()
    if limit is not None:
        with _REPAIR_BUDGET_STATE["lock"]:
            spent = _REPAIR_BUDGET_STATE["spent"]
        if spent >= limit:
            raise BudgetExceeded(
                f"repair budget exhausted: ${spent:.4f} spent of ${limit:.4f} "
                "(NUDGE_REPAIR_BUDGET caps cumulative repair-round spend)"
            )


def _repair_budget_charge(cost):
    if _repair_budget_limit() is not None:
        with _REPAIR_BUDGET_STATE["lock"]:
            _REPAIR_BUDGET_STATE["spent"] += cost



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


# ── agent state + checkpoints (design §7) ──────────────────────────

class AgentState:
    """Checkpointed agent state (design §7, v0.2c MVP).

    Every attribute write persists the full state to
    ``.nudge/runs/<run_id>/checkpoint.json`` (SQLite/Postgres stores are
    post-MVP). The run directory also registers ``program`` (the emitted
    entry file) and ``trace`` so ``nudge resume <run_id>`` can re-execute.

    Resume semantics: with ``NUDGE_RESUME`` set, the checkpoint is loaded
    and the first ``writes`` state writes of the re-execution are
    suppressed — deterministic replay of the recorded prefix reproduces
    exactly those writes, and the checkpoint already reflects them. Writes
    past the crash point go live and checkpoint as usual.
    """

    def __init__(self, agent, defaults):
        object.__setattr__(self, "_agent", agent)
        run = os.environ.get("NUDGE_RUN_ID") or f"run-{os.getpid()}"
        run_dir = Path(".nudge") / "runs" / run
        run_dir.mkdir(parents=True, exist_ok=True)
        object.__setattr__(self, "_dir", run_dir)
        values, writes = dict(defaults), 0
        ckpt = run_dir / "checkpoint.json"
        saved_values = None
        resuming = bool(os.environ.get("NUDGE_RESUME")) and ckpt.exists()
        if resuming:
            saved = json.loads(ckpt.read_text(encoding="utf-8"))
            writes = saved.get("writes", 0)
            saved_values = _jsonable(saved.get("values", {}))
            # Replay starts from the DEFAULTS, not the checkpoint: suppressed
            # writes are re-applied so augmented writes (+=) accumulate
            # correctly, and the prefix end is verified against the recorded
            # checkpoint (v1.6 divergence guard). Loading the checkpoint
            # would double-apply every += of the prefix.
        object.__setattr__(self, "_values", values)
        object.__setattr__(self, "_writes", writes)
        object.__setattr__(self, "_suppress", writes if os.environ.get("NUDGE_RESUME") else 0)
        # divergence guard reference: the recorded final values the
        # replayed prefix must reproduce (v1.6 — used to be unchecked)
        object.__setattr__(self, "_saved_values", saved_values)
        # NUDGE_PROGRAM overrides the registered entry file — `nudgec test`
        # runs the module through a driver script, so sys.argv[0] would
        # otherwise point `nudgec resume` at the wrong file
        program = os.environ.get("NUDGE_PROGRAM") or os.path.abspath(sys.argv[0])
        (run_dir / "program").write_text(program, encoding="utf-8")
        trace = os.environ.get("NUDGE_TRACE")
        if trace:
            (run_dir / "trace").write_text(os.path.abspath(trace), encoding="utf-8")
        if not resuming:
            self._checkpoint()
        # on resume the recorded checkpoint must survive until the replayed
        # prefix is verified — writing defaults over it here would destroy
        # both the resume point and the divergence-guard reference

    def __getattr__(self, name):
        try:
            return object.__getattribute__(self, "_values")[name]
        except KeyError:
            raise AttributeError(name) from None

    def __setattr__(self, name, value):
        if self._suppress > 0:
            # replayed-prefix write: APPLY it (so a diverged replay is
            # visible in the values) but don't checkpoint — the recorded
            # checkpoint already reflects a faithful prefix
            self._values[name] = value
            object.__setattr__(self, "_suppress", self._suppress - 1)
            if self._suppress == 0:
                self._guard_replay_faithful()
            return
        self._values[name] = value
        object.__setattr__(self, "_writes", self._writes + 1)
        self._checkpoint()

    def _guard_replay_faithful(self):
        """Resume divergence guard (v1.6): once the recorded prefix has been
        replayed, the reproduced state must equal the recorded checkpoint —
        otherwise the program changed since the crash and continuing would
        silently fork history. (A replay with FEWER writes than the prefix
        never reaches this point; that case is caught by llm/tool replay
        divergence instead.)"""
        saved = self._saved_values
        if saved is None:
            return
        now = _jsonable(self._values)
        if now != saved:
            raise ReplayMismatch(
                f"resume divergence in agent '{self._agent}': the replayed "
                f"state {now!r} does not match the recorded checkpoint "
                f"{saved!r} — the program changed since the crash; start a "
                f"new run"
            )

    def __repr__(self):
        return f"AgentState({self._agent!r}, {self._values!r})"

    def _checkpoint(self):
        payload = {
            "agent": self._agent,
            "values": _jsonable(self._values),
            "writes": self._writes,
        }
        (self._dir / "checkpoint.json").write_text(
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )


# ── model routing (design §4.4) ─────────────────────────────────────

_LAST_ROUTE = threading.local()

# NTF v1.1 (additive): records emitted inside a `par` branch carry a `branch`
# label — "par[0]", "par[1]", ... — so trace-view/trace-diff can separate
# parallel lanes. Frozen-v1 compatible (additive field, design §6.1).
_BRANCH = threading.local()


def _current_branch():
    return getattr(_BRANCH, "id", None)


def _run_with_branch(label, fn, x):
    prev = getattr(_BRANCH, "id", None)
    _BRANCH.id = label
    try:
        return _call_unpacked(fn, x)
    finally:
        _BRANCH.id = prev



def route(*arms):
    """User-defined model routing (design §4.4, v0.4): arms are
    ``(label, model, cond_fn_or_None)`` triples evaluated in order; the
    first arm whose condition is truthy wins, the arm with ``None`` is the
    ``otherwise`` fallback. The chosen label is picked up by the next
    ``llm_call``/``llm_stream`` as an additive ``route`` trace field."""
    chosen = None
    for label, model, cond in arms:
        if cond is None:
            if chosen is None:
                chosen = (label, model)
            break
        if cond():
            chosen = (label, model)
            break
    if chosen is None:
        raise RuntimeError("route block matched no arm and has no otherwise fallback")
    _LAST_ROUTE.choice = chosen
    return chosen[1]


def _take_route_label():
    choice = getattr(_LAST_ROUTE, "choice", None)
    _LAST_ROUTE.choice = None
    return choice[0] if choice else None


# ── merge reducer (design §7) ────────────────────────────────────────

def merge(l, r):
    """CRDT-style join behind `l | merge r` (design §7): dicts union
    (right side wins on key conflicts), lists append items the left side
    does not already hold (grow-only set), and anything else is
    overwritten by the right side."""
    if isinstance(l, dict) and isinstance(r, dict):
        return {**l, **r}
    if isinstance(l, list) and isinstance(r, list):
        out = list(l)
        for x in r:
            if x not in out:
                out.append(x)
        return out
    return r


# ── streaming (design §4.5) ──────────────────────────────────────────

class _PrefixImpossible(Exception):
    """The streamed prefix can no longer satisfy the schema (design §4.5)."""


class _PrefixValidator:
    """Incremental JSON-stream viability checker (design §4.5).

    Chunks are fed as they arrive; :meth:`feed` raises
    :class:`_PrefixImpossible` the moment *no* completion of the prefix can
    satisfy the schema, so the runtime can abort the stream early (tokens
    not yet spent are saved). Abort conditions: a literal of the wrong type
    starts, a number completes outside ``minimum``/``maximum`` or non-
    integral under ``integer``, a ``format: uri`` string completes invalid,
    an object closes with a ``required`` key missing, or the JSON itself
    malforms. Unknown object keys are allowed (JSON-Schema default) and a
    ``{}`` schema accepts anything.
    """

    def __init__(self, sch):
        self.sch = sch if isinstance(sch, dict) else {}
        self.stack = []    # open object/array frames
        self.scalar = None # in-progress string/key/number/literal
        self.done = False  # root value completed

    def feed(self, text):
        for ch in text:
            self._feed_char(ch)

    # ── schema helpers ────────────────────────────────────────────
    @staticmethod
    def _type_of(sch):
        return sch.get("type") if isinstance(sch, dict) else None

    def _check_start(self, ch, sch):
        t = self._type_of(sch)
        if t is None:
            return
        ok = {
            "object": ch == "{",
            "array": ch == "[",
            "string": ch == '"',
            "number": ch == "-" or ch.isdigit(),
            "integer": ch == "-" or ch.isdigit(),
            "boolean": ch in "tf",
            "null": ch == "n",
        }.get(t)
        if ok is False:
            raise _PrefixImpossible(f"expected {t}, value starts with {ch!r}")

    # ── char machine ──────────────────────────────────────────────
    def _feed_char(self, ch):
        if self.done:
            if not ch.isspace():
                raise _PrefixImpossible("trailing data after complete document")
            return
        if self.scalar is not None:
            self._feed_scalar(ch)
            return
        if ch.isspace():
            return
        if not self.stack:
            self._start_value(ch, self.sch)
            return
        f = self.stack[-1]
        if f["kind"] == "obj":
            st = f["state"]
            if st == "key":
                if ch == '"':
                    self.scalar = {"kind": "key", "buf": "", "esc": False}
                elif ch == "}":
                    self._close_obj(f)
                else:
                    raise _PrefixImpossible(f"object expects a key or '}}', got {ch!r}")
            elif st == "colon":
                if ch == ":":
                    f["state"] = "value"
                else:
                    raise _PrefixImpossible(f"expected ':', got {ch!r}")
            elif st == "value":
                f["state"] = "comma"
                subsch = {}
                if isinstance(f["sch"], dict):
                    subsch = f["sch"].get("properties", {}).get(f["key"], {})
                self._start_value(ch, subsch)
            else:  # comma
                if ch == ",":
                    f["state"] = "key"
                elif ch == "}":
                    self._close_obj(f)
                else:
                    raise _PrefixImpossible(f"object expects ',' or '}}', got {ch!r}")
        else:  # arr
            if f["state"] == "value":
                if ch == "]":
                    self.stack.pop()
                    self._after_value()
                else:
                    f["state"] = "comma"
                    subsch = f["sch"].get("items", {}) if isinstance(f["sch"], dict) else {}
                    self._start_value(ch, subsch)
            else:  # comma
                if ch == ",":
                    f["state"] = "value"
                elif ch == "]":
                    self.stack.pop()
                    self._after_value()
                else:
                    raise _PrefixImpossible(f"array expects ',' or ']', got {ch!r}")

    def _start_value(self, ch, sch):
        self._check_start(ch, sch)
        if ch == "{":
            self.stack.append({"kind": "obj", "sch": sch, "state": "key",
                               "key": None, "seen": set()})
        elif ch == "[":
            self.stack.append({"kind": "arr", "sch": sch, "state": "value"})
        elif ch == '"':
            self.scalar = {"kind": "string", "sch": sch, "buf": "", "esc": False}
        elif ch == "-" or ch.isdigit():
            self.scalar = {"kind": "number", "sch": sch, "buf": ch}
        else:
            self.scalar = {"kind": "literal", "sch": sch, "buf": ch}

    def _feed_scalar(self, ch):
        s = self.scalar
        if s["kind"] in ("string", "key"):
            if s["esc"]:
                s["esc"] = False
                s["buf"] += ch
            elif ch == "\\":
                s["esc"] = True
            elif ch == '"':
                self.scalar = None
                if s["kind"] == "key":
                    f = self.stack[-1]
                    f["key"] = s["buf"]
                    f["seen"].add(s["buf"])
                    f["state"] = "colon"
                else:
                    sch = s["sch"]
                    if isinstance(sch, dict) and sch.get("format") == "uri":
                        from urllib.parse import urlparse
                        parsed = urlparse(s["buf"])
                        if not (parsed.scheme and parsed.netloc):
                            raise _PrefixImpossible(f"not a valid uri: {s['buf']!r}")
                    self._after_value()
            else:
                s["buf"] += ch
        elif s["kind"] == "number":
            if ch in "0123456789+-.eE":
                s["buf"] += ch
            else:
                self.scalar = None
                try:
                    num = float(s["buf"])
                except ValueError:
                    raise _PrefixImpossible(f"malformed number {s['buf']!r}")
                sch = s["sch"]
                if isinstance(sch, dict):
                    if sch.get("type") == "integer" and num != int(num):
                        raise _PrefixImpossible(f"{s['buf']} is not an integer")
                    if "minimum" in sch and num < sch["minimum"]:
                        raise _PrefixImpossible(f"{num} < minimum {sch['minimum']}")
                    if "maximum" in sch and num > sch["maximum"]:
                        raise _PrefixImpossible(f"{num} > maximum {sch['maximum']}")
                self._after_value()
                self._feed_char(ch)  # the delimiter belongs to the parent
        else:  # literal: true / false / null
            s["buf"] += ch
            buf = s["buf"]
            if not any(w.startswith(buf) for w in ("true", "false", "null")):
                raise _PrefixImpossible(f"malformed literal {buf!r}")
            if buf in ("true", "false", "null"):
                self.scalar = None
                sch = s["sch"]
                t = self._type_of(sch)
                if t == "boolean" and buf == "null":
                    raise _PrefixImpossible("expected boolean, got null")
                if t == "null" and buf != "null":
                    raise _PrefixImpossible(f"expected null, got {buf}")
                self._after_value()

    def _close_obj(self, f):
        if isinstance(f["sch"], dict):
            missing = [k for k in f["sch"].get("required", []) if k not in f["seen"]]
            if missing:
                raise _PrefixImpossible(f"object closed missing required {missing[0]!r}")
        self.stack.pop()
        self._after_value()

    def _after_value(self):
        if not self.stack:
            self.done = True


def llm_stream(prompt, model=None, schema=None, retry=0, repair=False,
               budget=None, cache=None, tags=None, chunk_size=14):
    """One streaming typed LLM call (design §4.5).

    The answer arrives in chunks; with ``schema`` set, every prefix is
    validated incrementally (:class:`_PrefixValidator`) and a prefix that
    can no longer satisfy the schema aborts the stream early — the abort
    counts as a schema violation, so the §4.2 repair loop applies. Trace
    records carry additive ``streamed``/``chunks``/``early_abort`` fields
    (§6.1). MVP: the fake provider chunks deterministically (``chunk_size``
    characters); replay consumes the recorded final value like
    :func:`llm_call` (§6.2 — stream flags stay in the old trace).
    """
    if os.environ.get("NUDGE_REPLAY"):
        return llm_call(prompt, model=model, schema=schema, retry=retry,
                        repair=repair, budget=budget, cache=cache, tags=tags)
    real = _real_provider_for(model)
    if real is not None:
        # v1.1a: real providers run non-streaming for now (token streaming
        # lands with the SSE adapter) — the call behaves like llm_call
        return llm_call(prompt, model=model, schema=schema, retry=retry,
                        repair=repair, budget=budget, cache=cache, tags=tags)

    attempts = 1 + (retry if repair and schema is not None else 0)
    last_errors, last_raw = [], None
    _budget_precheck()
    # design §4.4: a model chosen via rt.route carries its arm label
    route_label = _take_route_label()
    # design §4.3 (v1.20 fix): the declared budget caps the WHOLE call site,
    # repair rounds included — same wall as llm_call
    site_spent = [0.0]
    def charge_site(cost):
        remaining = None if budget is None else float(budget) - site_spent[0]
        if remaining is not None and cost > remaining:
            raise BudgetExceeded(
                f"call site budget exhausted: round cost ${cost:.4f} with "
                f"${remaining:.4f} left of the declared ${float(budget):.4f} "
                f"(repair rounds share the site budget)"
            )
        site_spent[0] += cost
        _budget_charge(cost, None)
        if round_no >= 1:
            _repair_budget_charge(cost)

    def _x(d):
        return {**d, "route": route_label} if route_label else d

    for round_no in range(attempts):
        if round_no >= 1:
            _repair_budget_precheck()
        out = _fake_answer(prompt, model, schema)
        if schema is not None:
            text = json.dumps(_jsonable(out), ensure_ascii=False)
        else:
            text = str(out)
        chunks = [text[i:i + chunk_size] for i in range(0, len(text), chunk_size)] or [""]
        validator = _PrefixValidator(schema) if schema is not None else None
        aborted, consumed = None, 0
        for chunk in chunks:
            consumed += 1
            if validator is not None:
                try:
                    validator.feed(chunk)
                except _PrefixImpossible as e:
                    aborted = str(e)
                    break
        if aborted is not None:
            last_errors, last_raw = [f"stream aborted: {aborted}"], out
            _trace_call(model, prompt, out, round_no, "schema_violation",
                        extra=_x({"streamed": True, "chunks": consumed, "early_abort": True}))
            charge_site(FAKE_CALL_COST)
            # design §4.5: an unsatisfiable prefix aborts early and triggers repair
            prompt = _REPAIR_HINT.format(errors="stream aborted: " + aborted) + "\n" + str(prompt)
            continue
        if schema is None:
            _trace_call(model, prompt, out, 0, "ok",
                        extra=_x({"streamed": True, "chunks": consumed}))
            charge_site(FAKE_CALL_COST)
            return out
        errors = validate(schema, out)
        if not errors:
            _trace_call(model, prompt, out, round_no, "ok",
                        extra=_x({"streamed": True, "chunks": consumed}))
            charge_site(FAKE_CALL_COST)
            return out
        last_errors, last_raw = errors, out
        _trace_call(model, prompt, out, round_no, "schema_violation",
                    extra=_x({"streamed": True, "chunks": consumed}))
        charge_site(FAKE_CALL_COST)
        # design §4.2 step 1: feed raw output errors back to the model
        prompt = _REPAIR_HINT.format(errors="; ".join(errors)) + "\n" + str(prompt)
    raise SchemaFailure(last_errors, last_raw)


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
        provider, real = "replay", None
    else:
        real = _real_provider_for(model)
        provider = real[0] if real else "fake"

    attempts = 1 + (retry if repair and schema is not None else 0)
    last_errors, last_raw = [], None
    if provider != "replay":
        _budget_precheck()
    # design §4.4: a model chosen via rt.route carries its arm label
    route_label = _take_route_label()
    route_extra = {"route": route_label} if route_label else None
    # design §4.3: the declared `budget` caps the WHOLE call site, repair
    # rounds included — each round is charged against what remains
    site_spent = [0.0]
    def charge_site(cost):
        remaining = None if budget is None else float(budget) - site_spent[0]
        if remaining is not None and cost > remaining:
            raise BudgetExceeded(
                f"call site budget exhausted: round cost ${cost:.4f} with "
                f"${remaining:.4f} left of the declared ${float(budget):.4f} "
                f"(repair rounds share the site budget)"
            )
        site_spent[0] += cost
        _budget_charge(cost, None)
        if round_no >= 1:
            _repair_budget_charge(cost)
    for round_no in range(attempts):
        if round_no >= 1 and provider != "replay":
            _repair_budget_precheck()
        if provider == "replay":
            outputs = _replay_outputs()
            if _REPLAY_STATE["idx"] >= len(outputs):
                if not os.environ.get("NUDGE_RESUME"):
                    raise ReplayMismatch(
                        "program made more llm calls than the trace holds "
                        f"({len(outputs)} records)"
                    )
                # resume (design §7): the recorded prefix is exhausted —
                # continue live against the fake provider and trace it
                provider = real[0] if real else "fake"
                out, in_t, out_t = _complete(provider, model, prompt, schema)
            else:
                out = outputs[_REPLAY_STATE["idx"]]
                _REPLAY_STATE["idx"] += 1
        else:
            out, in_t, out_t = _complete(provider, model, prompt, schema)
        if schema is None:
            if provider != "replay":
                _trace_call(model, prompt, out, 0, "ok", extra=route_extra,
                            provider=provider, tokens={"in": in_t, "out": out_t},
                            cost=_call_cost(provider, model, in_t, out_t))
                charge_site(_call_cost(provider, model, in_t, out_t))
            return out
        errors = validate(schema, out)
        if not errors:
            if provider != "replay":
                _trace_call(model, prompt, out, round_no, "ok", extra=route_extra,
                            provider=provider, tokens={"in": in_t, "out": out_t},
                            cost=_call_cost(provider, model, in_t, out_t))
                charge_site(_call_cost(provider, model, in_t, out_t))
            # validated records support Nudge's `.field` syntax (AttrDict)
            return _attr(out)
        last_errors, last_raw = errors, out
        if provider != "replay":
            _trace_call(model, prompt, out, round_no, "schema_violation", extra=route_extra,
                        provider=provider, tokens={"in": in_t, "out": out_t},
                        cost=_call_cost(provider, model, in_t, out_t))
            charge_site(_call_cost(provider, model, in_t, out_t))
        # design §4.2 step 1: feed raw output errors back to the model
        prompt = _REPAIR_HINT.format(errors="; ".join(errors)) + "\n" + str(prompt)
    raise SchemaFailure(last_errors, last_raw)


# ── parallelism (design §5) ──────────────────────────────────────────


def _call_unpacked(fn, x):
    """Nudge's pair-unpacking: when the lambda takes more than one parameter
    and the element is a pair (a tuple, or the ``{first, second}`` record
    produced by :func:`zip`), it is spread across the parameters —
    ``|(a, h)| -> f(a, h)``."""
    try:
        argc = fn.__code__.co_argcount
    except AttributeError:
        argc = 1
    if argc > 1:
        if isinstance(x, tuple) and len(x) == argc:
            return fn(*x)
        if isinstance(x, dict) and argc == 2 and "first" in x and "second" in x:
            return fn(x["first"], x["second"])
    return fn(x)


_py_zip = zip


def zip(a, b):
    """Nudge `a zip b` — pairwise zip as a REUSABLE list of ``{first,
    second}`` records, matching the checker's type model (so `.first` /
    `.second` field access works) while :func:`_call_unpacked` still spreads
    pairs across multi-param ``par map`` lambdas. (Python's builtin zip
    yields one-shot tuples — field access on them was an AttributeError.)"""
    return [AttrDict({"first": x, "second": y}) for x, y in _py_zip(a, b)]


def par_map(coll, fn, concurrency=None):
    """Thread-pool fan-out. Results keep input order (map semantics); the
    budget counter is shared across branches, so a wall hit surfaces as
    ``BudgetExceeded`` from an in-flight branch (design §4.3/§5)."""
    items = list(coll)
    if not items:
        return []
    workers = concurrency or min(32, len(items))
    with ThreadPoolExecutor(max_workers=workers) as pool:
        return list(pool.map(
            lambda ix: _run_with_branch(f"par[{ix[0]}]", fn, ix[1]),
            enumerate(items),
        ))


def par_all(items):
    """Barrier: run all branches concurrently, return results in order."""
    items = list(items)
    if not items:
        return []
    with ThreadPoolExecutor(max_workers=len(items)) as pool:
        return list(pool.map(
            lambda ix: _run_with_branch(f"par[{ix[0]}]", (lambda f: f() if callable(f) else f), ix[1]),
            enumerate(items),
        ))


def par_race(items):
    """First completed branch wins; losers are cancelled best-effort
    (a call already in flight keeps its spend — design §5 budget refund
    is post-MVP).

    v1.3 fix: the pool no longer joins on exit — the old
    ``with ThreadPoolExecutor(...)`` block made ``shutdown(wait=True)``
    wait for every losing branch before returning, so a "race" took as
    long as the SLOWEST candidate. Losers now keep running in the
    background while the winner's result returns immediately."""
    items = list(items)
    if not items:
        raise ValueError("par race needs at least one candidate")
    pool = ThreadPoolExecutor(max_workers=len(items))
    futures = [
        pool.submit(_run_with_branch, f"par[{i}]", (lambda f: f() if callable(f) else f), it)
        for i, it in enumerate(items)
    ]
    try:
        for done in as_completed(futures):
            for other in futures:
                if other is not done:
                    other.cancel()
            return done.result()
    finally:
        pool.shutdown(wait=False, cancel_futures=True)
    raise ValueError("par race found no result")
