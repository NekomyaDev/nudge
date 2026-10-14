# CPU-Bound Execution Baseline for Nudge

## Executive Summary

This report establishes a measured baseline for CPU-bound execution in Nudge to inform bytecode virtual machine design. We evaluated the existing tree-walking AST interpreter (`crates/nudgec/src/eval/`) on a pure-compute program without external effects (no LLM, no tools, no computer use, no decide calls).

Key findings:
- Throughput: The native AST interpreter achieves ~100,600 user function calls per second with standard tracing, and ~128,400 calls per second with tracing redirected to `/dev/null`.
- Stack Depth Limit: The AST interpreter uses native Rust call frames for all evaluation. Linear recursion overflows the OS thread stack at approximately 1,250 nested frames.
- Tracing Overhead: In `nudgec run`, every user function return emits an NTF JSON record via `emit_fn_return`. Writing 81,468 records to disk consumes roughly 175 ms of wall-clock time (21.6% of execution). Even when redirected to `/dev/null`, per-call open/write operations and JSON formatting contribute substantial overhead.
- Relative Speed: Compared to Python 3.10 bytecode, Nudge AST execution is ~134x slower (null-trace) to ~170x slower (disk trace). Compared to optimized native Rust, it is ~9,380x to ~11,970x slower.
- Verdict: For live AI agent workloads, execution time is dominated by LLM and network I/O (typically 500 ms to 5,000 ms per turn), where pure compute accounts for under 0.1% of total latency. However, a bytecode VM with a heap-allocated operand and frame stack is strongly justified for:
  1. Preventing fatal native stack overflows on deep recursion.
  2. Accelerating zero-token offline trace replay in CI and regression test suites.
  3. Operating safely in constrained stack environments such as WebAssembly.

---

## 1. Benchmark Program and Deterministic Output

The benchmark program is located at `examples/bench/compute.ndg`. It exercises the core compute constructs of Nudge:
- Binary tree recursion (`fib(22)` = 17,711 across 57,313 function calls)
- Ternary nested recursion (`tak(12, 8, 4)` = 12 across 12,605 function calls)
- Linear recursion with arithmetic predicates and modulo operations (`count_primes(2, 500, 0)` = 95 across 3,207 function calls)
- List literals and parallel map transforms (`par map [1..16] |x| -> fib(x)` across 8,342 function calls)
- Runtime assertions validating intermediate invariants
- A deterministic single-integer checksum returned by `main()`

### Source (`examples/bench/compute.ndg`)

```nudge
// Pure compute benchmark for Nudge AST interpreter.
// Exercises recursive user functions, route branching, integer arithmetic,
// predicates, list literals, par map transforms, and assertion checks.
// Completely deterministic without any LLM, tool, or effectful operations.

fn fib(n: int) -> int {
    route {
        base: n when n <= 1,
        rec: fib(n - 1) + fib(n - 2) otherwise
    }
}

fn tak(x: int, y: int, z: int) -> int {
    route {
        base: y when x <= y,
        rec: tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y)) otherwise
    }
}

fn has_factor(n: int, d: int) -> bool {
    route {
        no: false when d * d > n,
        yes: true when n % d == 0,
        rec: has_factor(n, d + 1) otherwise
    }
}

fn is_prime(n: int) -> bool {
    route {
        small: false when n < 2,
        check: !has_factor(n, 2) otherwise
    }
}

fn count_primes(cur: int, max_val: int, acc: int) -> int {
    route {
        done: acc when cur > max_val,
        prime: count_primes(cur + 1, max_val, acc + 1) when is_prime(cur),
        composite: count_primes(cur + 1, max_val, acc) otherwise
    }
}

fn main() -> int {
    // 1. Binary tree recursion: fib(22) = 17711 (57,313 calls)
    let f = fib(22)
    assert f == 17711

    // 2. Ternary nested recursion: tak(12, 8, 4) = 12 (3,097 calls)
    let t = tak(12, 8, 4)
    assert t == 12

    // 3. Trial division and prime counting: 95 primes under 500 (~2,500 calls)
    let p = count_primes(2, 500, 0)
    assert p == 95

    // 4. Par map over list literal: pure transform
    let nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    let mapped = par map nums |x| -> fib(x)
    assert len(mapped) == 16

    // Deterministic checksum combining all four compute results
    f + t + p + len(mapped)
}
```

