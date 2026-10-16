# Nudge Quality Audit: Consistency, Security, Performance

- **Target Repository:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge`
- **Git Commit:** `684d461`
- **Mode:** Read-Only Audit (grep, read, AST/code inspection)
- **Constraint Compliance:** No code edits in repo, no commits, no cargo build/test executed.

---

## 1. Consistency

### 1.1 Diagnostic Codes (`hints.rs` and E-Code Usages)

#### Finding 1.1.1: Swapped and Inconsistent Descriptions between `E0102` and `E0103`
- **File & Line:** `crates/nudgec/src/hints.rs:11-12`, `crates/nudgec/src/check.rs:2427, 2455, 2478, 1243, 2413`
- **Severity:** Medium
- **Description:** `hints.rs` defines `E0102` as "the same type name is declared twice" and `E0103` as "duplicate declaration - every fn/tool/effect name must be unique". However, `check.rs` emits `E0102` for duplicate functions (`duplicate fn '{name}'`), duplicate tools (`duplicate tool '{name}'`), duplicate agents, and duplicate tests. Conversely, `check.rs` emits `E0103` only for duplicate parameter names (`check.rs:1243`) and duplicate record field declarations (`check.rs:2413`). As a result, users declaring duplicate functions or tools receive an error pointing them to rename a type alias instead of their function.
- **Minimal Fix:** Update `hints.rs` so `E0102` hints for duplicate top-level items (types, functions, tools, agents), and `E0103` hints for duplicate parameters or record fields; or align `check.rs` to emit `E0103` on duplicate top-level functions and tools.

#### Finding 1.1.2: Unused and Dead Diagnostic Codes in `hints.rs`
- **File & Line:** `crates/nudgec/src/hints.rs:33-35, 40`
- **Severity:** Low
- **Description:** Several E-codes defined in `hints.rs` are never emitted anywhere by the compiler or runtime:
  - `E0910` ("workspace path escape denied"): Defined in `hints.rs:33`, but `workspace.rs` returns a raw error string which `interpreter.rs:2340` formats as `error[E-native-0202]`.
  - `E0912` ("workspace.delete requires the WorkspaceDelete effect"): Defined in `hints.rs:34`, but `check.rs:440` treats `workspace.delete` by inserting `WorkspaceDelete` into required effects, which triggers standard `E0301`.
  - `E0913` ("store schema mismatch"): Defined in `hints.rs:35`, but never emitted anywhere in the codebase.
  - `E1002` ("invalid or unknown UI component"): Defined in `hints.rs:40`, but never emitted.
- **Minimal Fix:** Either remove unused codes from `hints.rs` or wire them into the appropriate validation sites (`workspace.rs`, `check.rs`, `store.rs`).

#### Finding 1.1.3: Missing Hints for Runtime `E-native-*` Codes
- **File & Line:** `crates/nudgec/src/hints.rs:43-52`, `crates/nudgec/src/eval/interpreter.rs:1107, 1438, 1443, 1447, 1450, 1795, 1811, 2254, 2328, 2340`
- **Severity:** Medium
- **Description:** The native runtime extensively emits `E-native-0100`, `E-native-0201`, `E-native-0202`, `E-native-0203`, `E-native-0300`, `E-native-0301`, `E-native-0302`, and `E-native-0303`. However, `hints.rs` contains zero entries for any `E-native-*` code.
- **Minimal Fix:** Add matching arms in `hints.rs::hint()` for all `E-native-01xx`, `E-native-02xx`, and `E-native-03xx` diagnostic codes.

#### Finding 1.1.4: Numbering Gaps and Ghost Reference to `E0805`
- **File & Line:** `crates/nudgec/src/codegen_ts.rs:382`, `crates/nudgec/src/hints.rs:26-30`
- **Severity:** Low
- **Description:** In `codegen_ts.rs:382`, code contains `// error: nested for_all (E0805)`. However, `E0805` does not exist in `hints.rs`, nor is it rejected in `check.rs` (nested `for_all` is allowed and supported in Rust AST and Python codegen). Furthermore, `E0803` is missing from the 08xx sequence.
- **Minimal Fix:** Clean up the comment in `codegen_ts.rs:382` or add proper diagnostic support for TypeScript generator limitations.

