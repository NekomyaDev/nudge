# Nudge Test Quality Audit

Target repository: `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` (HEAD `684d461`)

---

## 1. Flake Risks

### FLAKE-01: Secondary `EnvGuard` and Separate `ENV_LOCK` in `auth/tests.rs`
- **File and Line:** `crates/nudge-runtime/src/auth/tests.rs:20-53`
- **Why It Is a Problem:**
  `auth/tests.rs` defines its own `get_test_lock()` which locks `crate::provider::tests::ENV_LOCK` instead of `crate::trace::testing::TEST_STATE_LOCK`. Meanwhile, `provider/tests.rs` uses `TEST_STATE_LOCK` inside its `EnvGuard`. Because `ENV_LOCK` and `TEST_STATE_LOCK` are distinct mutexes, tests in `auth/tests.rs` and tests in `provider/tests.rs` or `trace.rs` run concurrently while mutating process-global environment variables (`HOME`, `NUDGE_API_KEY`, `NUDGE_CREDENTIALS_PATH`, `NUDGE_ENABLE_ANTIGRAVITY`). Furthermore, `auth/tests.rs:EnvGuard` does not acquire any lock on instantiation, so if a test creates an `EnvGuard` without manually calling `let _lock = get_test_lock()`, it mutates environment variables with zero synchronization.
- **Minimal Fix:**
  Unify `auth/tests.rs` to reuse `crate::provider::tests::EnvGuard` (or lock `crate::trace::testing::TEST_STATE_LOCK` directly inside `EnvGuard::set` / `EnvGuard::unset`).

---

