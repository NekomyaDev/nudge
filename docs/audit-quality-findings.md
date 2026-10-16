# Nudge Quality Audit Findings

Target repository: `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge` (HEAD `92331d6`)

## 1. Panic Risks

### PANIC-01: `str.substring` Out-of-Order or Inverted Slice Panic
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2635`
- **Exact Code:**
```rust
let chars: Vec<char> = s.chars().collect();
let sub = chars[start.min(chars.len())..end.min(chars.len())]
    .iter()
    .collect();
```
- **Why It Is a Problem:**
If `start > end` (for example, `str.substring("hello", 3, 1)`), Rust range indexing panics with `slice index starts at 3 but ends at 1`. Furthermore, when `start` is negative, `*i as usize` wraps to `usize::MAX`, causing `start.min(chars.len())` to become `chars.len()`. If `end < chars.len()`, it triggers an inverted slice and crashes the process.
- **Minimal Fix:**
```rust
let a = start.min(chars.len());
let b = end.min(chars.len());
let sub = if a <= b {
    chars[a..b].iter().collect()
} else {
    String::new()
};
```

---

### PANIC-02: Modulo Overflow Panic on `i64::MIN % -1`
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2835` and `crates/nudgec/src/vm.rs:334`
- **Exact Code:**
```rust
// interpreter.rs:2835:
(NudgeVal::Int(a), NudgeVal::Int(b)) => {
    if b == 0 {
        return Err("modulo by zero".into());
    }
    Ok(NudgeVal::Int(a % b))
}

// vm.rs:334:
(NudgeVal::Int(a), NudgeVal::Int(b)) => {
    if b == 0 {
        return Err("modulo by zero".into());
    }
    self.stack.push(NudgeVal::Int(a % b));
}
```
- **Why It Is a Problem:**
In Rust, signed integer modulo `i64::MIN % -1` panics unconditionally in both debug and release builds with `attempt to calculate the remainder with overflow`. A valid Nudge program executing `let x = -9223372036854775808; x % -1` crashes the runtime.
- **Minimal Fix:**
```rust
if a == i64::MIN && b == -1 {
    return Ok(NudgeVal::Int(0));
}
```

---

### PANIC-03: Integer Negation Overflow Panic on `-i64::MIN`
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2906` and `crates/nudgec/src/vm.rs:207`
- **Exact Code:**
```rust
// interpreter.rs:2906:
BinOp::Sub => match val {
    NudgeVal::Int(i) => Ok(NudgeVal::Int(-i)),
    ...
}

// vm.rs:207:
Op::Neg => {
    let val = self.stack.pop().ok_or("stack underflow")?;
    match val {
        NudgeVal::Int(i) => self.stack.push(NudgeVal::Int(-i)),
        ...
    }
}
```
- **Why It Is a Problem:**
Negating `i64::MIN` (`-i64::MIN`) panics in Rust with `attempt to negate with overflow` because `9223372036854775808` cannot fit in a signed 64-bit integer.
- **Minimal Fix:**
```rust
NudgeVal::Int(i) => {
    if let Some(neg) = i.checked_neg() {
        Ok(NudgeVal::Int(neg))
    } else {
        Err("integer negation overflow".into())
    }
}
```

---

### PANIC-04: `math.abs` Overflow Panic on `i64::MIN`
- **Severity:** High
- **File and Line:** `crates/nudge-runtime/src/stdlib/math.rs:123` and `crates/nudgec/src/eval/interpreter.rs:2673`
- **Exact Code:**
```rust
// math.rs:123:
Value::Int(v) => Ok(Value::Int(v.abs()))