#### Finding 1.1.5: Runtime Errors Emitted without Diagnostic Code Prefix
- **File & Line:** `crates/nudgec/src/vm.rs:516, 587`, `crates/nudgec/src/eval/interpreter.rs:1255, 1656`, `crates/nudgec/src/hints.rs:22, 24`
- **Severity:** Medium
- **Description:** `hints.rs` defines `E0703` (loop iteration limit exceeded) and `E0705` (index out of bounds). However, both `vm.rs` and `interpreter.rs` emit bare strings without the diagnostic code prefix (e.g., `"loop iteration limit exceeded: execution halted after {limit} iterations..."` and `"index out of bounds: index {i} out of range for list of length {}"`). Because the code prefix is omitted, `main.rs` cannot print the corresponding hint.
- **Minimal Fix:** Prefix both error messages with `"error[E0703]: "` and `"error[E0705]: "` in `vm.rs` and `interpreter.rs`.

#### Finding 1.1.6: Contradiction on `E0922` (Process Confinement vs FS Watch)
- **File & Line:** `README.md:259`, `docs/roadmap.md:144` vs `crates/nudgec/src/hints.rs:38`, `crates/nudge-runtime/src/fs_watch.rs:145`
- **Severity:** Medium
- **Description:** `README.md` and `docs/roadmap.md` state: "Executing anything is fail-closed: denied by default unless authorized by NUDGE_PROCESS_GRANTS (E0920, workspace confinement E0922)". However, in `hints.rs` and `fs_watch.rs`, `E0922` is defined as "fs.watch path escape denied: watched path must remain inside workspace root". `proc.rs` does not implement workspace confinement and does not use `E0922`.
- **Minimal Fix:** Correct `README.md` to clarify that `E0922` applies to `fs.watch`, and document process path confinement separately if implemented.

---

### 1.2 Error Surface: Interpreter vs Default Bytecode VM

#### Finding 1.2.1: Divergent Assertion Failure Messages
- **File & Line:** `crates/nudgec/src/vm.rs:595` vs `crates/nudgec/src/eval/interpreter.rs:1195`
- **Severity:** Medium
- **Description:** On assertion failure, the VM emits `assertion failed: {expr_str} evaluated to {val}`, printing the source expression text. The interpreter emits `assertion failed: {expr:?} evaluated to {val}`, dumping the verbose internal Rust AST Debug struct. This creates divergent error output and test fragility depending on `NUDGE_VM`.
- **Minimal Fix:** In `interpreter.rs:1195`, use an expression formatter/display helper rather than `{expr:?}`.

#### Finding 1.2.2: Call Stack Overflow Error Message Discrepancy
- **File & Line:** `crates/nudgec/src/vm.rs:644` vs `crates/nudgec/src/eval/interpreter.rs:1997, 2075`
- **Severity:** Low
- **Description:** When recursion limits are reached, `vm.rs` returns `call stack overflow`, whereas `interpreter.rs` returns `call stack overflow: recursion limit exceeded`.
- **Minimal Fix:** Standardize both to `"call stack overflow: recursion limit exceeded"`.

#### Finding 1.2.3: `Op::Field` in VM Panics/Errors on Task and UI Values
- **File & Line:** `crates/nudgec/src/vm.rs:490-505`, `crates/nudgec/src/bytecode.rs:516-522`, `crates/nudgec/src/eval/interpreter.rs:1505-1525`
- **Severity:** High
- **Description:** `bytecode.rs` compiles field access (`ExprKind::Field`) directly to `Op::Field(name)`. In `vm.rs`, `Op::Field` only matches `NudgeVal::Map`. If a compiled function accesses fields on a `Task` handle (e.g., `task.id` or `task.status`) or a `UiElement`, the VM crashes with `cannot access field '{name}' on type task`. In contrast, `interpreter.rs` supports `NudgeVal::Task` and `NudgeVal::UiElement` field resolution.
- **Minimal Fix:** Add pattern arms for `NudgeVal::Task` and `NudgeVal::UiElement` in `vm.rs` `Op::Field`, or delegate field resolution when the base is not a Map.

#### Finding 1.2.4: Silent VM Fallback on `math.*`, `string.*`, `env.*`, `time.*` Standard Library Calls
- **File & Line:** `crates/nudgec/src/bytecode.rs:761-780, 979-995`
- **Severity:** High
- **Description:** In `bytecode.rs`, the compiler delegates effectful calls (`io.`, `workspace.`, `store.`, `task.`, `fs.`, `proc.`, `ui.`) to `emit_delegated_expr`. However, `math.*`, `string.*`, `env.*`, and `time.*` are completely omitted from the delegation list. Any program calling `math.abs()` or `string.split()` causes bytecode compilation to fail with `not in bytecode subset: unknown function or effect`, which silently forces the execution engine to fall back to the slower AST interpreter in `interpreter.rs:3420`.
- **Minimal Fix:** Add `name.starts_with("math.") || name.starts_with("string.") || name.starts_with("env.") || name.starts_with("time.")` to the delegation conditions at lines 761 and 979 in `bytecode.rs`.

