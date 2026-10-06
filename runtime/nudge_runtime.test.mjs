// Tests for the TS runtime (node:test, run with: node --experimental-strip-types --test runtime/)
// Node >= 22.6 executes the plain-JS .ts module via --experimental-strip-types.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { validateOutput, llmCall, toolStub, forAll, decide, predictBatch, route } from "./nudge_runtime.ts";

const SCHEMA = {
  type: "object",
  required: ["answer", "confidence"],
  properties: {
    answer: { type: "string" },
    confidence: { type: "number" },
    tags: { type: "array", items: { type: "string" } },
  },
  additionalProperties: false,
};

function tracePath(records) {
  // temp trace file; the runtime reads NUDGE_REPLAY as a path
  const p = path.join(os.tmpdir(), `nudge-trace-${Date.now()}-${Math.random().toString(16).slice(2)}.jsonl`);
  fs.writeFileSync(p, records.map((r) => JSON.stringify(r)).join("\n"));
  return p;
}

test("validateOutput: accepts a schema-satisfying object", () => {
  const errs = validateOutput(SCHEMA, { answer: "42", confidence: 0.9, tags: ["a"] });
  assert.deepEqual(errs, []);
});

test("validateOutput: flags missing required fields", () => {
  const errs = validateOutput(SCHEMA, { answer: "42" });
  assert.equal(errs.length, 1);
  assert.match(errs[0], /missing required property 'confidence'/);
});

test("validateOutput: flags wrong types, including nested items", () => {
  const errs = validateOutput(SCHEMA, { answer: 42, confidence: 0.9, tags: ["a", 3] });
  assert.equal(errs.length, 2);
  assert.match(errs[0], /output\.answer: expected string/);
  assert.match(errs[1], /output\.tags\[1\]: expected string/);
});

test("validateOutput: flags unexpected properties when additionalProperties is false", () => {
  const errs = validateOutput(SCHEMA, { answer: "x", confidence: 0.1, extra: 1 });
  assert.equal(errs.length, 1);
  assert.match(errs[0], /unexpected property 'extra'/);
});

test("validateOutput: integer vs number and enum", () => {
  assert.deepEqual(validateOutput({ type: "integer" }, 3), []);
  assert.equal(validateOutput({ type: "integer" }, 3.5).length, 1);
  assert.deepEqual(validateOutput({ type: "number" }, 3), []);
  assert.deepEqual(validateOutput({ type: "number" }, 3.5), []);
  assert.deepEqual(validateOutput({ enum: ["a", "b"] }, "b"), []);
  assert.equal(validateOutput({ enum: ["a", "b"] }, "c").length, 1);
});

test("validateOutput: bounds, uri format, and additionalProperties schema", () => {
  assert.deepEqual(validateOutput({ type: "integer", minimum: 0, maximum: 10 }, 5), []);
  assert.equal(validateOutput({ type: "integer", minimum: 0, maximum: 10 }, -1).length, 1);
  assert.equal(validateOutput({ type: "integer", minimum: 0, maximum: 10 }, 11).length, 1);
  assert.deepEqual(validateOutput({ type: "string", format: "uri" }, "https://example.com/foo"), []);
  assert.equal(validateOutput({ type: "string", format: "uri" }, "not-a-uri").length, 1);
  const dictSchema = { type: "object", additionalProperties: { type: "number" } };
  assert.deepEqual(validateOutput(dictSchema, { a: 1, b: 2 }), []);
  assert.equal(validateOutput(dictSchema, { a: "wrong" }).length, 1);
});

test("replayed llm output violating the schema raises ReplayMismatch", () => {
  const tmp = tracePath([
    { kind: "llm.call", output: { answer: "ok", confidence: 0.9 } },
    { kind: "llm.call", output: { answer: 7, confidence: "high" } },
  ]);
  process.env.NUDGE_REPLAY = tmp;
  try {
    const out = llmCall({ prompt: "p", schema: SCHEMA });
    assert.deepEqual(out, { answer: "ok", confidence: 0.9 });
    assert.throws(
      () => llmCall({ prompt: "p", schema: SCHEMA }),
      /ReplayMismatch: recorded llm output violates the declared schema/
    );
  } finally {
    delete process.env.NUDGE_REPLAY;
  }
});

