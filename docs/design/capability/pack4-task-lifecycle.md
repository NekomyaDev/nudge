# Nudge Language Capability Request: Pack 4 (Task Lifecycle)

**Status:** Implemented (see git log)
**Implementation Commits:** `059c68e` (PACK 4 slice 1: task lifecycle with cooperative cancellation), `0f44e11` (PACK 4 slice 2: full async task runtime spawn/join)
**Author:** Pack 4 Coordinator
**Target Repository:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` at commit `e2cafc0`

## Overview and Classification: MEVCUT / KISMI / ONERI

| Feature | Classification | Source Reference / Status |
| :--- | :--- | :--- |
| SSE Parser and Chunking | MEVCUT | `crates/nudge-runtime/src/provider/stream.rs:11-182` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`). W3C SSE line/event parser and OpenAI chunk delta extractor. |
| Streaming Budget Wall | MEVCUT | `runtime/nudge_runtime/__init__.py:2044-2056` and `2084-2093` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`). Consumed tokens on stream failure are charged and traced. |
| Tool Grants Policy Engine | MEVCUT | `crates/nudge-runtime/src/grants.rs:1-275` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`). Fail-closed static policy from `NUDGE_TOOL_GRANTS`. |
| Concurrent Race (`par race`) | KISMI | In Python runtime (`runtime/nudge_runtime/__init__.py:2432-2471`), `ThreadPoolExecutor` returns first completed, but losing threads keep running in the background. In Rust interpreter (`crates/nudgec/src/eval/interpreter.rs:476-481`), `xs[0]` is evaluated sequentially. |
| Stream Cancellation | KISMI | In Python runtime (`runtime/nudge_runtime/__init__.py:2081-2083`), early schema violation breaks the stream via `_PrefixImpossible`, but external cooperative cancellation is absent. |
| Task Identifier (`TaskId`) | ONERI | Explicit first-class `Task[T]` handle with hierarchical identifier and parent-child relationship. |
| Explicit Task States | ONERI | Formal lifecycle state machine: `Pending`, `Running`, `Completed`, `Failed`, `Cancelled`. |
| Cooperative Cancellation Protocol | ONERI | Pre-effect checkpoints, SSE chunk checkpoints, and loop boundary polling. |
| Cancellation Propagation | ONERI | Structured tree propagation: canceling a task cancels subtasks and losing `par race` branches. |
| Budget and Grants on Cancel | ONERI | Immediate release of unspent reservations, trace accounting for pre-cancel tokens, and fail-closed prevention of post-cancel tool operations. |

---

## 1. Capability and Why It Blocks Local Apps

### Required Capability
Pack 4 introduces a formal Task Lifecycle subsystem into Nudge:
1. Explicit Task Handles (`Task[T]`) and Hierarchical Task IDs.
2. Definitive Lifecycle States (`Pending`, `Running`, `Completed`, `Failed`, `Cancelled`).
3. Cooperative Cancellation propagating across task trees, SSE streams, and `par race` branches.
4. Fail-closed tool invocation prevention upon cancellation.
5. Predictable budget reservation release and expenditure accounting upon cancellation.

### Why Missing Task Lifecycle Blocks Local Applications
Local AI desktop applications, interactive command-line agents, and edge deployments face severe operational failures without task lifecycle primitives:
1. Runaway Spend from Losing Race Arms: When racing models or search strategies with `par race`, the losing branches continue running in background threads. They consume tokens and spend against `NUDGE_BUDGET` long after the winner has delivered its answer.
2. Uncancellable UI Streaming: When an end-user presses "Stop" in a chat UI, `stream let` cannot abort the underlying network read. The process either hangs until TCP EOF or requires an ungraceful process kill.
3. Irreversible Side Effects: A cancelled or losing task arm cannot stop scheduled tool calls (`tool.call` or `computer.*`). File modifications, subprocess executions, or GUI clicks proceed uncontrolled, causing state corruption.
4. Missing Compositional Concurrency: Developers cannot monitor progress, query state, or coordinate nested worker tasks without first-class task abstractions.

---

## 2. Current State with Source and Commit Evidence

### A. SSE Parser and `stream let`