#### Finding 1.2.5: No Hint Printing for Runtime Errors in `main.rs`
- **File & Line:** `crates/nudgec/src/main.rs:1085-1093`
- **Severity:** Medium
- **Description:** When `nudgec check` encounters an error, it calls `print_hint(e.code)`. But when `nudgec` executes a program via `eval::run_program` and receives an error formatted as `error[E...]: ...`, it simply prints the error and exits with code 1 without invoking `print_hint`.
- **Minimal Fix:** Parse the E-code from the error prefix (if present) and call `print_hint` before `process::exit(1)`.

---

### 1.3 Environment Variable Documentation vs Actual Reads

#### Finding 1.3.1: Documented in README Table but Unused in Native Rust Runtime
- **File & Line:** `README.md:517, 518, 522, 540`
- **Severity:** Medium
- **Description:** The README environment variable table lists variables that are only implemented in the Python or TypeScript runtime and completely ignored in `nudge-runtime` (Rust):
  - `NUDGE_GUARD`: Only read in `runtime/nudge_runtime/__init__.py:2226` and `runtime/nudge_runtime.ts:408`.
  - `NUDGE_REPAIR_BUDGET`: Only read in Python runtime (`__init__.py:1494`).
  - `NUDGE_RESUME`: Only handled in Python/TS runtime and compiler CLI subprocess, not in `nudge-runtime::trace`.
  - `NUDGE_OTEL` / `NUDGE_OTEL_ENDPOINT`: Only handled in Python runtime (`__init__.py:727, 749`).
- **Minimal Fix:** Annotate in the README table which variables apply only to Python/TS versus the native Rust runtime, or implement their handlers in `nudge-runtime`.

#### Finding 1.3.2: Used in Native Rust Runtime but Undocumented in README Table
- **File & Line:** `crates/nudge-runtime/src/proc.rs:124, 220`, `crates/nudge-runtime/src/fs_watch.rs:61`, `crates/nudge-runtime/src/provider/gate.rs:109`, `crates/nudge-runtime/src/auth/`
- **Severity:** Medium
- **Description:** Multiple environment variables are read in `crates/` but missing from the README environment variable table:
  - `NUDGE_PROCESS_GRANTS`: Crucial security gate for `proc.*` command allowlisting.
  - `NUDGE_PROCESS_TIMEOUT`: Default timeout for process executions.
  - `NUDGE_FS_WATCH_TIMEOUT`: Timeout for `fs.watch` operations.
  - `NUDGE_ENABLE_ANTIGRAVITY`: Opt-in toggle for Antigravity provider.
  - `NUDGE_AUTH_ANTIGRAVITY`, `NUDGE_AUTH_GITHUB_COPILOT`: Direct tokens for provider auth.
  - `NUDGE_AUTH_TIMEOUT_SECS`, `NUDGE_AUTH_POLL_INTERVAL_MS`: OAuth flow timing controls.
  - `NUDGE_FORCE_TTY`, `NUDGE_NON_INTERACTIVE`: TTY override controls.
  - `NUDGE_COMPUTER_SCENARIO`: Scenario selector for computer fake provider.
- **Minimal Fix:** Add these variables to the README environment variable table with their defaults and scopes.

---

### 1.4 Duplicated Logic between `interpreter.rs` and `vm.rs`

#### Finding 1.4.1: Duplicated Value Operations and Operator Coercions
- **File & Line:** `crates/nudgec/src/vm.rs:219-470` vs `crates/nudgec/src/eval/interpreter.rs:2840-2965`
- **Severity:** Medium
- **Description:** Binary arithmetic, string concatenation, list concatenation, and relational comparisons are completely re-implemented between `vm.rs` and `interpreter.rs`. Type coercion rules (e.g., int + float -> float, float < int conversions) and error texts (`cannot compare {} < {}`) are duplicated across both modules, creating risk of semantic divergence when language operators evolve.
- **Minimal Fix:** Move binary/unary evaluation functions into `crates/nudgec/src/eval/val.rs` as methods on `NudgeVal`, and call them from both `vm.rs` and `interpreter.rs`.

