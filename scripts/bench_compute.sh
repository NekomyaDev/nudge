#!/usr/bin/env bash
# scripts/bench_compute.sh
# Benchmark runner for pure-compute CPU-bound Nudge workloads.
# Builds nudgec in release mode, executes examples/bench/compute.ndg for N iterations,
# verifies deterministic correctness, and measures wall-clock timing and call throughput.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

BENCH_FILE="examples/bench/compute.ndg"
EXPECTED_OUTPUT="17834"
TOTAL_USER_CALLS=81468
ITERATIONS="${BENCH_RUNS:-5}"

echo "=========================================================="
echo " Nudge CPU-Bound Compute Benchmark"
echo "=========================================================="
echo "Benchmark file: $BENCH_FILE"
echo "Expected output: $EXPECTED_OUTPUT"
echo "Iterations: $ITERATIONS"
echo ""

# 1. Build nudgec in release mode
echo "==> [1/3] Building nudgec in release mode..."
cargo build --release -q -p nudgec
NUDGEC="$REPO_ROOT/target/release/nudgec"

if [ ! -x "$NUDGEC" ]; then
    echo "error: nudgec binary not found at $NUDGEC" >&2
    exit 1
fi

# 2. Type-check benchmark
echo "==> [2/3] Type-checking $BENCH_FILE..."
$NUDGEC check "$BENCH_FILE"

# 3. Benchmark runs
echo "==> [3/3] Running $ITERATIONS iterations..."

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nudge_bench.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

RESULTS_FILE="$TMP_DIR/results.txt"
NULL_RESULTS_FILE="$TMP_DIR/null_results.txt"

# Run with standard trace recording
echo ""
echo "--- Standard Mode (default trace.jsonl sink) ---"
for i in $(seq 1 "$ITERATIONS"); do
    TRACE_PATH="$TMP_DIR/trace_$i.jsonl"
    START_NS=$(python3 -c 'import time; print(int(time.perf_counter_ns()))')
    OUTPUT=$(NUDGE_TRACE="$TRACE_PATH" "$NUDGEC" run "$BENCH_FILE")
    END_NS=$(python3 -c 'import time; print(int(time.perf_counter_ns()))')
    
    if [ "$OUTPUT" != "$EXPECTED_OUTPUT" ]; then
        echo "FAIL: Iteration $i output '$OUTPUT' != expected '$EXPECTED_OUTPUT'" >&2
        exit 1
    fi
    
    DURATION_MS=$(python3 -c "print(($END_NS - $START_NS) / 1_000_000)")
    echo "  Run $i: ${DURATION_MS} ms (output: $OUTPUT)"
    echo "$DURATION_MS" >> "$RESULTS_FILE"
done

# Run with /dev/null trace sink to isolate compute from disk write serialization
echo ""
echo "--- Null-Trace Mode (NUDGE_TRACE=/dev/null) ---"
for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c 'import time; print(int(time.perf_counter_ns()))')
    OUTPUT=$(NUDGE_TRACE="/dev/null" "$NUDGEC" run "$BENCH_FILE")
    END_NS=$(python3 -c 'import time; print(int(time.perf_counter_ns()))')
    
    if [ "$OUTPUT" != "$EXPECTED_OUTPUT" ]; then
        echo "FAIL: Iteration $i output '$OUTPUT' != expected '$EXPECTED_OUTPUT'" >&2
        exit 1
    fi
    
    DURATION_MS=$(python3 -c "print(($END_NS - $START_NS) / 1_000_000)")
    echo "  Run $i: ${DURATION_MS} ms (output: $OUTPUT)"
    echo "$DURATION_MS" >> "$NULL_RESULTS_FILE"
done

# Calculate stats
echo ""
echo "=========================================================="
echo " Summary Statistics"
echo "=========================================================="
python3 -c "
import statistics

def analyze(label, path, total_calls):
    with open(path) as f:
        vals = [float(x.strip()) for x in f if x.strip()]
    mean = statistics.mean(vals)
    median = statistics.median(vals)
    stdev = statistics.stdev(vals) if len(vals) > 1 else 0.0
    min_val = min(vals)
    max_val = max(vals)
    throughput = (total_calls / (mean / 1000.0))
    print(f'{label}:')
    print(f'  Runs:       {len(vals)}')
    print(f'  Mean:       {mean:.2f} ms ({mean/1000.0:.4f} s)')
    print(f'  Median:     {median:.2f} ms')
    print(f'  Min:        {min_val:.2f} ms')
    print(f'  Max:        {max_val:.2f} ms')
    print(f'  StdDev:     {stdev:.2f} ms')
    print(f'  Throughput: {throughput:,.0f} user fn calls/sec')
    print('')

analyze('Standard Mode (Disk Trace)', '$RESULTS_FILE', $TOTAL_USER_CALLS)
analyze('Null-Trace Mode (/dev/null)', '$NULL_RESULTS_FILE', $TOTAL_USER_CALLS)
"

echo "Verification: ALL $ITERATIONS iterations produced verified output $EXPECTED_OUTPUT."
echo "Benchmark completed successfully."