test("tool replay exhaustion raises ReplayMismatch (not a silent [])", () => {
  const tmp = tracePath([{ kind: "tool.call", tool: "find", input: [], output: [] }]);
  process.env.NUDGE_REPLAY = tmp;
  try {
    assert.doesNotThrow(() => toolStub("find", []));
    assert.throws(
      () => toolStub("find", []),
      /ReplayMismatch: program made more tool calls to 'find'/
    );
  } finally {
    delete process.env.NUDGE_REPLAY;
  }
});

// ── rt.forAll (property-based tests, design §6.4) ────────────────────
test("forAll passes a true property over int/str/injection cases", () => {
  forAll("int", [0, 10], (n) => {
    if (n < 0 || n > 10) throw new Error("out of range");
  }, "n");
  forAll("str", [8], (s) => {
    if (s.length > 8) throw new Error("too long");
  }, "s");
  forAll("injection", [], () => {}, "p");
  forAll("bool", [], () => {}, "b");
});

test("forAll shrinks a failing int to a minimal counterexample", () => {
  assert.throws(
    () => forAll("int", [0, 10], (n) => {
      if (n > 5) throw new Error("bad");
    }, "n"),
    /failed: n=6/,
  );
});

test("forAll case lists are deterministic (fixed seed)", () => {
  const seen = [];
  const probe = (n) => { seen.push(n); };
  forAll("int", [0, 5], probe, "n");
  const once = seen.join(",");
  seen.length = 0;
  forAll("int", [0, 5], probe, "n");
  assert.equal(seen.join(","), once);
});

// ── rt.decide (v1.4 "Decision") ──────────────────────────────────────
const DECIDE_QS = [
  { name: "dept", kind: "choice", prompt: "team?", options: ["billing", "technical", "security"] },
  { name: "risk", kind: "noul", prompt: "churn?" },
  { name: "urgency", kind: "score", prompt: "urgent?", levels: ["low", "soon", "critical"] },
];

test("fake decide returns typed answers, deterministically", () => {
  const a = decide(DECIDE_QS, "laptop broken", { model: "fake" });
  const b = decide(DECIDE_QS, "laptop broken", {});
  assert.equal(a.dept.winner, b.dept.winner);
  assert.ok(a.dept.p > 0 && a.dept.p <= 1);
  assert.equal(Object.keys(a.dept.distribution).length, 3);
  assert.ok(a.risk.p >= 0 && a.risk.p <= 1);
  assert.ok(a.urgency.score >= 0 && a.urgency.score <= 2);
});

test("fake decide distribution sums to 1 and winner is argmax", () => {
  const a = decide(DECIDE_QS, "another state", {});
  const sum = Object.values(a.dept.distribution).reduce((x, y) => x + y, 0);
  assert.ok(Math.abs(sum - 1) < 1e-9, `sum ${sum}`);
  assert.equal(a.dept.distribution[a.dept.winner], a.dept.p);
});

test("decide deadline miss annotates softly", () => {
  const a = decide(DECIDE_QS, "laptop broken", { deadline: -1 });
  assert.equal(a.dept.deadline_missed, true);
});

test("unconfigured provider raises with a clear message", () => {
  const save = process.env.NUDGE_DECISION_SERVERS;
  delete process.env.NUDGE_DECISION_SERVERS;
  try {
    assert.throws(() => decide(DECIDE_QS, "s", { model: "laya:multilingual" }),
      /not configured/);
  } finally {
    if (save !== undefined) process.env.NUDGE_DECISION_SERVERS = save;
  }
});

test("decide writes a decision.call record; replay consumes it; exhaustion raises", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nudge_decide_"));
  const trace = path.join(dir, "trace.jsonl");
  process.env.NUDGE_TRACE = trace;
  try {
    const qs = [{ name: "dept", kind: "choice", prompt: "team?", options: ["a", "b"] }];
    const winner = decide(qs, "state one", {}).dept.winner;
    const lines = fs.readFileSync(trace, "utf8").trim().split("\n").map(JSON.parse);
    const rec = lines.find((r) => r.kind === "decision.call");
    assert.ok(rec, "decision.call record written");
    assert.equal(rec.model, "fake");
    assert.ok(rec.questions.dept, "questions keyed by name");
    assert.equal(rec.answers.dept.winner, winner);
    assert.equal(lines.filter((r) => r.kind === "decision.call").length, 1);
    assert.ok(typeof rec.latency_ms === "number");
    assert.equal(rec.outcome, "ok");
    delete process.env.NUDGE_TRACE;

    // replay: consume the recorded answer
    process.env.NUDGE_REPLAY = trace;
    const replayed = decide(qs, "state one", {});
    assert.deepEqual(replayed, rec.answers);
    // exhaustion: a second decide without a second record raises
    assert.throws(() => decide(qs, "state two", {}), /more decide calls than the trace holds/);
  } finally {
    delete process.env.NUDGE_TRACE;
    delete process.env.NUDGE_REPLAY;
  }
});