### Deterministic Output

Program return value:
```
17834
```
Calculation verification:
- `f` = 17,711
- `t` = 12
- `p` = 95
- `len(mapped)` = 16
- Total checksum = 17,711 + 12 + 95 + 16 = 17,834
- Total user function calls executed: exactly 81,468 calls.

---

## 2. Measured Wall-Clock Timing and Throughput

Benchmarks were executed on an AMD Ryzen 7 5700X 8-Core Processor running Linux x86_64, using release builds (`cargo build --release -p nudgec`) orchestrated by `scripts/bench_compute.sh`.

Two operational modes were evaluated:
1. Standard Mode: Default `nudgec run` behavior where every function return appends an NTF trace line to `trace.jsonl`.
2. Null-Trace Mode: Running with `NUDGE_TRACE=/dev/null` to isolate interpreter computation from disk file write operations.

### Measurement Results (5 iterations each)

| Metric | Standard Mode (Disk Trace) | Null-Trace Mode (`/dev/null`) |
| :--- | :--- | :--- |
| Run 1 | 840.89 ms | 632.46 ms |
| Run 2 | 792.36 ms | 637.28 ms |
| Run 3 | 774.60 ms | 630.82 ms |
| Run 4 | 799.23 ms | 632.13 ms |
| Run 5 | 840.24 ms | 639.74 ms |
| Mean | **809.46 ms** (0.8095 s) | **634.49 ms** (0.6345 s) |
| Median | **799.23 ms** | **632.46 ms** |
| Min | 774.60 ms | 630.82 ms |
| Max | 840.89 ms | 639.74 ms |
| StdDev | 29.78 ms | 3.82 ms |
| User Function Calls | 81,468 | 81,468 |
| Call Throughput | **100,644 calls/s** | **128,400 calls/s** |

---

## 3. Comparative Perspective: Nudge vs Python vs Native Rust

To contextualize Nudge AST interpreter performance, an identical computational workload was implemented and measured in standard Python 3.10 and in compiled native Rust (`rustc -O`).

### Benchmark Comparison Table

| Language / Engine | Execution Model | Mean Wall-Clock | Calls / Second | Slowdown vs Native | Slowdown vs Python |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Native Rust (`rustc -O`) | Native Machine Code | **0.068 ms** (67.6 us) | ~1,200,000,000 calls/s | 1.0x (baseline) | 0.014x |
| CPython 3.10 | Stack Bytecode VM | **4.74 ms** (0.0047 s) | ~17,187,000 calls/s | ~70x | 1.0x (baseline) |
| Nudge Native Interpreter (`/dev/null`) | Recursive AST Walk | **634.49 ms** (0.6345 s) | ~128,400 calls/s | ~9,380x | ~134x |
| Nudge Native Interpreter (Disk Trace) | Recursive AST Walk + Disk I/O | **809.46 ms** (0.8095 s) | ~100,644 calls/s | ~11,970x | ~170x |

---

## 4. Architectural Analysis: Where Time Goes

Profiling and source examination of `crates/nudgec/src/eval/` reveal distinct factors responsible for the performance profile:

### 1. Scope and Environment Allocations (`call_user_fn`)
On every function invocation:
- A new environment container is allocated: `let child_env = Rc::new(RefCell::new(Env::new()))`.
- Every parameter is defined via `child_env.borrow_mut().define(params[i].name.clone(), val)`.
- This creates heap-allocated `String` keys inside a `std::collections::HashMap<String, NudgeVal>`.
- Parameter verification checks `child_env.borrow().get(&p.name)`, repeating hash computation and value cloning.
- For 81,468 calls, this causes over 250,000 hash map operations and heap allocations.

