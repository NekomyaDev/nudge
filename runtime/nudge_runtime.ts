// nudge_runtime.ts — TypeScript runtime for the Nudge TS backend (v0.3c MVP).
// @ts-nocheck — vendored, plain-JS style on purpose: runs under node as-is
// (renamed .mjs in tests) and compiles under tsc/deno. Strict-mode users'
// tsc should not type-check generated/vendor files; the runtime's own
// conformance is covered by the compiler's e2e suite. Subset: schema/llmCall/toolStub/replay,
// budget walls, render, merge, USD. Deferred: streaming,
// par scheduling, OTel export (the Python runtime covers those today).
import * as fs from "node:fs";
import * as process from "node:process";

export function schema(s) {
  return s;
}

export function extend(base, extra) {
  return { ...base, ...extra };
}

export function USD(v) {
  return parseFloat(v);
}

export function render(tpl, vars) {
  return tpl.replace(/\{([^}]+)\}/g, (m, k) => (vars[k] !== undefined ? String(vars[k]) : m));
}

export function zip(a, b) {
  const out = [];
  for (let i = 0; i < Math.min(a.length, b.length); i++) out.push({ first: a[i], second: b[i] });
  return out;
}

// CRDT-style join behind `l | merge r` (design §7): objects union (right
// wins), arrays append items the left side does not already hold.
export function merge(l, r) {
  if (Array.isArray(l) && Array.isArray(r)) {
    const out = l.slice();
    for (const x of r) if (!out.some((y) => JSON.stringify(y) === JSON.stringify(x))) out.push(x);
    return out;
  }
  if (l && r && typeof l === "object" && typeof r === "object") return { ...l, ...r };
  return r;
}

// User-defined model routing (design §4.4): arms are [label, model, cond]
// triples; the first truthy condition wins, `null` is the otherwise arm.
export function route(...arms) {
  for (const [label, model, cond] of arms) {
    if (cond === null || cond()) return model;
  }
  throw new Error("route block matched no arm and has no otherwise fallback");
}

const FAKE_CALL_COST = 0.001;
let _spent = 0;

function _tracePath() {
  return process.env.NUDGE_TRACE || "trace.jsonl";
}

function _emitTrace(record) {
  const path = _tracePath();
  let seq = 1;
  if (fs.existsSync(path)) {
    seq += fs.readFileSync(path, "utf8").split("\n").filter(Boolean).length;
  }
  fs.appendFileSync(path, JSON.stringify({ v: 1, seq, ...record }) + "\n");
}

function _budgetCharge(cost, budget) {
  // parity with the python runtime (design §4.3): the declared `budget` is a
  // PER-CALL wall against this call's own cost; NUDGE_BUDGET is the separate
  // run-level cap on total spend. (Previously the per-call budget was
  // wrongly applied against the cumulative run total — and float dust like
  // 0.050000000000000003 could trip the wall, hence the epsilon.)
  if (budget !== null && budget !== undefined && cost > budget + 1e-9) {
    const err = new Error(`BudgetExceeded: call cost $${cost.toFixed(4)} exceeds its declared budget $${budget}`);
    err.name = "BudgetExceeded";
    throw err;
  }
  _spent += cost;
  const runWall = process.env.NUDGE_BUDGET ? parseFloat(process.env.NUDGE_BUDGET) : null;
  if (runWall !== null && _spent > runWall + 1e-9) {
    const err = new Error(`BudgetExceeded: run spent $${_spent.toFixed(4)} > budget $${runWall}`);
    err.name = "BudgetExceeded";
    throw err;
  }
}

function _synth(sch) {
  if (!sch || typeof sch !== "object") return null;
  switch (sch.type) {
    case "object": {
      const out = {};
      for (const k of sch.required || Object.keys(sch.properties || {})) {
        out[k] = _synth((sch.properties || {})[k]);
      }
      return out;
    }
    case "array":
      // 3 items: parity with the python runtime's fake provider, so fan-out
      // shapes exercise the same cardinality on both backends
      return [_synth(sch.items), _synth(sch.items), _synth(sch.items)];
    case "string":
      return "fake-text";
    case "integer":
      return 1;
    case "number":
      return 0.5;
    case "boolean":
      return true;
    case "null":
      return null;
    default:
      return null;
  }
}

let _replayOutputsCache = null;
let _replayIdx = 0;

function _replayOutputs() {
  if (_replayOutputsCache === null) {
    const p = process.env.NUDGE_REPLAY;
    _replayOutputsCache = p
      ? fs.readFileSync(p, "utf8").split("\n").filter(Boolean).map(JSON.parse)
          .filter((r) => r.kind === "llm.call").map((r) => r.output)
      : [];
  }
  return _replayOutputsCache;
}