// interpreter.rs:2673:
Some(NudgeVal::Int(i)) => Ok(NudgeVal::Int(i.abs()))
```
- **Why It Is a Problem:**
Calling `.abs()` on `i64::MIN` panics with `attempt to negate with overflow`. A Nudge program calling `math.abs(-9223372036854775808)` will crash the runtime process.
- **Minimal Fix:**
```rust
Value::Int(v) => match v.checked_abs() {
    Some(val) => Ok(Value::Int(val)),
    None => Err("math.abs: integer overflow on i64::MIN".into()),
}
```

---

### PANIC-05: `math.pow` Panic on Integer Overflow
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2705`
- **Exact Code:**
```rust
(Some(NudgeVal::Int(a)), Some(NudgeVal::Int(b))) if *b >= 0 => {
    Ok(NudgeVal::Int(a.pow(*b as u32)))
}
```
- **Why It Is a Problem:**
Unlike `crates/nudge-runtime/src/stdlib/math.rs:179` which safely uses `checked_pow` with fallback to float, `interpreter.rs` uses standard `a.pow(...)`. Any exponentiation that overflows `i64` panics in debug mode. Furthermore, `*b as u32` truncates without check if `b > u32::MAX as i64`.
- **Minimal Fix:**
```rust
(Some(NudgeVal::Int(a)), Some(NudgeVal::Int(b))) if *b >= 0 => {
    if let Ok(exp) = u32::try_from(*b) {
        if let Some(v) = a.checked_pow(exp) {
            Ok(NudgeVal::Int(v))
        } else {
            Ok(NudgeVal::Float((*a as f64).powf(*b as f64)))
        }
    } else {
        Ok(NudgeVal::Float((*a as f64).powf(*b as f64)))
    }
}
```

---

### PANIC-06: `time.sleep` Panic on Non-Finite or Out-of-Range Floats
- **Severity:** High
- **File and Line:** `crates/nudge-runtime/src/stdlib/time.rs:177`
- **Exact Code:**
```rust
if ms < 0.0 {
    return Err("time.sleep: duration cannot be negative".to_string());
}
let duration = Duration::from_secs_f64(ms / 1000.0);
thread::sleep(duration);
```
- **Why It Is a Problem:**
`Duration::from_secs_f64` panics if the input float is NaN, infinity, or exceeds `u64::MAX` seconds. Because floating point division by zero (`1.0 / 0.0`) produces `f64::INFINITY` in Nudge, calling `time.sleep(1.0 / 0.0)` bypasses `ms < 0.0` and panics.
- **Minimal Fix:**
```rust
if !ms.is_finite() || ms < 0.0 || (ms / 1000.0) > (u64::MAX as f64) {
    return Err("time.sleep: duration must be finite, non-negative, and fit in u64 seconds".to_string());
}
```

---

### PANIC-07: `str.repeat` Capacity Overflow Panic
- **Severity:** Medium
- **File and Line:** `crates/nudge-runtime/src/stdlib/string.rs:234`
- **Exact Code:**
```rust
Ok(Value::String(s.repeat(*count as usize)))
```
- **Why It Is a Problem:**
`String::repeat` panics on capacity overflow when the allocated buffer exceeds memory limits (for example, `str.repeat("a", i64::MAX)`).
- **Minimal Fix:**
```rust
let count_usize = usize::try_from(*count).map_err(|_| "str.repeat: count too large".to_string())?;
if s.len().checked_mul(count_usize).map_or(true, |sz| sz > 64 * 1024 * 1024) {
    return Err("str.repeat: result exceeds maximum allowed size (64MB)".to_string());
}
Ok(Value::String(s.repeat(count_usize)))
```

---

### PANIC-08: Empty Token Vector Panic in `parser::parse`
- **Severity:** Medium
- **File and Line:** `crates/nudgec/src/parser.rs:69, 75`
- **Exact Code:**
```rust
fn peek(&self) -> &Tok {
    &self.t[self.i].tok
}
fn pos(&self) -> usize {
    self.t[self.i].start
}
```
- **Why It Is a Problem:**
If `parser::parse` is invoked with an empty vector of spanned tokens (`tokens.is_empty()`), `self.t[self.i]` directly indexes out of bounds and panics.
- **Minimal Fix:**
```rust
fn peek(&self) -> &Tok {
    self.t.get(self.i).map(|s| &s.tok).unwrap_or(&Tok::Eof)
}
fn pos(&self) -> usize {
    self.t.get(self.i).map(|s| s.start).unwrap_or(0)
}
```