### FLAKE-02: Unlocked `EnvReset` in `nudgec` AST Interpreter Tests
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:3830-3860`, `5548`, `5971`
- **Why It Is a Problem:**
  `interpreter.rs` defines a local `EnvReset` helper that mutates process-global environment variables via `std::env::set_var` and `std::env::remove_var` without acquiring any lock. Tests like `test_headless_interactive_editing_scrolling_and_keys` (line 5548) and `test_headless_cursor_preserved_mid_string_editing` (line 5971) set `NUDGE_UI_HEADLESS=1` using `EnvReset` without locking `ENV_LOCK`. When `cargo test` executes tests in parallel, concurrent tests race on environment variable reads and writes.
- **Minimal Fix:**
  Have `EnvReset::set` and `EnvReset::remove` lock `ENV_LOCK` (or `nudge_runtime::trace::testing::TEST_STATE_LOCK`), or ensure every test using `EnvReset` acquires `let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());` at the top of the test function.

---

### FLAKE-03: Missing `NODE_LOCK` in `ts_computer_program_runs_under_node`
- **File and Line:** `crates/nudgec/src/codegen_ts.rs:1650-1682`
- **Why It Is a Problem:**
  `codegen_ts.rs` defines `NODE_LOCK` (line 1192) to serialize tests that invoke `node` and write to temporary directories. Every other `node` integration test (`ts_par_map_two_params_destructures_the_element`, `ts_agent_state_checkpoints_and_resumes_under_node`, `ts_hello_runs_under_node`, `ts_budget_walls_match_python_semantics`, `ts_ui_acceptance_runs_under_node`) acquires `NODE_LOCK`. However, `ts_computer_program_runs_under_node` was added without acquiring `NODE_LOCK`, causing it to execute concurrently with other Node processes and race on filesystem operations in `/tmp`.
- **Minimal Fix:**
  Add `let _nl = NODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());` at the start of `ts_computer_program_runs_under_node`.

---

### FLAKE-04: Multiple Disjoint Test Locks Across `nudge-runtime` Subsystems
- **File and Line:**
  - `crates/nudge-runtime/src/trace.rs:436` (`TEST_STATE_LOCK`)
  - `crates/nudge-runtime/src/workspace.rs:450` (`WS_TEST_LOCK`)
  - `crates/nudge-runtime/src/task.rs:676` (`TASK_LOCK`)
  - `crates/nudge-runtime/src/proc.rs:44` (`PROC_TEST_LOCK`)
  - `crates/nudge-runtime/src/provider/tests.rs:4` (`ENV_LOCK`)
- **Why It Is a Problem:**
  These five independent mutexes govern shared global state. For example, `proc.rs` reads and modifies `task::current_task_id()` and calls `task::is_current_cancelled()`, while `task.rs` invokes `proc::reap_processes_for_task(&self.id)`. Tests in `proc.rs` hold only `PROC_TEST_LOCK`, while tests in `task.rs` hold only `TASK_LOCK`. Similarly, `store.rs` tests hold `WS_TEST_LOCK` while setting `NUDGE_WORKSPACE_DIR` and `NUDGE_STORE_PATH`, which are not covered by `TEST_STATE_LOCK`. Cross-subsystem tests can run concurrently and collide on global state.
- **Minimal Fix:**
  Establish a unified root lock (such as `TEST_STATE_LOCK`) for any test that mutates process-global state, or acquire `TEST_STATE_LOCK` within the subsystem guard constructors.

---

### FLAKE-05: Predictable PID-Only Temporary Paths Shared Across Parallel Test Threads
- **File and Line:**
  - `crates/nudge-runtime/src/auth/tests.rs:58, 122, 221, 537, 726, 796` (`nudge_test_store_{pid}`, etc.)
  - `crates/nudge-runtime/src/stdlib/tests.rs:32` (`nudge_io_test_{pid}`)
  - `crates/nudge-runtime/src/fs_watch.rs:262` (`nudge_fswatch_{suffix}_{pid}`)
  - `crates/nudge-runtime/src/trace.rs:957` (`nudge-trace-{name}-{pid}.jsonl`)
  - `crates/nudgec/src/codegen/tests.rs:144` (`nudge_e2e_{pid}_{name}`)
  - `crates/nudgec/src/codegen_ts.rs:1369, 1443, 1495, 1543, 1661` (`nudge_ts*_{pid}`)
- **Why It Is a Problem:**
  `std::process::id()` is identical for every thread within the test runner process. When tests with the same suffix run or when subtests share a pattern, they share the exact same path under `/tmp`. Any residual files or concurrent execution will clobber store files, credential stores, or watched directories.
- **Minimal Fix:**
  Append a high-resolution timestamp or thread ID (for example, `std::thread::current().id()`) or use `tempfile::tempdir()` to guarantee uniqueness per test run.

---

### FLAKE-06: Hardcoded Wall-Clock Sleeps for Thread and Process Synchronization
- **File and Line:**
  - `crates/nudge-runtime/src/auth/tests.rs:585, 653, 696` (`thread::sleep(Duration::from_millis(50))`)
  - `crates/nudge-runtime/src/proc.rs:859, 963` (`thread::sleep(Duration::from_millis(50))`)
  - `crates/nudge-runtime/src/task.rs:910` (`thread::sleep(Duration::from_millis(10))`)
  - `crates/nudge-runtime/src/fs_watch.rs:285` (`thread::sleep(Duration::from_millis(60))`)
  - `crates/nudgec/src/eval/interpreter.rs:5679, 5876` (`thread::sleep(Duration::from_millis(50))`)
- **Why It Is a Problem:**
  Tests rely on arbitrary sleeps (10ms to 60ms) to ensure background threads start, TCP listeners bind, processes spawn, or watcher loops initialize. On heavily loaded CI runners or high-concurrency test runs, scheduling latency can easily exceed 50ms, resulting in intermittent timeouts or assertion failures.
- **Minimal Fix:**
  Replace fixed sleeps with polling loops bounded by a deadline or synchronization primitives like `std::sync::mpsc::channel()` or `std::sync::Barrier`.

---

### FLAKE-07: Display Requirement in Desktop GUI Acceptance Test
- **File and Line:** `crates/nudge-runtime/src/ui_window.rs:1412-1419`
- **Why It Is a Problem:**
  `test_native_gui_desktop_acceptance` requires an active X11 or Wayland display. While marked with `#[ignore]`, running `cargo test -- --ignored` will fail on headless machines or open interactive windows on developer desktops, interfering with active work.
- **Minimal Fix:**
  Keep the headless mock tests as the standard verification target and gate `test_native_gui_desktop_acceptance` behind a dedicated Cargo feature (such as `gui-acceptance`) rather than just `#[ignore]`.

---

## 2. Coverage Gaps

