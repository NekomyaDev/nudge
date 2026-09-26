// Tests for the TS runtime (node:test, run with: node --experimental-strip-types --test runtime/)
// Node >= 22.6 executes the plain-JS .ts module via --experimental-strip-types.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { validateOutput, llmCall, toolStub } from "./nudge_runtime.ts";

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