#### What Exists Today
- Low-Level SSE Parser:
  - `crates/nudge-runtime/src/provider/stream.rs:11-36` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) defines `SseEvent` and `parse_openai_chunk`.
  - `crates/nudge-runtime/src/provider/stream.rs:127-182` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) provides `SseReader<R: BufRead>`, wrapping any reader and parsing lines via `read_line`.
- Native Interpreter Stream Binding:
  - `crates/nudgec/src/eval/interpreter.rs:105-142` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) checks `stream: true` in `StmtKind::Let`. For `NUDGE_PROVIDER=fake` or replay, it evaluates synchronously. For live providers, it calls `eval_stream_llm`.
  - `crates/nudgec/src/eval/interpreter.rs:1568-1575` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) triggers `nudge_runtime::provider::http::post_chat_stream` and passes the resulting reader to `consume_sse_stream`.
  - `crates/nudgec/src/eval/interpreter.rs:2093-2118` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) consumes events in a synchronous loop: `while let Some(event) = sse_reader.read_event()?`.
- Python Runtime Streaming:
  - `runtime/nudge_runtime/__init__.py:2060-2094` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) loops through SSE chunks (`for chunk in stream:`). If an exception occurs, tokens consumed up to that point are recorded and charged to the site budget (lines 2084-2093).

#### What Is Missing for Cancellation
- `SseReader` has no cancellation token, interrupt flag, or non-blocking poll capability.
- `consume_sse_stream` and `consume_sse_stream_with` cannot be signalled to break early from an external task controller.
- `stream let` is entirely synchronous and cannot be aborted without tearing down the thread or process.

---

### B. Parallel Execution (`par race`)

#### What Exists Today
- Native Rust Interpreter:
  - `crates/nudgec/src/eval/interpreter.rs:476-481` (`commit 3a66a56b464a938c01d41113b48227b2b73bcff9`):
    ```rust
    ExprKind::ParRace(xs) => {
        if xs.is_empty() {
            return Ok(NudgeVal::Unit);
        }
        self.eval_expr(&xs[0], env)
    }
    ```
    The native interpreter evaluates only the first branch (`xs[0]`) synchronously. It does not spawn threads or race candidates.
- Python Runtime:
  - `runtime/nudge_runtime/__init__.py:2432-2471` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`):
    Uses `ThreadPoolExecutor`. When the first branch completes successfully, it calls `other.cancel()` (line 2466) and `pool.shutdown(wait=False, cancel_futures=True)` (line 2470).
    However, Python futures can only cancel tasks that have not started. In-flight worker threads continue running.
    Lines 2437-2441 explicitly document:
    "v1.3 fix: the pool no longer joins on exit... Losers now keep running in the background while the winner's result returns immediately."
- TypeScript Runtime:
  - `runtime/nudge_runtime.ts:392-395`:
    `thunks.map` evaluates every branch sequentially in order and returns `results[0]`. No concurrent race exists.
- Compiler Lint Warning:
  - `crates/nudgec/src/lint.rs:301-316` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`):
    Emits lint `W0007` when a race branch executes a computer action, noting that losing arms are not cancelled and continue spending budget and modifying machine state.

#### What Is Missing for Cancellation
- No cooperative cancellation token passed into parallel branches.
- No mechanism to stop active HTTP requests or tool executions when a competing arm wins.
- No synchronization barrier ensuring that losing branches have safely terminated before subsequent dependent steps run.

---

### C. Tool Grants (`NUDGE_TOOL_GRANTS`)

#### What Exists Today
- Implementation:
  - `crates/nudge-runtime/src/grants.rs:1-275` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) implements fail-closed JSON policy matching via `check_tool_allowed(tool, server)`.
  - `crates/nudgec/src/eval/interpreter.rs:831` (`commit 218464c8a2f83d352897d98db2efbb0d1fa31103`) evaluates grants during MCP tool invocation.

#### What Is Missing for Cancellation
- Grants are purely static configuration.
- There is no dynamic linkage between task lifecycle state and tool execution permission: a cancelled task retains valid grants and can continue invoking tools unless checked explicitly.

---

### D. Budget Enforcement (`NUDGE_BUDGET`)