### GAP-01: UI Headless Dispatch and Key-Routing Methods Have Zero Tests
- **File and Line:** `crates/nudge-runtime/src/ui.rs:648, 665, 681-716, 717, 723, 738, 767`
- **Unchecked Functions/Branches:**
  - `HeadlessMount::dispatch_key_to(target_id, key)` (line 767) is never called in any test.
  - Cursor dispatch helpers: `dispatch_cursor_left`, `dispatch_cursor_right`, `dispatch_cursor_up`, `dispatch_cursor_down`, `dispatch_cursor_home`, `dispatch_cursor_end` (lines 681-716) have zero tests.
  - Headless editing actions: `dispatch_cut`, `dispatch_paste`, `dispatch_delete`, `dispatch_newline`, `dispatch_select_all` (lines 648, 665, 717, 723, 738) have zero tests.
  - `TextInputState::move_doc_start` (line 263), `move_doc_end` (line 274), and forward `delete` (line 188) have no unit tests.
- **Behavior to Assert:**
  Assert that `dispatch_key_to(Some("input_id"), "Enter")` targets the specified element handler. Assert that `dispatch_cursor_*` moves cursor and adjusts selection when `extend_selection` is true. Assert that `dispatch_delete` removes character at cursor forward and returns the change handler.

---

### GAP-02: `fs.watch` File Removal Event and Task Cancellation Untested
- **File and Line:** `crates/nudge-runtime/src/fs_watch.rs:155, 175-186`
- **Unchecked Functions/Branches:**
  - "removed" event branch (lines 175-186) checking that deleted files emit a `kind: "removed"` event.
  - `if crate::task::is_current_cancelled()` loop exit (line 155) returning `error[E-task-0001]: TaskCancelled`.
- **Behavior to Assert:**
  Delete a file in a watched directory while `fs.watch` is running and assert that `events[0].kind == "removed"`. Cancel the task holding `fs.watch` from a separate thread and assert that `fs.watch` immediately exits with `TaskCancelled`.

---

### GAP-03: `proc.*` Invalid Handles, Automatic Reaping, and Cancellation Pruning Untested
- **File and Line:** `crates/nudge-runtime/src/proc.rs:494, 514, 556, 589`
- **Unchecked Functions/Branches:**
  - `await_child` on unknown handle (line 494) returning `process handle '<handle>' not found`.
  - `kill_child` on unknown handle (line 514) returning `Ok(Value::Bool(false))`.
  - `reap_cancelled_processes()` (line 556) and `reap_processes_for_task()` (line 589).
- **Behavior to Assert:**
  Assert that calling `proc.await("invalid_handle")` returns an error. Assert that calling `proc.kill("invalid_handle")` returns false. Spawn a child under a named task, cancel the task, and assert that `reap_cancelled_processes()` terminates the child and removes it from `PROCESS_REGISTRY`.

---

### GAP-04: Registry API Functions in `task.rs` Untested
- **File and Line:** `crates/nudge-runtime/src/task.rs:589, 601, 612, 621`
- **Unchecked Functions/Branches:**
  - `task_status(id)` (line 601) returning current status or `error[E-task-0003]: InvalidTaskHandle`.
  - `task_cancel(id)` (line 589) returning `error[E-task-0003]` when `id` is not registered.
  - `task_is_cancelled(Some(id))` (line 612).
  - `task_wait_cancelled(Some(id))` (line 621).
- **Behavior to Assert:**
  Assert that `task_status("nonexistent")` and `task_cancel("nonexistent")` return `E-task-0003`. Assert that `task_is_cancelled(Some("valid_task"))` reflects parent and task cancellation accurately.

---

### GAP-05: `workspace.append` and Recursive `workspace.delete` Untested
- **File and Line:** `crates/nudge-runtime/src/workspace.rs:306, 428`
- **Unchecked Functions/Branches:**
  - `workspace.append` command (line 306).
  - `workspace.delete` with `recursive = true` on directories (line 428).
  - Validation failure branches for missing/invalid arguments across `workspace.*` and `store.*`.
- **Behavior to Assert:**
  Write initial text, call `workspace.append("file.txt", " more")`, and assert file content equals concatenated string. Create a directory containing nested files, call `workspace.delete("dir", true)`, and assert directory and children are removed.

---