---

## 2. Security (Defense-in-Depth)

### 2.1 Workspace Path Handling

#### Finding 2.1.1: TOCTOU and Symlink Dereference in `workspace.append`
- **File & Line:** `crates/nudge-runtime/src/workspace.rs:324-332`
- **Severity:** High
- **Description:** `workspace.append` validates `safe_path` using `resolve_and_validate_path`. It then opens `safe_path` with `fs::OpenOptions::new().create(true).append(true).open(&safe_path)`. Standard `fs::OpenOptions::open` follows symlinks. If a symlink is created or swapped at `safe_path` between resolution and open, writes will append directly to targets outside the workspace (e.g. `/etc/passwd` or user dotfiles).
- **Minimal Fix:** Use `custom_flags(libc::O_NOFOLLOW)` on Unix or verify that `safe_path.symlink_metadata()?.is_symlink()` is false immediately before and after opening.

#### Finding 2.1.2: Absolute Path Strip Failure with Symlinked Workspace Root
- **File & Line:** `crates/nudge-runtime/src/workspace.rs:76-86`
- **Severity:** Medium
- **Description:** In `resolve_and_validate_path`, absolute user paths are validated via `input_path.strip_prefix(&canonical_root)`. If `raw_root` contains a symlink (for example, `/home/user/workspace` symlinked to `/data/workspace`), an absolute input path using the user's workspace path (`/home/user/workspace/file.txt`) will fail `strip_prefix(&canonical_root)` because `canonical_root` is `/data/workspace`. The runtime rejects valid workspace accesses with a false `workspace path escape denied`.
- **Minimal Fix:** Canonicalize `input_path` (or its existing ancestor) before `strip_prefix(&canonical_root)`.

#### Finding 2.1.3: Dead and Non-Portable `lexical_normalize`
- **File & Line:** `crates/nudge-runtime/src/workspace.rs:33-53`
- **Severity:** Low
- **Description:** `lexical_normalize` hardcodes `normalized.push("/")` on `Component::RootDir` and skips `Component::Prefix(_)`, breaking Windows drive letters (`C:\...`). Furthermore, `lexical_normalize` is dead code never called by `resolve_and_validate_path`.
- **Minimal Fix:** Remove or refactor `lexical_normalize` to use standard `Path` component rebuilding across platforms.

---

### 2.2 Process Command Handling

#### Finding 2.2.1: No Working Directory Confinement for `proc.*`
- **File & Line:** `crates/nudge-runtime/src/proc.rs:330-335`
- **Severity:** Medium
- **Description:** Despite documentation in `README.md:259` promising "workspace confinement E0922" for processes, `Command::new(cmd)` does not call `.current_dir(workspace_root)`. Spawned processes run in the host process working directory, which may be outside the intended workspace root.
- **Minimal Fix:** Add `.current_dir(crate::workspace::get_workspace_root())` when configuring `Command`.

---

### 2.3 Grants Fail-Closed Coverage

#### Finding 2.3.1: Empty Argument List Bypasses Argument Grants in `proc.rs`
- **File & Line:** `crates/nudge-runtime/src/proc.rs:158-166`
- **Severity:** High
- **Description:** In `check_process_grants`, when a rule specifies permitted arguments (e.g., `{"git": ["status"]}` or `{"python": ["safe.py"]}`):
  ```rust
  let all_match = args.iter().all(|arg| {
      rules.iter().any(|r| r.as_str().map(|pat| crate::grants::fnmatch(pat, arg)).unwrap_or(false))
  });
  ```
  In Rust, `args.iter().all(...)` evaluates to `true` on an empty slice! Therefore, calling `proc.run("git", [])` or `proc.run("python", [])` returns `all_match = true` and succeeds, completely bypassing the intended command argument restrictions.
- **Minimal Fix:** If `rules` does not contain `*`, require `!args.is_empty()` and enforce positional or full-slice matching:
  ```rust
  if args.is_empty() {
      return Err(format!("error[E0920]: process '{cmd}' requires arguments matching grant policy"));
  }
  ```

---

### 2.4 Secret Handling in Auth