// ── decision cache (NUDGE_DECISION_CACHE) ────────────────────────────
test("decision cache: miss then hit, no second server call, cache field written", async () => {
  const http = await import("node:http");
  const { promisify } = await import("node:util");
  let serverCalls = 0;
  const server = http.createServer((req, res) => {
    serverCalls += 1;
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify({
      answers: { dept: { type: "choice", choice: "a", probabilities: { a: 0.7, b: 0.3 } } },
    }));
  });
  await promisify(server.listen.bind(server))(0, "127.0.0.1");
  const port = server.address().port;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nudge_dcache_"));
  const trace = path.join(dir, "trace.jsonl");
  const cachePath = path.join(dir, "cache.json");
  const savedVars = ["NUDGE_DECISION_SERVERS", "NUDGE_DECISION_CACHE", "NUDGE_TRACE"];
  const saved = savedVars.map((k) => process.env[k]);
  process.env.NUDGE_DECISION_SERVERS = JSON.stringify({ laya: { base_url: `http://127.0.0.1:${port}` } });
  process.env.NUDGE_DECISION_CACHE = cachePath;
  process.env.NUDGE_TRACE = trace;
  try {
    const qs = [{ name: "dept", kind: "choice", prompt: "team?", options: ["a", "b"] }];
    const first = await decide(qs, "same state", { model: "laya:multilingual" });
    const second = await decide(qs, "same state", { model: "laya:multilingual" });
    assert.equal(serverCalls, 1, "second call served from cache");
    assert.equal(first.dept.winner, second.dept.winner);
    const recs = fs.readFileSync(trace, "utf8").trim().split("\n").map(JSON.parse)
      .filter((r) => r.kind === "decision.call");
    assert.equal(recs.length, 2);
    assert.equal(recs[0].cache, undefined);
    assert.equal(recs[1].cache, "hit");
    // a changed state must miss
    await decide(qs, "different state", { model: "laya:multilingual" });
    assert.equal(serverCalls, 2);
  } finally {
    server.close();
    savedVars.forEach((k, i) => {
      if (saved[i] === undefined) delete process.env[k];
      else process.env[k] = saved[i];
    });
  }
});

const mockValenScript = path.join(os.tmpdir(), "nudge_mock_valen.mjs");
fs.writeFileSync(
  mockValenScript,
  `import fs from "node:fs";
const args = process.argv.slice(2);
const dataPath = args[args.indexOf("--data") + 1];
const outPath = args[args.indexOf("--output") + 1];
if (process.env.MOCK_VALEN_MARKER) {
  fs.appendFileSync(process.env.MOCK_VALEN_MARKER, "call\\n");
}
const raw = fs.readFileSync(dataPath, "utf8");
const lines = raw.trim().split("\\n").filter(Boolean);
const outLines = lines.map((l) => {
  const rec = JSON.parse(l);
  const targets = {};
  for (const [name, q] of Object.entries(rec.request.questions || {})) {
    if (name === "dept") {
      targets[name] = { probabilities: { billing: 0.9, technical: 0.1 } };
    } else if (name === "churn") {
      targets[name] = { probabilities: { yes: 0.9, no: 0.1 } };
    } else if (name === "urgency") {
      targets[name] = { probabilities: { "0": 0.3, "1": 0.7 } };
    } else {
      targets[name] = { probabilities: { yes: 0.9, no: 0.1 } };
    }
  }
  return JSON.stringify({ group_id: rec.group_id, targets });
});
fs.writeFileSync(outPath, outLines.join("\\n") + "\\n");
`
);