export function llmCall(opts) {
  const { prompt, model = null, schema: sch = null, budget = null } = opts;
  if (process.env.NUDGE_REPLAY) {
    const outs = _replayOutputs();
    if (_replayIdx >= outs.length) {
      throw new Error(`ReplayMismatch: program made more llm calls than the trace holds (${outs.length} records)`);
    }
    // replayed calls are not traced or charged (parity with the python runtime)
    return outs[_replayIdx++];
  }
  // v1.1a: real providers are Python-only for now (OpenAI-compatible
  // adapter ships in nudge_runtime; the TS adapter lands with async codegen)
  const prefix = model && model.includes(":") ? model.split(":")[0] : null;
  if ((process.env.NUDGE_PROVIDER && process.env.NUDGE_PROVIDER !== "fake") ||
      (prefix && ["openai", "gemini", "groq", "ollama"].includes(prefix))) {
    throw new Error("nudge_runtime.ts: real providers run on the Python runtime at v1.1a — compile with `nudgec build` for provider access");
  }
  const out = sch ? _synth(sch) : `[fake:${model}] ${prompt}`;
  // frozen v1 trace schema (design §6.1): the same field set the python
  // runtime emits — `nudgec trace-check` validates these as required
  _emitTrace({
    kind: "llm.call",
    model: model || "fake",
    params: { temperature: 0 },
    input: String(prompt),
    output: out,
    tokens: {
      in: String(prompt).split(/\s+/).filter(Boolean).length,
      out: String(out).split(/\s+/).filter(Boolean).length,
    },
    cost_usd: FAKE_CALL_COST,
    repair_round: 0,
    outcome: "ok",
    provider: "fake",
  });
  _budgetCharge(FAKE_CALL_COST, budget);
  return out;
}

let _replayToolCache = null;
const _replayToolIdx = {};

function _replayToolOutputs() {
  if (_replayToolCache === null) {
    const p = process.env.NUDGE_REPLAY;
    _replayToolCache = {};
    if (p) {
      for (const r of fs.readFileSync(p, "utf8").split("\n").filter(Boolean).map(JSON.parse)
        .filter((r) => r.kind === "tool.call")) {
        (_replayToolCache[r.tool] = _replayToolCache[r.tool] || []).push(r.output);
      }
    }
  }
  return _replayToolCache;
}

export function toolStub(name, args = [], opts = {}) {
  // full-replay parity with the python runtime: tool calls are mocked from
  // the trace and write NO record (the trace stays untouched during replay)
  if (process.env.NUDGE_REPLAY) {
    const recorded = _replayToolOutputs()[name] || [];
    const i = _replayToolIdx[name] || 0;
    _replayToolIdx[name] = i + 1;
    return i < recorded.length ? recorded[i] : [];
  }
  const record = { kind: "tool.call", tool: name, input: args, output: [] };
  if (opts.server) record.server = opts.server;
  _emitTrace(record);
  return [];
}


// ── agent state (design §7, v1.6 — parity with the python runtime) ────
// A Proxy over the state values: every field write persists the full
// state to .nudge/runs/<run_id>/checkpoint.json. Resume semantics mirror
// the python AgentState, INCLUDING the v1.6 divergence guard: the prefix
// replays from the defaults, and the reproduced values must equal the
// recorded checkpoint or the run aborts with ReplayMismatch.
export function agentState(agent, defaults) {
  const run = process.env.NUDGE_RUN_ID || `run-${process.pid}`;
  const dir = `.nudge/runs/${run}`;
  fs.mkdirSync(dir, { recursive: true });
  const ckptPath = `${dir}/checkpoint.json`;
  const values = { ...defaults };
  let writes = 0;
  let suppress = 0;
  let savedValues = null;
  const resuming = !!process.env.NUDGE_RESUME && fs.existsSync(ckptPath);
  if (resuming) {
    const saved = JSON.parse(fs.readFileSync(ckptPath, "utf8"));
    writes = saved.writes || 0;
    savedValues = saved.values || {};
    // replay from the DEFAULTS (not the checkpoint) so augmented writes
    // (+=/-=) re-accumulate instead of double-applying
    suppress = writes;
  }
  function checkpoint() {
    fs.writeFileSync(
      ckptPath,
      JSON.stringify({ agent, values, writes }, null, 2) + "\n",
    );
  }
  const state = new Proxy(
    {},
    {
      get: (_t, k) => values[k],
      set: (_t, k, v) => {
        if (suppress > 0) {
          // replayed-prefix write: apply (so += accumulates) but do not
          // checkpoint; the prefix end is verified against the recording
          values[k] = v;
          suppress--;
          if (suppress === 0 && savedValues !== null) {
            if (JSON.stringify(values) !== JSON.stringify(savedValues)) {
              throw new Error(
                `ReplayMismatch: resume divergence in agent '${agent}': the replayed state ` +
                  `${JSON.stringify(values)} does not match the recorded checkpoint ` +
                  `${JSON.stringify(savedValues)} — the program changed since the crash; start a new run`,
              );
            }
          }
          return true;
        }
        values[k] = v;
        writes++;
        checkpoint();
        return true;
      },
    },
  );
  fs.writeFileSync(`${dir}/program`, process.env.NUDGE_PROGRAM || process.argv[1] || "unknown");
  if (process.env.NUDGE_TRACE) {
    fs.writeFileSync(`${dir}/trace`, process.env.NUDGE_TRACE);
  }
  if (!resuming) {
    checkpoint();
  }
  return state;
}