#### Finding 2.4.1: Trace Redaction Excludes `NUDGE_*` Auth Secrets
- **File & Line:** `crates/nudge-runtime/src/redact.rs:74-76`
- **Severity:** High
- **Description:** `redact.rs` implements `matches_env_secret_name`:
  ```rust
  let upper = name.to_ascii_uppercase();
  if upper.starts_with("NUDGE_") {
      return false;
  }
  ```
  Because of this blanket exemption, environment variables used by Nudge itself to store credentials—including `NUDGE_AUTH_ANTIGRAVITY`, `NUDGE_AUTH_GITHUB_COPILOT`, `NUDGE_API_KEY`, and `NUDGE_ANTIGRAVITY_CLIENT_SECRET`—are excluded from the automatic environment secret detector in `load_env_secrets()`. If their values do not match standard regex patterns (e.g. `sk-...`), they will not be redacted from execution traces.
- **Minimal Fix:** Allow known credential suffixes even for `NUDGE_` variables:
  ```rust
  if upper.starts_with("NUDGE_") && !upper.contains("KEY") && !upper.contains("SECRET") && !upper.contains("AUTH") && !upper.contains("TOKEN") {
      return false;
  }
  ```

#### Finding 2.4.2: Unredacted OAuth Server Response Bodies in Error Strings
- **File & Line:** `crates/nudge-runtime/src/auth/loopback.rs:549, 666`, `crates/nudge-runtime/src/auth/device_flow.rs:274`
- **Severity:** Medium
- **Description:** When an OAuth token exchange or refresh endpoint returns an error, the raw response body is formatted into the error string:
  `"error[E-auth-0401]: token exchange returned HTTP status {status}: {resp_body}"`.
  If the authorization server echoes request parameters or returns verbose debug payloads containing partial tokens, these are printed directly to `eprintln!` unmasked.
- **Minimal Fix:** Pass `resp_body` through `crate::redact::Redactor::from_env().redact_string(&resp_body)` or truncate to status and sanitized error codes.

---

### 2.5 Credentials and Store File Permissions

#### Finding 2.5.1: Permission Window / Race Condition in `save_to`
- **File & Line:** `crates/nudge-runtime/src/auth/store.rs:106-112`
- **Severity:** High
- **Description:** In `CredentialStore::save_to`, credentials are written via `std::fs::write(&tmp_path, json)`. On Unix, `std::fs::write` creates the file with the ambient process umask (commonly `0644`). Only after writing finishes is `set_permissions(&tmp_path, 0o600)` called. This leaves a window where credentials containing OAuth tokens are world-readable on multi-user systems.
- **Minimal Fix:** On Unix, open `tmp_path` using `std::os::unix::fs::OpenOptionsExt::mode(0o600)` prior to writing:
  ```rust
  #[cfg(unix)]
  use std::os::unix::fs::OpenOptionsExt;
  let mut opts = std::fs::OpenOptions::new();
  opts.write(true).create(true).truncate(true);
  #[cfg(unix)]
  opts.mode(0o600);
  let mut file = opts.open(&tmp_path)?;
  ```

#### Finding 2.5.2: Unrestricted File Mode on Persistent Store (`.nudge/store.json`)
- **File & Line:** `crates/nudge-runtime/src/store.rs:232-265`
- **Severity:** Medium
- **Description:** In `store.rs`, `Store::flush()` creates the store file via `fs::File::create(&temp_path)` and creates the directory via `fs::create_dir_all(parent)` without applying `0600` or `0700` permissions. Persistent data stored via `store.set` is left world-readable if ambient umask is `0022`.
- **Minimal Fix:** Apply `0o600` permissions on Unix to `temp_path` and `0o700` to `.nudge`.

---

### 2.6 Replay Determinism Integrity

#### Finding 2.6.1: Missing `request_hash` Falls Back to Unchecked FIFO Replay
- **File & Line:** `crates/nudge-runtime/src/trace.rs:652-686, 717-760`
- **Severity:** High
- **Description:** In both `replay_take_llm` and `tool_call_replay`, the runtime checks `st.records.iter().all(|r| r.get("request_hash").is_some())`. If a crafted trace omits `request_hash` from records, the runtime falls back to legacy FIFO order (`st.order_idx`). Under FIFO order, tool calls and LLM calls return recorded output without validating the tool name, server name, prompt, or arguments. A crafted trace can feed arbitrary attacker-controlled data into subsequent tool invocations or file writes.
- **Minimal Fix:** In replay mode, fail closed when `request_hash` is missing, or restrict legacy order strictly to explicit opt-in flags.

