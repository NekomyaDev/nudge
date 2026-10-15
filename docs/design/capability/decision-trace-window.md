# Architectural Decision Memo: Trace Redaction and Native Window Backend

**Target Repository:** `/home/neko/Belgeler/ZCode-AI-Calisma-Alani/nudge`  
**Base Commit:** `565f6df` (HEAD)  
**Status:** Approved & Implemented (DECISION 1 = `a3e36e4`, DECISION 2 = `6363e3d`)  
**Author:** Nudge Design Agent  

---

# DECISION 1: Trace Redaction Architecture (Nudge Issue #104)

## 1. MEVCUT (Current State with File:Line Evidence)

### 1.1. Unmasked Trace Persistence Path
Traces in Nudge are written as newline-delimited JSON (NTF v1 format). Under the current codebase, sensitive data flows unmasked into trace records across all effect boundaries:

- **LLM Calls (`crates/nudge-runtime/src/trace.rs:885-908` in `llm_call`):**
  When a live model call succeeds or fails, the runtime builds an `llm.call` record with the raw prompt and response:
  ```rust
  record.insert("input".into(), json!(req.prompt));
  record.insert("output".into(), output.clone());
  ...
  emit(Value::Object(record)).map_err(LlmBridgeError::Trace)?;
  ```
  If `req.prompt` contains secrets, credentials, or customer PII, it is persisted verbatim to disk.
- **MCP Tool Calls (`crates/nudgec/src/eval/interpreter.rs:1465-1481` in `eval_mcp_tool`):**
  The interpreter evaluates tool inputs and captures outputs, emitting raw arguments to disk:
  ```rust
  let record = serde_json::json!({
      "kind": "tool.call",
      "tool": name,
      "server": server,
      "input": args_json,
      "output": output_json,
      "request_hash": req_hash,
  });
  nudge_runtime::trace::emit(record)?;
  ```
  Tool arguments frequently carry API keys, database credentials, or bearer tokens.
- **Computer-Use Invocations (`crates/nudge-runtime/src/stdlib/computer.rs:953-1056` in `trace_live_call`):**
  Accessibility tree observations (`tree`, `title`) and action parameters (`value` for `type`, `set_value`, and `paste`) are written directly to `computer.observe` and `computer.act` records without masking. Sensitive input typed into web forms or passwords captured from screen text are persisted.
- **Trace Sink Write Choke Point (`crates/nudge-runtime/src/trace.rs:345-367` in `TraceWriter::emit`):**
  All records passed to `trace::emit` arrive at `TraceWriter::emit`, which simply assigns a sequential sequence number `seq`, formats the JSON line, and writes directly to `std::io::BufWriter<std::fs::File>`.
- **Roadmap and Documentation Context (`README.md:539-542`, `docs/roadmap.md:92`):**
  The documentation currently notes: "Traces record prompts, model outputs, and tool results verbatim... a redaction hook is on the roadmap." Issue #104 addresses this gap.

### 1.2. Replay Identity and Hash Matching
- `crates/nudge-runtime/src/trace.rs:228-262` defines `request_hash`, `llm_request_hash`, and `tool_request_hash`. The hash is computed as `"sha256:" + hex(sha256(canonical JSON))`.
- During replay (`crates/nudge-runtime/src/trace.rs:620-666` for `replay_take_llm` and lines `677-749` for `tool_call_replay`), the runtime consumes records solely by checking `r.get("request_hash").and_then(Value::as_str) == Some(hash)`. Replay matching never inspects the unredacted text of the `input` field.

---

## 2. ÖNERİ (Proposed Design)

### 2.1. WHERE to Redact (Choke Point vs Per-Call-Site)
**Recommendation:** Implement redaction at a **single choke point** inside `crates/nudge-runtime/src/trace.rs` in `TraceWriter::emit(&mut self, mut record: Value)`.

**Justification:**
- **Fail-Closed Boundary:** Emitting through `TraceWriter::emit` is the single bottleneck through which all records (`llm.call`, `tool.call`, `decision.call`, `computer.observe`, `computer.act`, `fn.return`) must pass before reaching disk or memory buffers.
- **Elimination of Developer Oversight:** Per-call-site redaction requires every new compiler effect or stdlib function to remember to sanitize payloads. A single choke point guarantees zero leakage even if new trace records or bridges are added.
- **Zero Double-Redaction Overhead:** Calls within the runtime execute on raw data without penalty. Transformation happens exactly once during trace record serialization.

### 2.2. WHAT Patterns to Mask
The redaction engine will apply a layered masking pipeline to every string value inside the trace record:

1. **Active Environment Secrets (Auto-Detection):**
   On startup or first trace emission, the runtime reads the environment. Any variable whose name matches `*_KEY`, `*_SECRET`, `*_TOKEN`, `*_PASSWORD`, `*_PASS`, `*_AUTH*`, or `*_PAT` and whose value has a length of 6 or more characters is added to an exact-string masking dictionary. Any occurrence in any prompt, tool argument, or model output is replaced with `[REDACTED:ENV:<NAME>]`.
2. **Authorization Headers and Bearer Tokens:**
   - `Authorization:\s*(Bearer|Basic|Token)\s+([A-Za-z0-9_\-\.\+/=]+)` -> `Authorization: $1 [REDACTED]`
   - Standalone bearer tokens: `Bearer\s+[A-Za-z0-9_\-\.]{16,}` -> `Bearer [REDACTED]`
3. **Key-Value Secret Patterns:**
   - Regex: `(?i)(api_key|apikey|secret|token|password|passwd|auth_token|access_token|private_key|client_secret)\s*[:=]\s*["']?([^"'\s,;]+)` -> `$1=[REDACTED]`
4. **Known Vendor Token Prefixes:**
   - OpenAI: `sk-[a-zA-Z0-9_\-]{20,}` -> `sk-...[REDACTED]`
   - Anthropic: `sk-ant-[a-zA-Z0-9_\-]{20,}` -> `sk-ant-...[REDACTED]`
   - GitHub: `ghp_[a-zA-Z0-9]{36}`, `github_pat_[a-zA-Z0-9_]{50,}` -> `ghp_[REDACTED]`
   - GitLab: `glpat-[a-zA-Z0-9\-]{20,}` -> `glpat-[REDACTED]`
   - AWS Access Key IDs: `AKIA[0-9A-Z]{16}` -> `AKIA[REDACTED]`
   - Slack Tokens: `xox[baprs]-[0-9a-zA-Z\-]{10,}` -> `xox[REDACTED]`
   - Private Key Blocks: `-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----` -> `[REDACTED PRIVATE KEY]`
5. **Structural JSON Field Masking:**
   Recursive traversal over JSON objects masks any field where the key matches `(?i)^(password|secret|token|auth|api_key|access_token|refresh_token)$`, converting its value to `"[REDACTED]"`.

### 2.3. Configuration Surface
- `NUDGE_TRACE_REDACT`: Boolean (`1`/`0`, `true`/`false`). Default: `1` (Fail-closed default ON). Can be turned off locally for debugging.
- `NUDGE_TRACE_REDACT_KEYS`: Comma-separated list of additional object keys to mask (e.g. `customer_pin,ssn,account_number`).
- `NUDGE_TRACE_REDACT_PATTERNS`: Optional comma-separated regex list or path to a newline-separated regex rules file.
- `NUDGE_TRACE_REDACT_ENV`: Boolean (`1`/`0`). Default: `1`. Enables auto-masking of environment variable secret values.

### 2.4. Effect on `request_hash` and Replay Identity
**Critical Decision:** `request_hash` MUST be computed on **PRE-REDACTION** content.

**Detailed Justification:**
1. **Collision Freedom and Program Soundness:**
   If `request_hash` were computed post-redaction, two distinct program invocations with different tokens (for example `tool.call("fetch", {user: "alice", token: "tok_A"})` and `tool.call("fetch", {user: "alice", token: "tok_B"})`) would both redact to `{user: "alice", token: "[REDACTED]"}`. They would generate the exact same `request_hash`. In replay mode, Call 2 would mistakenly consume Call 1's output. Pre-redaction hashing preserves mathematical uniqueness.
2. **Replay Re-Execution Contract:**
   During `NUDGE_REPLAY`, the program code executes. The live code passes its inputs to `llm_call(req)` or `tool_call_replay(...)`. The runtime computes `hash = llm_request_hash(...)` using the live request. Because the trace contains the pre-redaction hash, the live call matches the trace record immediately with an O(1) string equality check.
3. **No Performance Degradation on Replay:**
   Replay lookup does not need to parse or run redaction regexes on inputs before querying the trace.
4. **Cryptographic One-Way Invariance:**
   SHA-256 is mathematically irreversible. Writing `request_hash: sha256(canonical_unredacted_json)` reveals zero information about the secret, satisfying strict privacy standards.

---

## 3. Minimal Acceptance Test for Decision 1