---

## 2. Edge-Case and Correctness Bugs

### EDGE-01: Text Input Cursor Reset to End on Every Edit
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:343`
- **Exact Code:**
```rust
let st = app.ensure_input_state(target_id);
st.text = new_value.to_string();
st.cursor = st.char_len();
st.selection_anchor = None;
```
- **Why It Is a Problem:**
`dispatch_type_headless`, `dispatch_backspace_headless`, and `dispatch_delete_headless` all forward to `dispatch_change_headless`. In `dispatch_change_headless`, `st.cursor` is unconditionally reset to `st.char_len()`. Editing or deleting text anywhere other than the end of the input forces the cursor to jump to the end of the buffer on every keystroke.
- **Minimal Fix:**
```rust
let st = app.ensure_input_state(target_id);
st.text = new_value.to_string();
st.cursor = st.cursor.min(st.char_len());
st.selection_anchor = None;
```

---

### EDGE-02: Argument Inversion in `str.join` Between Runtime and Interpreter
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2550-2563` vs `crates/nudge-runtime/src/stdlib/string.rs:106-114`
- **Exact Code:**
```rust
// interpreter.rs:2550:
"str.join" => {
    let sep = args.first()...unwrap_or("");
    let list = args.get(1)...ok_or("'str.join' requires a list as second argument")?;
...
// string.rs:110:
"str.join" => {
    match (&args[0], &args[1]) {
        (Value::List(list), Value::String(sep)) => ...
```
- **Why It Is a Problem:**
In `nudge-runtime`, `str.join` expects `(list, separator)`. In `interpreter.rs`, `str.join` expects `(separator, list)`. A program calling `str.join` with the documented arguments fails in one engine while passing in the other.
- **Minimal Fix:** Update `interpreter.rs` to accept `(list, separator)` matching `nudge-runtime`.

---

### EDGE-03: `str.index_of` Byte Offset vs Character Index Desynchronization
- **Severity:** Medium
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:2669` vs `crates/nudge-runtime/src/stdlib/string.rs:205-207`
- **Exact Code:**
```rust
// interpreter.rs:2669:
let idx = s.find(pat).map(|i| i as i64).unwrap_or(-1);