#### Finding 2.6.2: Unbounded Trace Memory Allocation Denial-of-Service
- **File & Line:** `crates/nudge-runtime/src/trace.rs:561, 584`
- **Severity:** Medium
- **Description:** Trace loading reads the entire file into memory with `std::fs::read_to_string(path)` twice (once for LLM records, once for tool records). A large or malicious trace file can trigger out-of-memory crashes.
- **Minimal Fix:** Stream the file using `std::io::BufReader::lines()` in a single pass to collect both LLM and tool records.

---

## 3. Performance (Hot Paths Only)

### 3.1 Trace Writer Per-Record Overhead

#### Finding 3.1.1: Re-instantiating `Redactor::from_env()` on Every Trace Record
- **File & Line:** `crates/nudge-runtime/src/trace.rs:349`, `crates/nudge-runtime/src/redact.rs:163-176, 316-320`
- **Severity:** Critical
- **Impact:** 10x-50x slowdown on tracing workloads; millions of unnecessary allocations and regex compilations.
- **Description:** Every trace record (and every function return via `emit_fn_return`) invokes `redact_record_fields`, which executes `Redactor::from_env()`. This scans all process environment variables, allocates multiple vectors, and parses regexes on every single record write.
- **Minimal Fix:** Cache the `Redactor` instance in a thread-safe `LazyLock<RwLock<Redactor>>` or `Arc<Redactor>` on the trace writer.

---

### 3.2 Parser Clones in Lookaheads

#### Finding 3.2.1: Redundant `self.peek().clone()` in Parser Hot Paths
- **File & Line:** `crates/nudgec/src/parser.rs:151, 161, 192, 299, 501, 601, 676, 957, 1062, 1126, 1218, 1232, 1263, 1447`
- **Severity:** High
- **Impact:** Eliminates dozens of string heap allocations per parsed statement and expression.
- **Description:** In `parser.rs`, code frequently writes `match self.peek().clone() { ... }` or `(self.peek().clone(), self.peek2().clone())`. Because `Tok` contains heap-allocated `String`s (`Ident`, `Str`, `Prompt`), this needlessly clones strings just to inspect enum variants.
- **Minimal Fix:** Change call sites to borrow tokens: `match self.peek() { ... }` matching on references (`&Tok::Ident(ref name)`).

---

### 3.3 VM Execution Hot Paths and Stack Copies

#### Finding 3.3.1: O(N^2) String Concatenation in Loops
- **File & Line:** `crates/nudgec/src/vm.rs:236`, `crates/nudgec/src/eval/interpreter.rs:2846`
- **Severity:** High
- **Impact:** Degrades string concatenation loops from O(N) to O(N^2).
- **Description:** When evaluating `NudgeVal::Str(a) + NudgeVal::Str(b)`, both the VM and interpreter execute `NudgeVal::Str(format!("{a}{b}"))`. In loops, this repeatedly allocates new heap strings rather than reusing capacity or mutating in place.
- **Minimal Fix:** Mutate `a` in place using `a.push_str(&b)` when `a` is uniquely owned.

#### Finding 3.3.2: Scope Cloning in `Op::DelegateExpr`
- **File & Line:** `crates/nudgec/src/vm.rs:675-684`, `crates/nudgec/src/eval/interpreter.rs:3428-3435`
- **Severity:** Medium
- **Impact:** Heavy allocation per delegated expression; clones all in-scope locals into a new environment map.
- **Description:** Whenever a delegated expression executes, `vm.rs` clones all active local variable names and values into a `Vec<(String, NudgeVal)>`. Then `run_program` allocates an `Rc<RefCell<Env>>` and inserts every cloned local into a `HashMap`.
- **Minimal Fix:** Pass a borrowed local slice/scope lookup adapter into `DelegateRequest::Expr` rather than allocating and cloning into a new `Env`.

#### Finding 3.3.3: Heap-Allocated Constants Cloned on Every `Op::Const`
- **File & Line:** `crates/nudgec/src/vm.rs:151-154`
- **Severity:** Medium
- **Impact:** Unnecessary string and collection allocations on every bytecode constant push.
- **Description:** `Op::Const(idx)` executes `let val = chunk.constants[*idx as usize].clone(); self.stack.push(val);`. For large strings, lists, or maps, pushing a constant inside a loop deep-clones the value repeatedly.
- **Minimal Fix:** Wrap complex constants in `Arc` or `Rc`, or use copy-on-write pointers in `NudgeVal`.