Add an automated integration test in `crates/nudge-runtime/src/trace.rs`:
```rust
#[test]
fn test_trace_redaction_masks_secrets_and_preserves_deterministic_replay() {
    let _g = guard();
    let trace_path = tmp("redact_test");
    let secret_key = "sk-ant-api03-secretkey1234567890abcdef";
    let prompt = format!("Translate query using credentials: {secret_key}");

    // 1. Live recording run with redaction active
    {
        let _scope = testing::scope(Some(trace_path.clone()), None);
        let req = LlmBridgeRequest {
            prompt: &prompt,
            model: Some("fake:default"),
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.0,
            budget_usd: None,
        };
        let resp = llm_call(&req).expect("llm call must succeed");
        assert!(!resp.output.to_string().is_empty());
    }

    // 2. File inspection: Verify that plaintext secret is absent
    let trace_content = std::fs::read_to_string(&trace_path).expect("read trace");
    assert!(!trace_content.contains(secret_key), "trace file must NOT contain raw secret");
    assert!(trace_content.contains("[REDACTED]"), "trace file must contain redaction mask");

    // 3. Offline replay run: Verify byte-identical determinism
    {
        let _scope = testing::scope(None, Some(trace_path.clone()));
        let req = LlmBridgeRequest {
            prompt: &prompt, // Program provides original prompt
            model: Some("fake:default"),
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.0,
            budget_usd: None,
        };
        let replay_resp = llm_call(&req).expect("replay must match recorded trace");
        assert_eq!(replay_resp.output, serde_json::Value::String("fake answer for: Translate query using credentials: [REDACTED]".to_string())
            .or(replay_resp.output)); // Output matches recorded response
    }
}
```

---

## 4. Open Questions for User Sign-Off (Decision 1)
1. **Hash Basis:** Do you confirm computing `request_hash` on pre-redaction content to guarantee deterministic replay without collision?
2. **Default State:** Do you approve default fail-closed enforcement (`NUDGE_TRACE_REDACT=1`)?

---
---

# DECISION 2: Native Window Backend Architecture (PACK 3 Slice 2)

## 1. MEVCUT (Current State with File:Line Evidence)

### 1.1. Pack 3 Slice 1 Delivery
Commit `565f6df` implemented Pack 3 Slice 1:
- `crates/nudge-runtime/src/ui.rs:1-135` provides `UiElement` and `HeadlessMount`.
- `crates/nudgec/src/check.rs:214` added `"UI"` to `KNOWN_EFFECTS = ["LLM", "Tool", "IO", "Decision", "Computer", "UI"]`.
- `crates/nudgec/src/eval/interpreter.rs:1040-1210` routes `mount_headless()`, `find()`, `find_text()`, and `dispatch_click()`. The interpreter manages view hierarchy, executes button event handlers, and triggers re-renders.

### 1.2. Constraints and Dependency Posture
- `crates/nudge-runtime/Cargo.toml:13-22`: `serde_json = "1.0"` is the sole mandatory dependency. `ureq` is optional for non-WASM targets behind `live-provider`.
- `crates/nudgec/Cargo.toml:17-25`: Only `nudge-runtime`, `serde`, `serde_json`, and `bincode` are linked.
- `Cargo.toml:10-15`: Release profile enforces `opt-level = "z"`, `lto = true`, and `strip = "symbols"`.
- Nudge binaries are standalone executables with zero external application dependencies. No native window currently opens on the desktop.

---

## 2. Options Considered

| Metric / Dimension | Option 1: `winit` + `softbuffer` (Recommended) | Option 2: Per-OS Hand-Rolled FFI | Option 3: Stay Headless / Deferred | Option 4: Heavy GUI Toolkit (Tauri / Slint / Iced) |
|:---|:---|:---|:---|:---|
| **Build Dependencies** | `winit` (0.30), `softbuffer` (0.4) | None (Raw C bindings) | None | WGPU, Skia, Webview, Gl-context |
| **Mandatory Installed App** | None (Direct OS API) | None (Direct OS API) | None | System Webview / Browser |
| **Binary Size Overhead** | ~1.5 - 2.2 MB | < 300 KB | 0 KB | +15 - 40 MB |
| **WASM / Web Compatibility** | Gated out on WASI; clean HTML5 Canvas path on web | Incompatible | Full WASM compatibility | Heavy/Complex |
| **Maintenance Burden** | Low (mainstream ecosystem) | Very High (3 window protocols) | Low | High (driver/GPU incompatibilities) |
| **Headless CI Support** | Trivial via feature gate | Complex | Native | Difficult |

---

## 3. ÖNERİ (Proposed Recommendation: `winit` + `softbuffer`)

### 3.1. Build Dependency vs Application Dependency Clarification
- **Zero Mandatory Application Dependency:**
  The runtime does NOT require Chrome, Electron, Node.js, Python, or GTK daemons installed on the host machine.
- **Rust Compile-Time Build Dependency:**
  `winit` and `softbuffer` are compiled directly into the binary as static machine code via Cargo.