#### What Exists Today
- Reservation and Charge:
  - `runtime/nudge_runtime/__init__.py:1437-1455` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`): `_budget_reserve` holds amount aside for in-flight requests under a lock.
  - `runtime/nudge_runtime/__init__.py:1457-1462` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`): `_budget_release` releases the held reservation.
  - `runtime/nudge_runtime/__init__.py:1537-1551` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`): `_budget_charge` increments total spent and trips `BudgetExceeded` if exceeded.
  - `runtime/nudge_runtime/__init__.py:2293-2305` (`commit cf66e973c36f5303cee615d05fb7c5d7704d41aa`): `_complete_reserved` wraps LLM completion with reservation acquire and release in a `finally` block.

#### What Is Missing for Cancellation
- No task-level budget binding.
- Losing branches in `par race` do not immediately release reservations when the race resolves, causing false `BudgetExceeded` trips on subsequent calls.

---

## 3. Proposed .ndg Source Syntax

The design keeps existing `stream let`, `tool`, and `par race` syntax intact while adding minimal task operations.

### A. Spawning and Task Handles
```nudge
// Spawn an asynchronous task returning a typed Task handle
let t: Task[string] = spawn generate_summary(doc)

// Non-blocking query of task state
let s: string = task.status(t) // "pending" | "running" | "completed" | "failed" | "cancelled"

// Blocking join to retrieve result
let result: string = task.join(t)
```

### B. Cooperative Cancellation
```nudge
// Cancel a running task handle
task.cancel(t)

// Checkpoint inside long-running functions
fn crawl_pages(urls: [string]) -> [string] uses Tool {
    let mut results = []
    for u in urls {
        if task.is_cancelled() {
            break
        }
        let content = fetch_url(u)
        results = results + [content]
    }
    results
}
```

### C. Automatic Cancellation in `par race`
The syntax for `par race` remains unchanged:
```nudge
let fastest = par race [
    search_fast(query),
    search_thorough(query)
]
```
Semantics update: When either branch yields a successful value, the runtime automatically invokes `task.cancel()` on the remaining branch handles.

---

## 4. Type and Effect Semantics

### A. Type Construction
- `Task[T]`: First-class generic type representing an ongoing computation producing `T`.
- Type compatibility follows standard covariance: `Task[T]` can be passed into functions accepting `Task[T]`.

### B. Effect Propagation
- Spawning a task captures the effects required by its body:
  - If `f()` uses `[LLM, Tool]`, `spawn f()` captures those effects.
  - The spawning function does not discharge those effects at the spawn site.
  - `task.join(t)` re-propagates any runtime errors produced during effect execution.
- Task control primitives (`task.cancel`, `task.status`, `task.is_cancelled`) do not require additional effect annotations. They operate as standard runtime primitives.

### C. Error Codes
- `error[E-task-0001]: TaskCancelled`
  Raised when attempting to join a cancelled task, or when an operation detects cancellation at a checkpoint.
- `error[E-task-0002]: TaskFailed`
  Raised when joining a task that panicked or failed with an unhandled exception.
- `error[E-task-0003]: InvalidTaskHandle`
  Raised when passing an uninitialized or corrupt task handle.

---

## 5. Local Execution Behavior

### A. Task Identification and Hierarchy
- Every task receives a monotonic, structured identifier: `root`, `root.1`, `root.1.1`.
- Each task maintains an atomic cancel token (`Arc<AtomicBool>`).
- Subtasks inherit their parent's token via a chained reference: if the parent token is cancelled, child tokens evaluate as cancelled.

### B. Explicit Task States
The task state transitions through a five-state machine:
1. `Pending`: Created and queued.
2. `Running`: Actively executing.
3. `Completed`: Finished with successful output value.
4. `Failed`: Terminated due to error.
5. `Cancelled`: Interrupted via cancellation token.

Transitions from `Completed`, `Failed`, or `Cancelled` are prohibited (terminal states).

### C. Cooperative Checkpoints
Cooperative cancellation is checked at three points:
1. Pre-Effect Barrier: Prior to initiating any LLM HTTP request (`post_chat_stream`), tool execution (`eval_mcp_tool`), or computer action (`eval_computer`), the runtime checks the active task cancel token. If set, the call aborts immediately with `E-task-0001`.
2. SSE Chunk Processing: Inside `consume_sse_stream_with`, the loop verifies the cancel token on every parsed chunk. If cancelled, it closes the underlying stream and exits.
3. Loop and Expression Boundaries: The interpreter inspects the cancel token on backward jumps and statement dispatch.

### D. Race Arm Cancellation
In `par race`:
1. Each arm is launched under a distinct child `TaskId`.
2. When the first successful result arrives, all sibling task handles are immediately marked `Cancelled`.
3. Sibling workers halt at their next checkpoint.

---

## 6. Security Boundary: Budget and Grants on Cancel

### A. Budget Behavior on Cancellation (`NUDGE_BUDGET`)
1. Immediate Reservation Release: When a task is cancelled, any active budget reservation (`_budget_reserve`) is freed immediately via `_budget_release`, preventing blocked capacity for subsequent tasks.
2. Accurate Pre-Cancel Charging: All tokens received before cancellation are calculated via `_call_cost`, charged to `_BUDGET_STATE["spent"]`, and recorded in the NTF trace with `outcome: "cancelled"`. Nudge guarantees: no phantom spend and no unrecorded tokens.
3. Post-Cancel Allocation Ban: Cancelled tasks are denied new budget reservations.

### B. Tool Grants on Cancellation (`NUDGE_TOOL_GRANTS`)
1. Fail-Closed Boundary: When a task enters `Cancelled` state, all tool permissions for that task and its descendants are revoked at the execution boundary.
2. Even if `NUDGE_TOOL_GRANTS` permits the tool, the runtime denies execution before invoking the adapter, raising `error[E-task-0001]: TaskCancelled`.
3. No pending tool operations may launch after a cancel signal is registered.

---

## 7. Minimal Acceptance Test

```nudge
// Acceptance Test: par race arm cancellation and tool refusal verification