// string.rs:205:
Some(byte_idx) => {
    let char_idx = s[..byte_idx].chars().count() as i64;
    Ok(Value::Int(char_idx))
}
```
- **Why It Is a Problem:**
`s.find(pat)` returns a raw byte index. For multibyte strings (for example, `"🦀x"`), `interpreter.rs` returns byte offset 4 instead of character index 1, breaking compatibility with `str.substring` and `nudge-runtime`.
- **Minimal Fix:**
```rust
let idx = s.find(pat).map(|b| s[..b].chars().count() as i64).unwrap_or(-1);
```

---

### EDGE-04: Multiline Text Input Selection and Click Coordinate Inaccuracy
- **Severity:** Medium
- **File and Line:** `crates/nudge-runtime/src/ui_window.rs:584` and `crates/nudge-runtime/src/ui_window.rs:706`
- **Exact Code:**
```rust
let sel_x = rx + 8 + (sel_start as i32) * 16;
let sel_w = ((sel_end - sel_start) as i32) * 16;
...
let rel_x = cx - (lbox.rect.x + 8);
let char_idx = (rel_x.max(0) / 16) as usize;
```
- **Why It Is a Problem:**
`sel_start` is a character offset across the entire multiline string. Computing `sel_x` directly from `sel_start * 16` projects all lines onto the first line. Likewise, mouse click character calculation ignores the vertical line coordinate `cy`.
- **Minimal Fix:** Use `st.line_and_col_of(sel_start)` to calculate the appropriate line offset and row position.

---

### EDGE-05: Non-ASCII Character Glyph Mapping
- **Severity:** Low
- **File and Line:** `crates/nudge-runtime/src/ui_window.rs:127`
- **Exact Code:**
```rust
let code = (ch as usize).min(127);
let glyph = FONT_8X8_BASIC[code];
```
- **Why It Is a Problem:**
All unicode characters above 127 are clamped to index 127 (the DEL control code / delete symbol in 8x8 bitmap), rendering visual garbage for non-ASCII text without warning.
- **Minimal Fix:** Check `if (ch as usize) < 128 { FONT_8X8_BASIC[ch as usize] } else { FONT_8X8_BASIC[b'?' as usize] }`.

---

### EDGE-06: CRLF Line Ending Column Drift in `ui.rs`
- **Severity:** Low
- **File and Line:** `crates/nudge-runtime/src/ui.rs:284, 317`
- **Exact Code:**
```rust
pub fn line_and_col_of(&self, char_idx: usize) -> (usize, usize) {
    ...
    for c in self.text.chars().take(char_idx) {
        if c == '\n' { line += 1; col = 0; } else { col += 1; }
    }
}
pub fn line_len(&self, line_idx: usize) -> usize {
    self.text.split('\n').nth(line_idx).map(|l| l.chars().count()).unwrap_or(0)
}
```
- **Why It Is a Problem:**
Splitting on `\n` retains `\r` on Windows CRLF line endings, incrementing the column count by 1 for carriage returns and creating cursor position mismatch.
- **Minimal Fix:** Filter out `\r` when counting columns and strip trailing `\r` in `line_len`.

---

## 3. Resource Leaks and Blocking

### LEAK-01: Unreaped Zombie Processes and Leaked Reader Threads on `proc.spawn`
- **Severity:** High
- **File and Line:** `crates/nudge-runtime/src/proc.rs:308, 316, 337`
- **Exact Code:**
```rust
let stdout_handle = std::thread::spawn(move || {
    let mut buf = Vec::new();
    if let Some(mut r) = stdout_pipe.take() {
        let _ = r.read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).to_string()
});
...
let managed = ManagedChild { ... };
with_registry(|reg| {
    reg.insert(handle_id.clone(), managed);
});
```
- **Why It Is a Problem:**
`ManagedChild` has no `Drop` implementation. If a Nudge script executes `proc.spawn` without subsequently calling `proc.await` or `proc.kill`, child processes are never reaped, remaining as zombies or orphaned processes on the host. Furthermore, the spawned stdout and stderr reader threads block indefinitely on their pipe read loops, and entries in `PROCESS_REGISTRY` accumulate without bound.
- **Minimal Fix:** Implement `Drop for ManagedChild` to terminate and wait on unclosed child processes, joining background pipe threads. Add an automatic reaping pass during interpreter or task shutdown.

---

### LEAK-02: Unbounded Reader Memory in Process Execution
- **Severity:** High
- **File and Line:** `crates/nudge-runtime/src/proc.rs:311, 319`
- **Exact Code:**
```rust
if let Some(mut r) = stdout_pipe.take() {
    let _ = r.read_to_end(&mut buf);
}
```
- **Why It Is a Problem:**
`r.read_to_end(&mut buf)` has no size ceiling. A child process generating gigabytes of continuous output will allocate until host memory exhaustion (OOM).
- **Minimal Fix:**
```rust
const MAX_PROC_OUTPUT_BYTES: u64 = 16 * 1024 * 1024; // 16 MB
r.take(MAX_PROC_OUTPUT_BYTES).read_to_end(&mut buf);
```

---

### LEAK-03: Permanent Memory Growth in Global `TASK_REGISTRY`
- **Severity:** Medium
- **File and Line:** `crates/nudge-runtime/src/task.rs:508-512`
- **Exact Code:**
```rust
pub fn task_register(token: TaskToken) {
    with_registry(|reg| {
        reg.insert(token.id.as_str().to_string(), token);
    });
}
```
- **Why It Is a Problem:**
Every task spawned via `task_spawn` is stored in the static `TASK_REGISTRY`. Because `TaskToken` holds an `Arc<TaskInner>` with child references, and no cleanup or removal routine is ever called on task completion or cancellation, all task records and closures are leaked permanently in memory for the life of the process.
- **Minimal Fix:** Add a removal or pruning method `task_unregister(id: &str)` called when a task transitions to terminal state, or use a weak reference map.

---

### LEAK-04: Indefinite Blocking Without Timeout or Cancellation in `McpSession::send_request`
- **Severity:** Medium
- **File and Line:** `crates/nudge-runtime/src/mcp.rs:254-267`
- **Exact Code:**
```rust
loop {
    let mut line = String::new();
    let bytes_read = self.reader.read_line(&mut line).map_err(...)?;
    if bytes_read == 0 {
        return Err(format!("MCP server '{}' closed the pipe during '{method}'", self.server_name));
    }
    ...
}
```
- **Why It Is a Problem:**
`read_line` contains no read timeout and never checks `crate::task::is_current_cancelled()`. If an MCP server hangs or sends non-matching response IDs, the host runtime thread blocks permanently and cannot be cancelled.
- **Minimal Fix:** Add `if crate::task::is_current_cancelled() { return Err("error[E-task-0001]: TaskCancelled".into()); }` inside the loop and set a read deadline.

---

### LEAK-05: Unbounded Host Stack Recursion in Interpreter
- **Severity:** High
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:1984`
- **Exact Code:**
```rust
fn call_user_fn(
    &mut self,
    fn_item: &Item,
    agent_name: Option<&str>,
    args: &[Expr],
    kwargs: &[(String, Expr)],
    env: &Rc<RefCell<Env>>,
) -> Result<NudgeVal, String>
```
- **Why It Is a Problem:**
Unlike the bytecode VM which bounds recursion using `MAX_CALL_FRAMES = 10_000` on a heap stack, `call_user_fn` in the AST interpreter recurses on the native OS thread stack without any depth counter. Pathological recursion crashes the process via stack overflow (SIGSEGV).
- **Minimal Fix:** Maintain a `call_depth: usize` field on `Interpreter`, returning `Err("call stack overflow: recursion limit exceeded".into())` when `call_depth > 1_000`.