test("valen transport: subprocess JSONL contract, typed answers, trace record", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nudge_valen_"));
  const trace = path.join(dir, "trace.jsonl");
  const savedVars = ["NUDGE_DECISION_SERVERS", "NUDGE_TRACE"];
  const saved = savedVars.map((k) => process.env[k]);
  process.env.NUDGE_DECISION_SERVERS = JSON.stringify({
    valen: { command: `node ${mockValenScript}` },
  });
  process.env.NUDGE_TRACE = trace;
  try {
    const qs = [
      { name: "dept", kind: "choice", prompt: "team?", options: ["billing", "technical"] },
      { name: "churn", kind: "noul", prompt: "churn?" },
      { name: "urgency", kind: "score", prompt: "urgent?", levels: ["low", "soon"] },
    ];
    const a = await decide(qs, "printer on fire", { model: "valen:preview" });
    assert.equal(a.dept.winner, "billing");
    assert.ok(Math.abs(a.dept.p - 0.9) < 1e-9);
    assert.equal(a.churn.p, 0.9);
    assert.ok(a.urgency.score > 0 && a.urgency.score < 1);
    const recs = fs.readFileSync(trace, "utf8").trim().split("\n").map(JSON.parse)
      .filter((r) => r.kind === "decision.call");
    assert.equal(recs.length, 1);
    assert.equal(recs[0].provider, "valen");
  } finally {
    savedVars.forEach((k, i) => {
      if (saved[i] === undefined) delete process.env[k];
      else process.env[k] = saved[i];
    });
  }
});

// ── predictBatch (multi-state decisions) ─────────────────────────────
test("predictBatch: one valen subprocess for N states, per-state records, order kept", async () => {
  const marker = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "nudge_vmark_")), "calls");
  const savedVars = ["NUDGE_DECISION_SERVERS", "NUDGE_TRACE", "NUDGE_DECISION_CACHE", "MOCK_VALEN_MARKER"];
  const saved = savedVars.map((k) => process.env[k]);
  process.env.NUDGE_DECISION_SERVERS = JSON.stringify({
    valen: { command: `node ${mockValenScript}` },
  });
  process.env.MOCK_VALEN_MARKER = marker;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nudge_batch_"));
  process.env.NUDGE_TRACE = path.join(dir, "trace.jsonl");
  try {
    const qs = [
      { name: "dept", kind: "choice", prompt: "team?", options: ["billing", "technical"] },
      { name: "churn", kind: "noul", prompt: "churn?" },
    ];
    const states = ["t0", "t1", "t2"];
    process.env.NUDGE_DECISION_CACHE = path.join(dir, "cache.json");
    const res = await predictBatch(qs, states, { model: "valen:preview" });
    assert.equal(res.length, 3);
    assert.deepEqual(res.map((r) => r.dept.winner), ["billing", "billing", "billing"]);
    assert.equal(fs.readFileSync(marker, "utf8").split("call").length - 1, 1, "one subprocess");
    const recs = fs.readFileSync(process.env.NUDGE_TRACE, "utf8").trim().split("\n").map(JSON.parse);
    assert.equal(recs.length, 3);
    assert.ok(recs.every((r) => r.batch && r.batch.size === 3));

    // cache absorbs a repeat; one new state = one more subprocess
    await predictBatch(qs, states, { model: "valen:preview" });
    assert.equal(fs.readFileSync(marker, "utf8").split("call").length - 1, 1);
    await predictBatch(qs, [...states, "t3"], { model: "valen:preview" });
    assert.equal(fs.readFileSync(marker, "utf8").split("call").length - 1, 2);

    // fake path: deterministic, no subprocess
    const fa = predictBatch(qs, states, {});
    assert.equal(fa.length, 3);
    assert.ok(fa[0].dept.winner);
  } finally {
    savedVars.forEach((k, i) => {
      if (saved[i] === undefined) delete process.env[k];
      else process.env[k] = saved[i];
    });
  }
});

test("route: picks arm and attaches route label to trace record", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nudge_route_"));
  const traceFile = path.join(dir, "trace.jsonl");
  const prevTrace = process.env.NUDGE_TRACE;
  process.env.NUDGE_TRACE = traceFile;
  try {
    const chosenModel = route(
      ["cheap", () => "m-cheap", () => true],
      ["strong", () => "m-strong", null]
    );
    assert.equal(chosenModel, "m-cheap");
    llmCall({ prompt: "hello", model: chosenModel });
    const recs = fs.readFileSync(traceFile, "utf8").trim().split("\n").map(JSON.parse);
    assert.equal(recs.length, 1);
    assert.equal(recs[0].model, "m-cheap");
    assert.equal(recs[0].route, "cheap");
  } finally {
    if (prevTrace === undefined) delete process.env.NUDGE_TRACE;
    else process.env.NUDGE_TRACE = prevTrace;
  }
});

// ── computer use (v1.5) ───────────────────────────────────────────────

import { computerObserve, computerClick, computerType, computerKey, computerSetValue, computerPerform, computerPaste, ComputerDenied } from "./nudge_runtime.ts";