tool mock_external_write(data: string) -> bool
tool mock_delay(ms: int) -> bool

fn fast_candidate() -> string {
    "done"
}

fn slow_candidate_with_tool() -> string uses Tool {
    // Simulated work
    let _ = mock_delay(100)
    // Security assertion: this tool call MUST NOT execute
    let ok = mock_external_write("leaked_mutation")
    "slow_done"
}

test "race winner stops loser before tool execution" {
    let result = par race [
        fast_candidate(),
        slow_candidate_with_tool()
    ]

    assert result == "done"

    // Verification:
    // 1. Losing arm must be stopped before calling mock_external_write.
    // 2. Trace must NOT contain any tool.call record for mock_external_write.
}

test "manual cancel mid-flow halts task and prevents tool call" {
    let t = spawn {
        let text = llm"""generate content""" with { model: "fake" }
        mock_external_write(text)
    }

    // Cancel before tool step completes
    task.cancel(t)

    assert task.status(t) == "cancelled"
}
```

---

## 8. Bytecode VM and Effect Delegation Note

### Confirmed Source Behaviors
1. Bytecode VM is Default Engine: `crates/nudgec/src/eval/interpreter.rs:1734-1736` (`commit 3a66a56b464a938c01d41113b48227b2b73bcff9`) sets VM execution as the default whenever `NUDGE_VM != "0"`.
2. Expression-Level Effect Delegation: `crates/nudgec/src/bytecode.rs:605-620` and `crates/nudgec/src/vm.rs:578-594` (`commit d48fcac96bc66146f72ad80cff0113f9665f11f6`) delegate effectful expressions (inline LLM and tool calls) to the interpreter via `Op::DelegateExpr`.
3. Compilation Rejection of Concurrency Primitives:
   - `crates/nudgec/src/bytecode.rs:258-260`: `if *stream { return Err("not in bytecode subset: stream let".to_string()); }`
   - `crates/nudgec/src/bytecode.rs:627`: `ExprKind::ParRace(_) => Err("not in bytecode subset: par race".to_string())`
   - Any program containing `stream let` or `par race` triggers a compile error that falls back to the full tree-walking interpreter in `run_program` (`crates/nudgec/src/eval/interpreter.rs:1758`).

### Unconfirmed Aspects from Source
1. Source contains no bytecode instructions for thread dispatch, fiber switching, or async continuations.
2. It cannot be confirmed from source whether the bytecode engine is intended to gain a native concurrent scheduler or if all concurrency will remain delegated to the runtime and interpreter.
3. Therefore, task lifecycle primitives should be anchored in `nudge-runtime` and the interpreter delegation layer, exposing an `Op::CheckCancel` instruction or delegation error boundary in the VM.