---

## 4. Error Handling Issues

### ERR-01: Silently Swallowed Trace Emission Failure in `eval_stream_llm`
- **Severity:** Medium
- **File and Line:** `crates/nudgec/src/eval/interpreter.rs:3191`
- **Exact Code:**
```rust
let _ = nudge_runtime::trace::emit(record);
```
- **Why It Is a Problem:**
In `eval_stream_llm`, any error from `trace::emit` (such as disk full or write failure) is discarded with `let _ =`, unlike all other call sites which strictly propagate trace errors via `?`. This allows streaming calls to silently fail trace persistence and corrupt replay traces.
- **Minimal Fix:**
```rust
nudge_runtime::trace::emit(record)
    .map_err(|e| format!("error[E-native-0303]: trace write failed: {e}"))?;
```

---

### ERR-02: Mutex Lock Poisoning Panics in Runtime Globals
- **Severity:** Medium
- **File and Line:** `crates/nudge-runtime/src/store.rs:290, 299` and `crates/nudge-runtime/src/task.rs:161, 181, 188`
- **Exact Code:**
```rust
let mut lock = GLOBAL_STORE.lock().unwrap();
let mut lock = TASK_REGISTRY.lock().unwrap();
```
- **Why It Is a Problem:**
Using `.lock().unwrap()` panics if a separate thread previously panicked while holding the mutex. An isolated error cascades into an unrecoverable process panic.
- **Minimal Fix:** Use `.lock().unwrap_or_else(|e| e.into_inner())` or map poisoning errors to `Result::Err`.