function withEnv(env, fn) {
  const prev = {};
  for (const k of Object.keys(env)) { prev[k] = process.env[k]; process.env[k] = env[k]; }
  try { fn(); } finally {
    for (const k of Object.keys(env)) {
      if (prev[k] === undefined) delete process.env[k];
      else process.env[k] = prev[k];
    }
  }
}

test("computer: fake desktop observe/click/type/set_value with a trace", () => {
  withEnv({ NUDGE_TRACE: "/tmp/nudge-cu-test.jsonl" }, () => {
    fs.writeFileSync("/tmp/nudge-cu-test.jsonl", "");
    const obs = computerObserve("FakeApp", { allow: ["FakeApp"], screenshot: true });
    assert.equal(obs.state_id, "s-1");
    assert.equal(obs.elements.length, 4);
    assert.ok(obs.tree.includes("[1] button \"OK\""));
    assert.ok(obs.screenshot.startsWith("data:image/png;base64,"));
    assert.ok(obs.screenshot_hash.startsWith("sha256:"));
    const r = computerClick(1, { allow: ["FakeApp"] });
    assert.equal(r.ok, true);
    const r2 = computerType("hello", { allow: ["FakeApp"] });
    assert.equal(r2.ok, true);
    const r3 = computerSetValue(3, "x", { allow: ["FakeApp"] });
    assert.equal(r3.ok, true);
    const recs = fs.readFileSync("/tmp/nudge-cu-test.jsonl", "utf8").trim().split("\n").map(JSON.parse);
    assert.deepEqual(recs.map((r) => r.kind), ["computer.observe", "computer.act", "computer.act", "computer.act"]);
    assert.equal(recs[0].elements.length, 4); // elements land in the record for replay
  });
});

test("computer: allow scope refuses other apps (ComputerDenied)", () => {
  assert.throws(() => computerObserve("Terminal", { allow: ["FakeApp"] }), ComputerDenied);
});

test("computer: kill switch refuses everything", () => {
  withEnv({ NUDGE_COMPUTER_KILL: "1" }, () => {
    assert.throws(() => computerObserve("FakeApp", { allow: ["FakeApp"] }), ComputerDenied);
  });
});

test("computer: replay consumes recorded observations and never re-fires actions", () => {
  const obsRec = { v: 1, seq: 1, kind: "computer.observe", app: "Notes", state_id: "s-1",
    title: "Notes", element_count: 2, outcome: "ok", latency_ms: 4,
    elements: [{ index: 0, role: "window", title: "Notes" }, { index: 1, role: "button", title: "OK", pressable: true }] };
  const actRec = { v: 1, seq: 2, kind: "computer.act", action: "click", app: "Notes",
    target: { index: 1 }, outcome: "ok", latency_ms: 9, ok: true };
  const p = tracePath([obsRec, actRec]);
  withEnv({ NUDGE_REPLAY: p }, () => {
    const obs = computerObserve("Notes", { allow: ["Notes"] });
    assert.equal(obs.title, "Notes");
    assert.equal(obs.elements.length, 2);
    assert.ok(obs.tree.includes("OK")); // tree re-rendered from recorded elements
    const r = computerClick(1, { allow: ["Notes"] });
    assert.equal(r.ok, true); // the recorded result — nothing executed
  });
});

test("computer: drift mode re-observes live and reports the mechanical diff", () => {
  const recElements = [
    { index: 0, role: "window", title: "Notes" },
    { index: 1, role: "button", title: "OK", pressable: true },
    { index: 2, role: "textfield", title: "Body", editable: true },
  ];
  const obsRec = { v: 1, seq: 1, kind: "computer.observe", app: "Notes", state_id: "s-1",
    title: "Notes", element_count: 3, outcome: "ok", latency_ms: 4, elements: recElements,
    tree: recElements.map((e) => `[${e.index}] ${e.role} "${e.title}"`).join("\n"),
    screenshot_hash: "sha256:aaa" };
  const actRec = { v: 1, seq: 2, kind: "computer.act", action: "click", app: "Notes",
    target: { index: 1 }, outcome: "ok", latency_ms: 9, ok: true };
  const p = tracePath([obsRec, actRec]);
  withEnv({ NUDGE_REPLAY: p, NUDGE_COMPUTER_DRIFT: "1", NUDGE_TRACE: "/tmp/nudge-cu-drift.jsonl" }, () => {
    fs.writeFileSync("/tmp/nudge-cu-drift.jsonl", "");
    const live = computerObserve("Notes", { allow: ["Notes"] }); // live fake desktop ≠ recorded tree
    assert.equal(live.drift.changed, true);
    assert.ok(live.drift.summary.length > 0);
    const r = computerClick(1, { allow: ["Notes"] }); // dry-run
    assert.equal(r.ok, true);
    const recs = fs.readFileSync("/tmp/nudge-cu-drift.jsonl", "utf8").trim().split("\n").map(JSON.parse);
    assert.equal(recs[0].replay_check, true);
    assert.equal(recs[1].dry_run, true);
  });
});