### GAP-06: RFC 8628 Device Flow Error and Cancellation Branches Untested
- **File and Line:** `crates/nudge-runtime/src/auth/device_flow.rs:180, 187, 208, 212, 218, 224`
- **Unchecked Functions/Branches:**
  - Polling timeout (line 180) returning `error[E-auth-0402]`.
  - Task cancellation (line 187) returning `error[E-task-0001]: TaskCancelled`.
  - Provider response error codes: `slow_down` (line 208), `expired_token` (line 212), `access_denied` (line 218), and unexpected error (line 224).
- **Behavior to Assert:**
  Mock the device authorization token endpoint returning `{"error": "access_denied"}` or `{"error": "expired_token"}` and assert `authenticate()` returns `E-auth-0402`. Mock a response with `slow_down` and assert polling interval increases.

---

### GAP-07: Fixes LEAK-04 and ERR-01 from Commit 684d461 Lack Dedicated Tests
- **File and Line:**
  - `crates/nudge-runtime/src/mcp.rs:253, 259, 265` (LEAK-04: MCP cancellation and 16MB read cap)
  - `crates/nudgec/src/eval/interpreter.rs:3191` (ERR-01: `eval_stream_llm` trace error propagation)
- **Unchecked Functions/Branches:**
  - In `mcp.rs:253, 259`: `if crate::task::is_current_cancelled() { return Err("error[E-task-0001]: TaskCancelled".into()); }`
  - In `mcp.rs:265`: `(&mut self.reader).take(max_bytes).read_line(&mut line)`
  - In `interpreter.rs:3191`: Trace emit error propagation with `?` in `eval_stream_llm`.
- **Behavior to Assert:**
  Assert that calling `McpSession::send_request` in a cancelled task returns `TaskCancelled` before and during reading. Assert that `eval_stream_llm` returns an error when trace writing fails (for example, targeting an unwritable path).

---

## 3. Weak Assertions

### WEAK-01: `test_antigravity_provider_reads_only_nudge_store_not_jcode` Bypasses Store and Ignores Failure Root Cause
- **File and Line:** `crates/nudge-runtime/src/auth/tests.rs:837-848, 883-884`
- **Why It Is Weak:**
  Line 837 executes `assert!(provider.call(...).is_err(), "Fails on network connecting to 8642, NOT on missing auth!")`. This assertion passes for any network or configuration error regardless of whether foreign credentials were read. Worse, lines 883-884 instantiate `AntigravityProvider::with_token("ya29.nudge_own_legitimate_token", Some(mock_url))`, which directly sets the token in the constructor without ever querying the credential store. The test therefore never tests if `AntigravityProvider::new()` reads Nudge store credentials instead of `~/.jcode`.
- **Minimal Fix:**
  Construct the provider with `AntigravityProvider::new()` (or configure base URL without injecting a manual token) and verify against the mock server that the request carries the store's token.

---

### WEAK-02: Non-Recursive Source Directory Scan in `test_never_reads_foreign_credentials_directories`
- **File and Line:** `crates/nudge-runtime/src/auth/tests.rs:444-468`
- **Why It Is Weak:**
  `std::fs::read_dir(src_dir)` only iterates immediate entries in `crates/nudge-runtime/src/`. It does not recurse into `src/auth/`, `src/provider/`, or `src/stdlib/`. If any foreign harvester code were added inside `src/auth/` or `src/provider/`, this test would completely miss it and pass green.
- **Minimal Fix:**
  Use recursive directory traversal (or a recursive walk function) to inspect all `.rs` files under `src/`.

---

### WEAK-03: `is_ok()` Assertions Without Verifying Return Values in `test_os_process_functions`
- **File and Line:** `crates/nudge-runtime/src/stdlib/tests.rs:853-876`
- **Why It Is Weak:**
  `test_os_process_functions` asserts `assert!(args_res.is_ok())`, `assert!(cwd_res.is_ok())`, and `assert!(pid_res.is_ok())`. If `os.pid` returned 0 or -1, or if `os.cwd` returned an empty string, the test would still pass.
- **Minimal Fix:**
  Unwrap the values and assert that `pid > 0`, `cwd` is a valid non-empty path, and `args` is a `Value::List`.

---

### WEAK-04: `is_ok()` Assertions Without Verifying Timestamps or Formats in `test_time_now_and_format`
- **File and Line:** `crates/nudge-runtime/src/stdlib/tests.rs:800-834`
- **Why It Is Weak:**
  `now_res`, `fmt_res`, and `fmt_int` are asserted only with `is_ok()`. They do not verify that the returned timestamp is greater than 0 or that the formatted string matches the expected ISO date format.
