#!/usr/bin/env bash
# scripts/wasm_host_e2e.sh - End-to-end verification of Nudge WASM live-tool host bridge.
#
# Builds the WebAssembly runtime binary with the host-tool bridge import,
# packages a sample Nudge program with a live tool call into a standalone bundle,
# and executes the Node.js WASI host harness to verify the full round-trip.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=========================================================="
echo " Nudge WASM Live Tool Host Bridge E2E Verification Suite"
echo "=========================================================="

# 1. Dependency checks
if ! command -v node >/dev/null 2>&1; then
    echo "ERROR: node is required to run WASI host harness but was not found in PATH" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo is required to build WASM targets but was not found in PATH" >&2
    exit 1
fi

NODE_VERSION="$(node -v)"
echo "Node.js runtime: $NODE_VERSION"

# 2. Build nudge-replay-wasi for wasm32-wasip1
echo "==> [1/3] Building nudge-replay-wasi for wasm32-wasip1..."
cargo build --release --target wasm32-wasip1 -p nudge-agent --bin nudge-replay-wasi

WASM_BIN="$REPO_ROOT/target/wasm32-wasip1/release/nudge-replay-wasi.wasm"
if [ ! -f "$WASM_BIN" ]; then
    echo "ERROR: Expected WASM binary not found at '$WASM_BIN'" >&2
    exit 1
fi
echo "WASM binary ready: $WASM_BIN ($(wc -c < "$WASM_BIN" | tr -d ' ') bytes)"

# 3. Package test Nudge program with live tool call
echo "==> [2/3] Packaging Nudge agent program with live tool call..."
TEST_PROG="$REPO_ROOT/examples/wasm-host/test_tool.ndg"
TEST_BUNDLE="$REPO_ROOT/target/wasm_host_test_agent"

if [ ! -f "$TEST_PROG" ]; then
    echo "ERROR: Test program '$TEST_PROG' does not exist" >&2
    exit 1
fi

bash "$REPO_ROOT/scripts/build_agent.sh" "$TEST_PROG" "$TEST_BUNDLE" --replay-only

if [ ! -f "$TEST_BUNDLE" ]; then
    echo "ERROR: Failed to create agent bundle at '$TEST_BUNDLE'" >&2
    exit 1
fi
echo "Agent bundle ready: $TEST_BUNDLE"

# 4. Run Node.js host harness
echo "==> [3/3] Running Node.js WASI host harness..."
node "$REPO_ROOT/examples/wasm-host/e2e.mjs" --wasm "$WASM_BIN" --bundle "$TEST_BUNDLE"

echo "=========================================================="
echo " ALL GATES PASSED: WASM live-tool host bridge verified!    "
echo "=========================================================="