test("computer: perform + paste with dispatch receipts and bounds", () => {
  withEnv({ NUDGE_TRACE: "/tmp/nudge-cu-h.jsonl" }, () => {
    fs.writeFileSync("/tmp/nudge-cu-h.jsonl", "");
    const obs = computerObserve("FakeApp", { allow: ["FakeApp"] });
    assert.ok(obs.elements.every((el) => Array.isArray(el.bounds)));
    const p = computerPerform(1, "press", { allow: ["FakeApp"] });
    assert.equal(p.ok, true);
    assert.equal(p.action_sent, true); // bridge knows the input was dispatched
    const w = computerPaste("hello", null, { allow: ["FakeApp"] });
    assert.equal(w.ok, true);
    const recs = fs.readFileSync("/tmp/nudge-cu-h.jsonl", "utf8").trim().split("\n").map(JSON.parse);
    assert.equal(recs[0].snapshot_mode, "full");
    assert.equal(recs[1].action, "perform");
    assert.equal(recs[2].action, "paste");
    assert.equal(recs[1].action_sent, true);
  });
});

test("computer: replay verifies the call signature (app, action, target)", () => {
  const obsRec = { v: 1, seq: 1, kind: "computer.observe", app: "Notes", state_id: "s-1",
    title: "Notes", element_count: 2, outcome: "ok", latency_ms: 4,
    elements: [{ index: 0, role: "window", title: "Notes" }, { index: 1, role: "button", title: "OK", pressable: true }] };
  const actRec = { v: 1, seq: 2, kind: "computer.act", action: "click", app: "Notes",
    target: { index: 1 }, outcome: "ok", latency_ms: 9, ok: true };

  // observing a DIFFERENT app than the trace recorded must diverge loudly
  const p1 = tracePath([obsRec]);
  assert.throws(() => withEnv({ NUDGE_REPLAY: p1 }, () => computerObserve("Other", {})),
    /ReplayMismatch.*app 'Notes', program observed 'Other'/);

  // acting with a different action name than the recorded one
  const p2 = tracePath([obsRec, actRec]);
  withEnv({ NUDGE_REPLAY: p2 }, () => {
    computerObserve("Notes", {});
    assert.throws(() => computerKey("Escape", {}),
      /ReplayMismatch.*trace action 'click', program called 'key'/);
  });

  // acting on a different target than the recorded one
  const p3 = tracePath([obsRec, actRec]);
  withEnv({ NUDGE_REPLAY: p3 }, () => {
    computerObserve("Notes", {});
    assert.throws(() => computerClick(2, {}),
      /ReplayMismatch.*trace target .*program target/);
  });
});

test("computer: screenshot replay fidelity via the content-addressed sidecar", () => {
  const p = "/tmp/nudge-cu-sidecar.jsonl";
  withEnv({ NUDGE_TRACE: p }, () => {
    fs.rmSync(p + ".assets", { recursive: true, force: true });
    const obs = computerObserve("FakeApp", { allow: ["FakeApp"], screenshot: true });
    assert.ok(obs.screenshot.startsWith("data:image/png;base64,"));
    const recs = fs.readFileSync(p, "utf8").trim().split("\n").map(JSON.parse);
    assert.ok(recs[0].screenshot_asset, "sidecar name in the record");
    const asset = fs.readFileSync(p + ".assets/" + recs[0].screenshot_asset, "utf8");
    assert.ok(asset.startsWith("data:image/png;base64,"));
  });
  // replay: the recorded observation restores its screenshot from the sidecar
  const recs = fs.readFileSync(p, "utf8").trim().split("\n").map(JSON.parse);
  withEnv({ NUDGE_REPLAY: p }, () => {
    const obs = computerObserve("FakeApp", {});
    assert.equal(obs.screenshot, fs.readFileSync(p + ".assets/" + recs[0].screenshot_asset, "utf8"),
      "replay sees the same pixels the record run saw");
  });
  fs.rmSync(p + ".assets", { recursive: true, force: true });
});
