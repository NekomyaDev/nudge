// Tests for the TS runtime (node:test, run with: node --experimental-strip-types --test runtime/)
// Node >= 22.6 executes the plain-JS .ts module via --experimental-strip-types.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { validateOutput, llmCall, toolStub, forAll, decide } from "./nudge_runtime.ts";

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
  assert.deepEqual(validateOutput({ enum: ["a", "b"] }, "b"), []);
  assert.equal(validateOutput({ enum: ["a", "b"] }, "c").length, 1);
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