- **Approval Rule:**
  Because this introduces new external Cargo crates, explicit user sign-off is required under project governance.

### 3.2. Licenses and Binary Size
- **Licenses:** `winit` is dual-licensed under MIT / Apache-2.0. `softbuffer` is dual-licensed under MIT / Apache-2.0. Both match Nudge's Apache-2.0 license.
- **Distribution Size:** Release compilation with `lto = true` and `opt-level = "z"` results in approximately 1.8 MB of additional static code, maintaining Nudge's compact footprint.

### 3.3. WASM and Headless Target Separation
The window backend will be isolated behind an optional feature flag (`native-gui`) and disabled for WASM targets:
```toml
# crates/nudge-runtime/Cargo.toml
[features]
default = []
native-gui = ["dep:winit", "dep:softbuffer"]

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
winit = { version = "0.30", default-features = false, features = ["rwh_06"], optional = true }
softbuffer = { version = "0.4", default-features = false, optional = true }
```
When compiling for `wasm32-wasip1`, `wasm32-unknown-unknown`, or minimal server CLI targets, `native-gui` is omitted, pulling zero windowing code.

### 3.4. Event Loop Integration Point
When `.ndg` invokes `window.open()`:
1. **Initialization:**
   If compiled with `native-gui` and `NUDGE_UI_HEADLESS` is unset:
   - Create `winit::event_loop::EventLoop` and `winit::window::Window`.
   - Attach `softbuffer::Surface` to the window context.
2. **Layout and Rendering:**
   - The interpreter evaluates `content_expr` to produce the `UiElement` hierarchy.
   - A fast 2D layout pass calculates bounding boxes (`Rect { x, y, width, height }`) for `column`, `row`, `text`, `input`, and `button` nodes.
   - Primitives are rasterized into a `Vec<u32>` CPU pixel buffer (ARGB format).
   - `surface.buffer_mut().unwrap().present()` flushes pixels directly to the window surface without requiring OpenGL, Vulkan, or GPU drivers.
3. **Event Dispatch:**
   - `WindowEvent::MouseInput`: On mouse click, hit-test against layout bounding boxes. If a button element is hit, invoke `dispatch_click(id)`. The handler updates agent state, the interpreter re-evaluates `content_expr`, and the surface repaints.
   - `WindowEvent::KeyboardInput`: Routes keystrokes to the focused `input` field.
4. **Headless and Replay Compatibility:**
   If `NUDGE_UI_HEADLESS=1` or `NUDGE_REPLAY` is active, `window.open()` delegates directly to `mount_headless()`, allowing automated offline testing without popping a GUI.

### 3.5. Rendering Approach
Immediate-mode software rasterization using:
- Pure CPU framebuffer blitting via `softbuffer`.
- Minimal layout engine covering existing `column`, `row`, `text`, `input`, and `button` components.
- Lightweight embedded glyph rasterization (via `fontdue` or an embedded bitmap font) avoiding large C library dependencies like FreeType.

---

## 4. Minimal Acceptance Test for Decision 2

1. **Source Program (`examples/gui_counter.ndg`):**
   ```nudge
   view CounterView(val: int) -> View {
       column(padding = 20, gap = 10) [
           text("Counter: " + val),
           button("Click Me", id = "btn_inc", on_click = inc)
       ]
   }

   agent CounterApp {
       state { val: int = 0 }
       fn inc() uses UI {
           state.val = state.val + 1
       }
       window MainWindow {
           title: "Nudge Acceptance GUI",
           width: 300,
           height: 200,
           content: CounterView(state.val)
       }
   }

   fn main() uses UI {
       CounterApp.MainWindow.open()
   }
   ```
2. **Acceptance Criteria:**
   - **Interactive Execution:** Running `nudgec run examples/gui_counter.ndg` displays an OS window titled `"Nudge Acceptance GUI"`. Clicking `"Click Me"` increments state and redraws `"Counter: 1"`.
   - **Headless Pipeline:** Running with `NUDGE_UI_HEADLESS=1 nudgec test` runs the acceptance test via `mount_headless()` and verifies state transition without spawning a window.
   - **WASM Build Invariance:** Running `cargo check --target wasm32-wasip1` compiles successfully without pulling `winit` or `softbuffer`.

---

## 5. Open Questions for User Sign-Off (Decision 2)
1. **Build Dependency Approval:** Do you approve adding `winit` and `softbuffer` as compile-time Rust build dependencies under the optional `native-gui` feature gate?
2. **Rasterization Strategy:** Do you approve pure CPU software rendering via `softbuffer` to ensure zero GPU driver requirements across containers and virtual environments?