- **Minimal Fix:**
  Assert that `now > 1_700_000_000` and formatted date matches regex or expected string components.

---

### WEAK-05: Missing Error Message Checks in `test_time_sleep`
- **File and Line:** `crates/nudge-runtime/src/stdlib/tests.rs:843-849`
- **Why It Is Weak:**
  The test asserts `.is_err()` on `f64::NAN`, `f64::INFINITY`, `1.0 / 0.0`, `-1.0`, and `f64::MAX`. If `time.sleep` failed for an unexpected reason (like an argument parsing failure), the test would still pass.
- **Minimal Fix:**
  Unwrap the error strings and assert they contain `"time.sleep: duration must be finite, non-negative, and fit in u64 seconds"`.

---

### WEAK-06: Silent Early Returns in TypeScript Codegen Integration Tests
- **File and Line:** `crates/nudgec/src/codegen_ts.rs:1314, 1362, 1436, 1488, 1536, 1654`
- **Why It Is Weak:**
  Six integration tests check `if Command::new("node").arg("--version").output().is_err() { return; }` and `if !runtime.exists() { return; }`. If Node is not installed or the relative path to `runtime/` is unresolved, these tests exit cleanly without executing any code or making any assertions, masking broken lowerings.
- **Minimal Fix:**
  Log an explicit warning or use an environment flag (like `CI=true`) that fails if required dependencies are absent.

---

## 4. Duplicate Tests That Could Drift

### DUP-01: Byte-for-Byte Duplicated `json.rs` and Unit Tests
- **File and Line:** `crates/nudge-runtime/src/json.rs` vs `crates/nudgec/src/json.rs`
- **Why It Is a Problem:**
  `crates/nudgec/src/json.rs` is an identical copy of `crates/nudge-runtime/src/json.rs`, including tests `test_json_roundtrip`, `test_json_numbers_print_int_when_whole`, and `surrogate_pairs_decode_and_nonfinite_numbers_dump_as_null`. When a bug fix or performance tweak is made to one JSON parser, the other copy drifts silently.
- **Minimal Fix:**
  Have `nudgec` import `nudge_runtime::json` instead of maintaining a parallel copy of the module and its tests.

---

### DUP-02: Identical `test_budget_reservation_released_on_cancel` in Two Crates
- **File and Line:** `crates/nudge-runtime/src/task.rs:817-851` vs `crates/nudgec/src/eval/interpreter.rs:4907-4945`
- **Why It Is a Problem:**
  The entire test logic for verifying that task cancellation frees budget reservations is duplicated line-for-line between `nudge-runtime/src/task.rs` and `nudgec/src/eval/interpreter.rs`. Changes to budget logic or error strings require updating both copies.
- **Minimal Fix:**
  Keep the lower-level unit test in `task.rs` and make the interpreter test focus solely on Nudge source evaluation via `run_program`.

---

### DUP-03: Duplicated Antigravity Foreign Credential Isolation Tests
- **File and Line:** `crates/nudge-runtime/src/auth/tests.rs:793-849` vs `crates/nudge-runtime/src/provider/tests.rs:404-463`
- **Why It Is a Problem:**
  Both test suites set up `.jcode/antigravity_oauth.json` and verify that foreign credentials are not read. However, they use different lock mechanisms (`ENV_LOCK` vs `TEST_STATE_LOCK`) and configure slightly different environment variables, allowing their assertions and assumptions to drift out of sync.
- **Minimal Fix:**
  Consolidate the foreign credential isolation check into `auth/tests.rs` where store loading is verified, and remove the redundant test in `provider/tests.rs`.

---

### DUP-04: Tool Grant Rule Matrix Duplicated in Rust and Python
- **File and Line:** `crates/nudge-runtime/src/grants.rs:280-345` vs `runtime/test_nudge_runtime.py:48-64`
- **Why It Is a Problem:**
  The rule evaluation matrix (absent env, empty string, empty JSON object, malformed JSON, non-object, wildcard matches) is tested twice in separate languages. Any change to error message format or rule parsing syntax in the Rust engine can cause Python test fixtures to become obsolete.
- **Minimal Fix:**
  Use a shared JSON test fixture file (e.g. `tests/fixtures/grant_matrix.json`) loaded by both the Rust and Python test suites.
