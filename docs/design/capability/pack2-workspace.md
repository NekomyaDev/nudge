# NUDGE LANGUAGE CAPABILITY REQUEST
**PACK 2: Safe Workspace and Persistence**
**Status:** Implemented (see git log)
**Implementation Commits:** `0668add` (PACK 2 slice 1: scoped, safe workspace.* filesystem containment), `b2d5f16` (PACK 2 slice 2: store.* typed persistent key-value app-data store)
**Author:** NekomyaDev
**Target Repo:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` (HEAD `e2cafc0`)

---

## 1. Capability Overview and Motivation

### The Capability
Pack 2 introduces a sandboxed, capability-gated filesystem abstraction (`workspace.*`) and a structured persistent storage interface (`store.*`) to Nudge. This contract guarantees:
1. Authority-scoped filesystem operations confined to an explicit workspace root.
2. Protection against path traversal (`../`) and external symlink escapes.
3. Separation of destructive deletion authority from standard read/write operations.
4. Crash-consistent atomic writes using temporary staging, filesystem synchronization (fsync), and atomic rename.
5. A strongly typed application data store surviving process termination and system reboots.

### Why the Absence of This Contract Blocks Local Applications
Local agentic applications (such as workspace code assistants, refactoring engines, batch dataset organizers, and multi-turn stateful CLI agents) must interact with real files and maintain local state across user commands. Today, building local applications on Nudge is blocked by four critical limitations:

1. **Unbounded Filesystem Exposure:** Existing runtime IO functions bypass any directory boundary. If exposed to agentic execution, a compromised or halluncinated tool invocation can read or mutate arbitrary host files (`/etc/passwd`, `~/.ssh`, source trees outside the project).
2. **Data Corruption and In-Place Truncation:** Current write operations truncate files directly in place. An interruption, crash, or power failure during execution produces torn or zero-byte files, permanently corrupting project assets.
3. **Over-Privileged Deletion:** Deletion is grouped with general write capabilities. In the current implementation, `io.delete` recursively removes entire directory trees (`rm -rf`) with zero access checks. A routine granted write permission can inadvertently destroy an entire workspace.
4. **No Clean Application Persistence:** The existing agent checkpointing mechanism is tied directly to trace replay (`nudge resume`) for single-run crash recovery. It does not provide an addressable key-value or document store that persists across normal program runs. Applications are forced to resort to unmanaged, unsafe raw files.

---

## 2. Current State with Source and Commit Evidence

### [MEVCUT] What Exists in Source Today
- **Runtime IO Library (`crates/nudge-runtime/src/stdlib/io.rs:10-29, 32-184`):**
  - Introduced in commit `f452559` ("feat: add compiler, runtime and VS Code extension sources"), updated in `956e056`, `b3eb8a4`, and `fc14ea1`.
  - Implements basic operations: `io.read`, `io.write`, `io.append`, `io.exists`, `io.delete`, `io.list_dir`, `io.print`, `io.println`, `io.read_line`, `io.flush`.
  - Operates directly against host OS calls without containment:
    - `io.read` calls `std::fs::read_to_string(&path)` (`io.rs:42`).
    - `io.write` calls `std::fs::write(&path, &content)` (`io.rs:60`).
    - `io.append` calls `std::fs::OpenOptions::new().create(true).append(true).open(&path)` (`io.rs:79-87`).
    - `io.delete` calls `std::fs::remove_dir_all(&path)` for directories and `std::fs::remove_file(&path)` for files (`io.rs:109-113`).
    - `io.list_dir` calls `std::fs::read_dir(&path)` (`io.rs:128`).
- **Tool Capability Grants (`crates/nudge-runtime/src/grants.rs:8-37, 163-239`):**
  - Added in commit `218464c` ("feat(native): tool.call replay, NUDGE_TOOL_GRANTS, and stream let").
  - Enforces fail-closed evaluation of `NUDGE_TOOL_GRANTS` for external tool calls (`tool <name> on server <server>`).
  - Evaluated at the runtime boundary in `crates/nudgec/src/eval/interpreter.rs:848` inside `eval_mcp_tool`.
- **NTF v1 Trace Sink (`crates/nudge-runtime/src/trace.rs:343-367, 482-512`):**
  - Added in commit `a7d6a1d`, with buffered writes in commit `f2ad291`.
  - Emits frozen NTF v1 JSONL records (`trace.jsonl` or `NUDGE_TRACE`) capturing `llm.call`, `tool.call`, `decision.call`, and `computer.*` steps.
- **IO Effect Registration (`crates/nudgec/src/check.rs:214`):**
  - Registered in `KNOWN_EFFECTS = ["LLM", "Tool", "IO", "Decision", "Computer"]`.

### [KISMI] Partial Implementations and Critical Deficiencies
- **Disconnection of `.ndg` from `nudge-runtime::stdlib::io`:**
  - In commit `7a61aa7` ("feat(native): Phase 1 AST tree-walking interpreter for real Nudge syntax"), `crates/nudgec/src/eval/interpreter.rs` introduced an internal `call_stdlib` function (`interpreter.rs:891-1153`).
  - `interpreter.rs:608-619` dispatches identifiers and dotted names starting with `io.` to `self.call_stdlib(name, &evaluated_args)`.
  - However, `call_stdlib` (`interpreter.rs:893-910`) only implements `"io.print"` and `"io.println"`.
  - Any call to `io.read`, `io.write`, `io.append`, `io.exists`, `io.delete`, or `io.list_dir` reaches line 1152: `other => Err(format!("unknown standard library function '{other}'"))`.
  - **Verdict:** A `.ndg` program cannot execute any file IO function in `nudge-runtime::stdlib::io` today. The runtime crashes with an unknown standard library function error.
- **Static Checker Blind Spot:**
  - In `crates/nudgec/src/check.rs:317-334`, the `IO` effect is inferred ONLY for calls to `"replay"` or `"python"`.
  - In `crates/nudgec/src/check.rs:907, 1029`, calls like `io.read(...)` are parsed as `ExprKind::Field` (object `io`, field `read`). `check_expr` only inspects identifier calls and skips non-identifier callees, returning `Ty::Unknown` without emitting `E0101` and without inferring the `IO` effect.
- **Bytecode VM Incompatibility:**
  - In `crates/nudgec/src/bytecode.rs:732-750`, `check_expr_effects` treats `io.read(...)` as a pure call because its prefix is not `"computer"` and it is not in `tool_names`.
  - During compilation (`crates/nudgec/src/bytecode.rs:608-612`), `compile_expr` encounters `io.read` and fails with `not in bytecode subset: unknown function or effect 'io.read'`.
  - `run_program` (`crates/nudgec/src/eval/interpreter.rs:1736`) catches this error and falls back to the tree-walking interpreter, which then fails at runtime inside `call_stdlib`.
- **Tool Grants Do Not Cover Filesystem:**
  - `crates/nudge-runtime/src/grants.rs` only inspects MCP/external tool names (`server/tool`, `server/*`, `tool`, `*`). It has no capability to restrict file paths, directory boundaries, or filesystem access modes.
- **Checkpoints Are Not an App-Data Store:**
  - In the native interpreter (`crates/nudgec/src/eval/env.rs:48-94`, commit `7a61aa7`), `AgentStore` is strictly an in-memory `HashMap<String, HashMap<String, NudgeVal>>`. It performs no filesystem writes.
  - In Python and TypeScript codegen (`crates/nudgec/src/codegen.rs:188`, `crates/nudgec/src/codegen_ts.rs:198`, `runtime/nudge_runtime/__init__.py:61-118`), `AgentState` writes `.nudge/runs/<run_id>/checkpoint.json`. As specified in `docs/design.md:143-162` (§7), this is strictly an internal crash-recovery checkpoint tied to trace replay (`nudge resume <run_id>`). It cannot be queried by key, is scoped to an ephemeral execution PID, does not validate schemas on read, and cannot serve as general application persistence.
- **Unsafe Host Operations in Runtime Library:**
  - `crates/nudge-runtime/src/stdlib/io.rs:109-113` executes `fs::remove_dir_all(&path)` when `path` is a directory. Any script with access to `io.delete` can recursively delete arbitrary host directories without validation.
  - `crates/nudge-runtime/src/stdlib/io.rs:60` performs non-atomic writes via direct truncation, risking torn files on abort.
  - There is no path normalization, no canonicalization check, and no protection against symlink escapes.

---

## 3. [ÖNERİ] Proposed `.ndg` Source Syntax

We propose introducing two distinct, sandboxed modules: `workspace` (scoped filesystem) and `store` (persistent typed store).

### Workspace Filesystem Operations
```nudge
// Read and write within the authorized workspace
fn process_code(filename: string) -> string uses [IO, Workspace] {
    if workspace.exists(filename) {
        let original = workspace.read(filename)
        let transformed = "// Header\n" + original
        workspace.write(filename, transformed)
        transformed
    } else {
        "File not found"
    }
}

// Separate destructive deletion permission
fn purge_temporary(dir_name: string) -> bool uses [IO, Workspace, Workspace.Delete] {
    if workspace.exists(dir_name) {
        workspace.delete(dir_name, recursive: true)
        true
    } else {
        false
    }
}
```

### Persistent App-Data Store Operations
```nudge
// Typed schema surviving process termination and machine restarts
record UserPref {
    theme: string,
    history_limit: int,
}

fn update_preferences(theme: string) -> UserPref uses Store {
    let current: UserPref = store.get("user_pref", default: UserPref {
        theme: "system",
        history_limit: 50,
    })
    let updated = UserPref {
        theme: theme,
        history_limit: current.history_limit,
    }
    store.set("user_pref", updated)
    updated
}
```

### Surface API Signatures
- `workspace.read(path: string) -> string`
- `workspace.write(path: string, content: string) -> unit`
- `workspace.append(path: string, content: string) -> unit`
- `workspace.exists(path: string) -> bool`
- `workspace.list(dir: string = ".") -> [string]`
- `workspace.delete(path: string, recursive: bool = false) -> unit`
- `store.get(key: string, default: T) -> T`
- `store.set(key: string, val: T) -> unit`
- `store.has(key: string) -> bool`
- `store.delete(key: string) -> bool`
- `store.keys(prefix: string = "") -> [string]`

---

## 4. Type & Effect Semantics

### Effects Architecture
1. **`Workspace`:** Required for non-destructive filesystem operations (`read`, `write`, `append`, `exists`, `list`). Sub-effect of `IO`.
2. **`Workspace.Delete`:** Required specifically for `workspace.delete`. Destructive actions cannot be hidden under general write permission.
3. **`Store`:** Required for all persistent application store operations (`get`, `set`, `has`, `delete`, `keys`).

### Effect Verification (`crates/nudgec/src/check.rs`)
- Add `"Workspace"`, `"Workspace.Delete"`, and `"Store"` to `KNOWN_EFFECTS` (`check.rs:214`).
- Update `direct_effects` (`check.rs:288-375`) to inspect `ExprKind::Field` and `ExprKind::Call`:
  - Callee `workspace.delete` -> infers `Workspace` and `Workspace.Delete`.
  - Callee `workspace.*` -> infers `Workspace` (and transitively `IO`).
  - Callee `store.*` -> infers `Store`.
- Any function invoking these builtins without declaring them in its `uses` clause triggers diagnostic `E0301` ("effect used with no `uses` clause") or `E0302` ("`uses` clause too narrow").

### Type Inference and Generic Binding
- `workspace.read` returns `string`.
- `workspace.write` and `workspace.append` return `unit`.
- `workspace.exists` returns `bool`.
- `workspace.list` returns `[string]`.
- `store.get(key, default: T)` infers return type `T` from the default value argument.
- `store.set(key, val: T)` requires `val` to be a valid serializable Nudge value (primitive, list, or record). Functions, agents, or opaque handles are rejected at compile time with `E0201`.

### Diagnostics and Error Tagging
- **`E0910` (WorkspacePathEscape):** Attempted path traversal outside the authorized workspace boundary via `../` or symlinks.
- **`E0911` (WorkspaceDenied):** Workspace path or operation refused by execution policy (`NUDGE_WORKSPACE_GRANTS`).
- **`E0912` (DeleteDenied):** `workspace.delete` invoked without `Workspace.Delete` capability or effect.
- **`E0913` (StoreSchemaMismatch):** Data loaded from store fails schema validation against the requested type.

---

## 5. Local Execution Behavior

### Scoped Workspace Directory Authority
- The root of the workspace authority is defined by `--workspace <dir>` CLI flag or `NUDGE_WORKSPACE_DIR` environment variable (defaults to the current working directory).
- All relative paths are evaluated strictly against this workspace root.

### Symlink and Path-Escape Protection
Before any physical filesystem operation:
1. Lexically normalize the input path, stripping redundant `.` and evaluating relative `..` segments.
2. Canonicalize both the workspace root and the target path via `std::fs::canonicalize`.
3. If the target does not exist yet (e.g. for new file writes), canonicalize the nearest existing parent directory.
4. Verify containment: `canonical_target.starts_with(&canonical_workspace_root)`.
5. If the target is a symlink, verify that the link target resolves strictly inside the workspace boundary. Any external target aborts immediately with `E0910`.

### Separate Delete Authority
- Calling `workspace.delete` invokes `std::fs::remove_file` for single files.
- Directory deletion requires explicit argument `recursive: true`. If `recursive` is false and the target is a directory, the call fails.
- Deleting the workspace root itself is explicitly forbidden and fails closed.

### Atomic Writes (Crash Consistency)
To prevent corrupted or half-written files:
1. Staging: Write payload to a hidden sibling file in the same directory: `.<filename>.tmp.<pid>.<timestamp>`.
2. Sync: Call `std::io::Write::flush()` followed by `std::fs::File::sync_all()` to commit bytes to physical media.
3. Atomic Commit: Perform atomic filesystem replacement via `std::fs::rename(staging_path, target_path)`.
4. On any error or process termination before the rename completes, the target file remains in its prior state; the temporary staging file is cleaned up on subsequent boot.

### Typed App-Data Store
- Located at `.nudge/store.db` within the workspace (or `NUDGE_STORE_PATH`).
- Implemented as an embedded SQLite database (or a crash-safe write-ahead log JSON engine).
- Independent of run directories (`.nudge/runs/`) and trace sinks (`trace.jsonl`).
- Persists across separate execution sessions, reboots, and clean invocations.

---

## 6. Security Boundary (Grants & Limits)

Execution permissions are defined via `NUDGE_WORKSPACE_GRANTS` (JSON policy, extending `crates/nudge-runtime/src/grants.rs`):

```json
{
  "root": "./workspace",
  "read": ["*"],
  "write": ["data/*", "build/*"],
  "delete": ["build/tmp/*"],
  "max_write_bytes": 10485760
}
```

### Enforcement Rules
- **Fail-Closed:** When `NUDGE_WORKSPACE_GRANTS` is set, any path not matching a glob pattern is denied (`E0911`).
- **Least Privilege:** Write permission does NOT imply delete permission. Deleting requires an explicit rule under `"delete"`.
- **Write Quota:** `max_write_bytes` limits the maximum bytes permitted in a single write operation. Writes exceeding this limit abort with a quota exceeded error.

---

## 7. Minimal Acceptance Tests

The implementation must pass these concrete acceptance tests:

1. **Workspace Boundary Test:**
   - Call `workspace.read("../secret.txt")` and `workspace.read("/etc/hosts")`.
   - Create a symlink `sym_escape -> /etc` inside the workspace and call `workspace.read("sym_escape/hosts")`.
   - **Verification:** Both operations fail with `E0910 (WorkspacePathEscape)`.

2. **Half-Write / Atomic Recovery Test:**
   - Pre-populate `report.txt` with `"valid content"`.
   - Initiate `workspace.write("report.txt", "interrupted partial payload")` and simulate a SIGKILL or panic before the atomic rename step.
   - Re-open `report.txt`.
   - **Verification:** `report.txt` still contains exactly `"valid content"`. No torn or truncated files exist.

3. **Delete Authorization Test:**
   - Configure policy granting `write: ["data/*"]` with empty or absent `"delete"` rule.
   - Run a program calling `workspace.delete("data/test.txt")`.
   - **Verification:** Operation fails with `E0912 (DeleteDenied)`.

4. **Persistent App Store Survival Test:**
   - Run Process 1: calls `store.set("session_count", 42)`. Process 1 exits cleanly.
   - Run Process 2 in a new session (different PID, fresh trace): calls `store.get("session_count", default: 0)`.
   - **Verification:** Returns `42`, proving storage persistence across independent process lifecycles.

---

## 8. Bytecode VM Execution Path Note

Nudge executes on the bytecode VM by default (`crates/nudgec/src/eval/interpreter.rs:1734-1756`, commit `3a66a56`).
- **Compilation:** In `crates/nudgec/src/bytecode.rs:732-750`, calls to `workspace.*` and `store.*` must not be classified as pure expressions. The bytecode compiler must compile them via `Op::DelegateExpr` or `Op::DelegateCall` (similar to inline `llm` and `tool` expressions, `bytecode.rs:747, 761`).
- **Delegation Execution:** During `vm::run_with_delegate`, the VM emits a `DelegateRequest::Call` or `DelegateRequest::Expr` to the interpreter hook (`interpreter.rs:1739-1752`).
- **Parity Guarantee:** The interpreter evaluates the operation against the authority checks, path confinement rules, and atomic storage handlers. The exact same security checks, effect verifications, and error diagnostics apply identically whether running on the default VM or the interpreter fallback (`NUDGE_VM=0`).
