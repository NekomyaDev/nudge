#!/usr/bin/env bash
# Offline Replay Regression Gate ($0 CI Story)
# Verifies that Nudge's record -> replay round-trip is deterministic,
# requires $0 tokens and zero network access, and fails loudly when the
# replay trace is empty or exhausted.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nudge_replay_gate.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo "=========================================================="
echo " Nudge Deterministic Offline Replay Regression Gate"
echo "=========================================================="

# 1. Build nudgec
echo "==> [1/5] Checking / building nudgec..."
if [ -z "${NUDGEC:-}" ]; then
    if [ ! -x "./target/debug/nudgec" ]; then
        echo "Building nudgec (cargo build -p nudgec)..."
        cargo build -p nudgec
    else
        echo "Found existing nudgec binary at ./target/debug/nudgec"
    fi
    NUDGEC="./target/debug/nudgec"
fi

if [ ! -x "$NUDGEC" ]; then
    echo "ERROR: nudgec binary not found or not executable at '$NUDGEC'" >&2
    exit 1
fi
echo "Using compiler: $NUDGEC"

EXAMPLE="examples/hello_llm.ndg"
if [ ! -f "$EXAMPLE" ]; then
    echo "ERROR: Target example '$EXAMPLE' does not exist!" >&2
    exit 1
fi

TRACE_FILE="$TMP_DIR/trace.jsonl"
RECORD_OUT="$TMP_DIR/record.out"
REPLAY_OUT="$TMP_DIR/replay.out"
EMPTY_TRACE="$TMP_DIR/empty_trace.jsonl"
NEG_OUT="$TMP_DIR/neg.out"
NEG_ERR="$TMP_DIR/neg.err"

# 2. RECORD
echo "==> [2/5] RECORD: Running $EXAMPLE with fake provider to record trace..."
env -u NUDGE_REPLAY NUDGE_PROVIDER=fake NUDGE_TRACE="$TRACE_FILE" "$NUDGEC" run "$EXAMPLE" > "$RECORD_OUT"

if [ ! -s "$TRACE_FILE" ]; then
    echo "ERROR: Trace file '$TRACE_FILE' was not written or is empty!" >&2
    exit 1
fi

TRACE_LINES=$(wc -l < "$TRACE_FILE" | tr -d ' ')
echo "Record succeeded: generated $TRACE_LINES trace record(s)."
echo "--- Recorded Output ---"
cat "$RECORD_OUT"
echo "-----------------------"

# 3. VALIDATE
echo "==> [3/5] VALIDATE: Checking trace against frozen v1 schema..."
"$NUDGEC" trace-check "$TRACE_FILE"
echo "Trace schema validation passed."

# 4. REPLAY
echo "==> [4/5] REPLAY: Running $EXAMPLE in replay mode (\$0 tokens, zero network)..."
env -u NUDGE_TRACE NUDGE_PROVIDER=fake NUDGE_REPLAY="$TRACE_FILE" "$NUDGEC" run "$EXAMPLE" > "$REPLAY_OUT"

echo "Replay succeeded. Verifying byte-identical determinism with diff..."
if diff -u "$RECORD_OUT" "$REPLAY_OUT"; then
    echo "Determinism verified: record and replay outputs are byte-identical!"
else
    echo "ERROR: Replay output diverges from record output!" >&2
    exit 1
fi

# 5. NEGATIVE
echo "==> [5/5] NEGATIVE: Verifying replay exhaustion fails loudly on empty trace..."
touch "$EMPTY_TRACE"

set +e
env -u NUDGE_TRACE NUDGE_PROVIDER=fake NUDGE_REPLAY="$EMPTY_TRACE" "$NUDGEC" run "$EXAMPLE" > "$NEG_OUT" 2> "$NEG_ERR"
NEG_EXIT=$?
set -e

if [ "$NEG_EXIT" -eq 0 ]; then
    echo "ERROR: Negative test unexpectedly succeeded (exit code 0) on empty trace!" >&2
    echo "Replay exhaustion must fail loudly with non-zero exit." >&2
    cat "$NEG_OUT"
    exit 1
fi

echo "Negative test passed: exited non-zero (exit code $NEG_EXIT) as expected."
echo "--- Negative Test Error Output ---"
cat "$NEG_ERR"
echo "----------------------------------"

echo "=========================================================="
echo " ALL GATES PASSED: Deterministic offline replay verified (\$0 tokens, zero network)! "
echo "=========================================================="