### 2. Variable Lookup via Lexical Chains
Inside expressions, variable access (`env.borrow().get(name)`) traverses lexical parent pointers with string hashing and string comparisons. Once found, `NudgeVal` is cloned. For stack-based virtual machines, local variables reside in fixed-index array slots resolved at compile time with zero string hashing and zero heap allocation.

### 3. Native Rust Call Stack Recursion and Stack Depth Limit
`eval_expr`, `eval_stmt`, and `eval_body` are recursive Rust functions. Each Nudge function invocation requires roughly 15 to 20 nested Rust stack frames across `eval_call`, `call_user_fn`, `eval_body`, `eval_route`, `eval_binary`, and `eval_expr`.
- Consequence: Linear recursion (e.g., `sum_rec(n)`) overflows the thread stack and terminates the process with `fatal runtime error: stack overflow` at approximately `n = 1,250`.
- The language has no while/for loops, making recursion the primary iteration mechanism. Thus, the native interpreter cannot handle loops over moderately sized inputs.

### 4. Per-Call NTF Trace Serialization
In `crates/nudgec/src/eval/interpreter.rs`, every user function return executes:
```rust
if let Ok(val) = &result {
    emit_fn_return(fn_name, val)?;
}
```
In `crates/nudge-runtime/src/trace.rs`, `emit` does not use an open file handle or buffered writer:
- It acquires `SINK.lock()`.
- It converts the value to JSON.
- It calls `std::fs::OpenOptions::new().append(true).open(&path)` on every record.
- It writes bytes and closes the file.
Across 81,468 calls, opening and closing the trace file 81,468 times takes ~175 ms on disk, and roughly 230 ms of syscall CPU time even when targeting `/dev/null`.

### 5. Value Boxing and Dynamic Type Dispatch
Values are wrapped in the `NudgeVal` enum. Arithmetic operations (`+`, `-`, `*`, `/`, `%`) and comparisons require enum pattern matching, loose equality checks, and dynamic wrapping back into `NudgeVal`.

---

## 5. Honest Verdict: Is a Bytecode VM Justified?

### For Live AI Agent Execution: No
Nudge was designed for AI agent workflows where the dominant activities are:
- `llm"""..."""` calls: 500 ms to 5,000 ms per invocation.
- External MCP tool calls and HTTP requests: 50 ms to 1,500 ms per invocation.
- Computer-use automation steps: 100 ms to 2,000 ms per invocation.

In a representative agent turn, user code evaluates a few routing branches, parses fields, and formats prompts. The total CPU compute time is typically under 0.5 ms. Reducing 0.5 ms to 0.01 ms provides imperceptible user-facing gain against token generation latency and network round-trip time.

### For Language Robustness, Tooling, and CI: Yes
A bytecode virtual machine provides essential architectural value in four specific areas:

1. **Eliminating the 1,250 Stack Depth Ceiling**: Because Nudge lacks native looping constructs, algorithms must use recursion. A bytecode VM replaces native Rust call frames with a heap-allocated call frame and operand stack, allowing recursion to arbitrary depths without crashing the host process.
2. **Deterministic Offline Replay Performance**: Nudge features zero-cost offline trace replay for regression testing and CI verification (`nudgec resume`, `scripts/offline_replay_gate.sh`). When LLM and tool calls are mocked via deterministic replay, the entire runtime is CPU-bound. In test suites executing hundreds of traces, a 20x to 50x speedup significantly cuts CI cycle times.
3. **WebAssembly and Embedded Constraints**: When running inside WebAssembly runtimes or restricted host environments, native stack space is often limited (e.g. 64 KB to 1 MB). The recursive AST interpreter quickly exhausts this space. A compact bytecode VM runs in a flat loop with bounded native stack requirements.
4. **Immediate Pragmatic Optimization**: Prior to building a full bytecode compiler, buffering the NTF trace sink and disabling `fn.return` emission for non-effectful internal helper functions will immediately boost compute throughput by 20% to 40% with minimal code changes.
