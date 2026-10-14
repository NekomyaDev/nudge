#!/usr/bin/env node
/**
 * End-to-end verification of Nudge WASM live-tool host bridge.
 *
 * Runs a Nudge agent compiled to WebAssembly under Node.js WASI, provides the
 * `nudge_host_tool_call` import via `host.js`, dispatches a live tool call,
 * returns the result into WASM linear memory, and verifies that the agent
 * receives and outputs the result.
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { WASI } from "node:wasi";
import { createHostToolImports, defaultToolDispatcher } from "./host.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "../..");

function parseArgs() {
  const args = process.argv.slice(2);
  let wasmPath = path.resolve(REPO_ROOT, "target/wasm32-wasip1/release/nudge-replay-wasi.wasm");
  let bundlePath = path.resolve(REPO_ROOT, "target/test_tool");

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--wasm" && i + 1 < args.length) {
      wasmPath = path.resolve(process.cwd(), args[++i]);
    } else if (args[i] === "--bundle" && i + 1 < args.length) {
      bundlePath = path.resolve(process.cwd(), args[++i]);
    } else if (args[i] === "-h" || args[i] === "--help") {
      console.log("Usage: node e2e.mjs [--wasm <path>] [--bundle <path>]");
      process.exit(0);
    }
  }

  return { wasmPath, bundlePath };
}

async function main() {
  const { wasmPath, bundlePath } = parseArgs();

  console.log("==========================================================");
  console.log(" Nudge WASM Live Tool Host Bridge E2E Verification");
  console.log("==========================================================");
  console.log(` WASM Binary:   ${wasmPath}`);
  console.log(` Agent Bundle:  ${bundlePath}`);
  console.log("==========================================================");

  if (!fs.existsSync(wasmPath)) {
    console.error(`ERROR: WASM binary not found at ${wasmPath}`);
    process.exit(1);
  }
  if (!fs.existsSync(bundlePath)) {
    console.error(`ERROR: Agent bundle not found at ${bundlePath}`);
    process.exit(1);
  }

  // Use relative path for WASI preopen resolution
  const relativeBundle = path.relative(REPO_ROOT, bundlePath);

  const wasi = new WASI({
    version: "preview1",
    args: ["nudge-replay-wasi", relativeBundle, "--live"],
    env: {
      ...process.env,
      NUDGE_PROVIDER: "host",
    },
    preopens: {
      ".": REPO_ROOT,
    },
  });

  let wasmMemory;
  const toolCalls = [];

  function trackingDispatcher(server, tool, args) {
    console.log(`\n[JS Host Bridge] -> Incoming live tool call: ${server}::${tool}`);
    console.log(`[JS Host Bridge] -> Arguments: ${JSON.stringify(args)}`);
    toolCalls.push({ server, tool, args });

    const result = defaultToolDispatcher(server, tool, args);
    console.log(`[JS Host Bridge] <- Tool execution result: ${result}`);
    return result;
  }

  const hostImports = createHostToolImports(() => wasmMemory, trackingDispatcher);

  const wasmBytes = fs.readFileSync(wasmPath);
  const wasmModule = new WebAssembly.Module(wasmBytes);

  const importObject = {
    wasi_snapshot_preview1: wasi.wasiImport,
    ...hostImports,
  };

  const instance = new WebAssembly.Instance(wasmModule, importObject);
  wasmMemory = instance.exports.memory;

  console.log("\n[Execution] Starting Nudge WASM agent inside WASI runtime...");
  let capturedStdout = "";
  const origStdoutWrite = process.stdout.write;
  process.stdout.write = (chunk, ...args) => {
    capturedStdout += chunk.toString();
    return origStdoutWrite.call(process.stdout, chunk, ...args);
  };

  let exitCode;
  try {
    exitCode = wasi.start(instance);
  } finally {
    process.stdout.write = origStdoutWrite;
  }

  console.log(`[Execution] WASM process exited with code ${exitCode}`);

  console.log("\n==========================================================");
  console.log(" Assertions & Verifications");
  console.log("==========================================================");

  // Assertion 1: Tool call received by host
  if (toolCalls.length === 0) {
    console.error("FAIL: Host bridge did not receive any tool calls.");
    process.exit(1);
  }
  console.log(`✓ Host received ${toolCalls.length} tool call(s) over host bridge:`);
  for (const call of toolCalls) {
    console.log(`  - ${call.server}::${call.tool} (${JSON.stringify(call.args)})`);
  }

  // Assertion 2: Specific tool call verification
  const calcCall = toolCalls.find(
    (c) => c.server === "calculator" && c.tool === "add" && c.args.a === 17 && c.args.b === 25
  );
  if (!calcCall) {
    console.error("FAIL: Expected tool call 'calculator::add' with a=17, b=25 not found.");
    process.exit(1);
  }
  console.log("✓ Expected calculator::add call parameters verified");

  // Assertion 3: Program received and printed tool result
  if (!capturedStdout.includes("42")) {
    console.error(`FAIL: Expected output to contain '42', but got:\n${capturedStdout}`);
    process.exit(1);
  }
  console.log("✓ Agent printed live tool result (42) to stdout");

  // Assertion 4: Clean exit code
  if (exitCode !== 0) {
    console.error(`FAIL: Expected exit code 0, got ${exitCode}`);
    process.exit(1);
  }
  console.log("✓ Process exit code 0");

  console.log("\n==========================================================");
  console.log(" SUCCESS: Nudge WASM live-tool host bridge verified end-to-end!");
  console.log("==========================================================");
}

main().catch((err) => {
  console.error("Unhandled error in e2e test:", err);
  process.exit(1);
});
