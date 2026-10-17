//! NTF v1 trace writing + deterministic LLM replay (native).
//!
//! Byte-parity target is the Python reference runtime
//! (`runtime/nudge_runtime/__init__.py`):
//!
//! * **Trace writing** (`NUDGE_TRACE`, default `trace.jsonl`): every LLM
//!   call appends one `llm.call` record — `{"v": 1, "seq": n, ...}` with a
//!   gapless 1-based `seq` (docs/ntf-spec.md, frozen v1).
//! * **LLM replay** (`NUDGE_REPLAY=<trace.jsonl>`): the bridge serves the
//!   recorded `llm.call` outputs — deterministic, $0, offline. It NEVER
//!   touches a live provider: no match is a loud `ReplayMismatch`, never a
//!   silent network call.
//!
//! Replay identity is the Python runtime's `request_hash`
//! (`_llm_request_hash`): `"sha256:" + sha256(canonical JSON)` over
//! `{"op": "llm", "model", "input", "schema", "params": {"temperature": 0},
//! "images"}` — prompt + model + schema + params. Records without
//! `request_hash` (legacy traces) replay in global record order, exactly
//! like the Python runtime.

use crate::provider;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// ── SHA-256 (FIPS 180-4) — request_hash parity needs it and the crate is
//    std-only otherwise ──────────────────────────────────────────────

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = sha256(data);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

// ── JSON serialization ──────────────────────────────────────────────
//
// Two modes over the same writer, both matching the Python `json` module:
//
// * `request_hash` canonical form: sorted keys, tight separators, and
//   `ensure_ascii=True` escapes (`\u00e9`, surrogate pairs for astral
//   chars) — the exact bytes Python hashes.
// * trace lines: Python's default separators (`, ` / `: `) with
//   `ensure_ascii=False` (UTF-8 passes through verbatim).

fn write_json(v: &Value, ascii: bool, sep: (&str, &str), out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                out.push_str(&n.to_string());
            } else {
                out.push_str(&py_float(n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => write_json_str(s, ascii, out),
        Value::Array(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep.0);
                }
                write_json(x, ascii, sep, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            // serde_json::Map is a BTreeMap here: iteration is already in
            // sorted-key order, which is what the canonical form needs
            out.push('{');
            for (i, (k, x)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep.0);
                }
                write_json_str(k, ascii, out);
                out.push_str(sep.1);
                write_json(x, ascii, sep, out);
            }
            out.push('}');
        }
    }
}

fn write_json_str(s: &str, ascii: bool, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if ascii && (c as u32) >= 0x80 => {
                let cp = c as u32;
                if cp > 0xFFFF {
                    let t = cp - 0x10000;
                    out.push_str(&format!(
                        "\\u{:04x}\\u{:04x}",
                        0xD800 + (t >> 10),
                        0xDC00 + (t & 0x3FF)
                    ));
                } else {
                    out.push_str(&format!("\\u{cp:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python `repr(float)` — shortest round-trip with a forced `.0` on
/// integral values and a two-digit signed exponent (`1e+20`, `1e-07`).
fn py_float(f: f64) -> String {
    let s = format!("{f:?}");
    match s.find('e') {
        Some(pos) => {
            let (mant, exp) = s.split_at(pos);
            let n: i32 = exp[1..].parse().unwrap_or(0);
            format!("{mant}e{n:+03}")
        }
        None => s,
    }
}

/// The canonical `request_hash` payload bytes: Python's
/// `json.dumps(payload, sort_keys=True, separators=(",", ":"))` with the
/// default `ensure_ascii=True`.
fn canonical_bytes(v: &Value) -> Vec<u8> {
    let mut out = String::new();
    write_json(v, true, (",", ":"), &mut out);
    out.into_bytes()
}

/// Trace-line bytes: Python's `json.dumps(line, ensure_ascii=False)` —
/// default separators, UTF-8 passes through verbatim.
fn line_bytes(v: &Value) -> String {
    let mut out = String::new();
    write_json(v, false, (", ", ": "), &mut out);
    out
}

// ── request_hash (additive NTF field, replay identity) ───────────────

/// sha256 over the canonical request JSON — parity with Python's
/// `_request_hash` (`"sha256:" + hexdigest`).
pub fn request_hash(payload: &Value) -> String {
    format!("sha256:{}", sha256_hex(&canonical_bytes(payload)))
}

/// LLM call identity — parity with Python's `_llm_request_hash`: prompt +
/// model + schema + params + images. `model` is the raw model option
/// (None → `"default"`, like `model or "default"`).
pub fn llm_request_hash(
    model: Option<&str>,
    prompt: &str,
    schema: Option<&Value>,
    images: &[String],
) -> String {
    request_hash(&json!({
        "op": "llm",
        "model": model.unwrap_or("default"),
        "input": prompt,
        "schema": schema.cloned().unwrap_or(Value::Null),
        "params": {"temperature": 0},
        "images": images,
    }))
}

/// Tool call identity — parity with Python's `_tool_request_hash` and the
/// Rust interpreter's recording: `{"op": "tool", "tool", "server", "input"}`.
pub fn tool_request_hash(tool: &str, server: Option<&str>, input: &Value) -> String {
    request_hash(&json!({
        "op": "tool",
        "tool": tool,
        "server": server,
        "input": input,
    }))
}

// ── sink: NTF v1 trace writing ─────────────────────────────────────

static DEFAULT_SINK: AtomicBool = AtomicBool::new(false);
static SINK: Mutex<Option<TraceWriter>> = Mutex::new(None);

struct ThreadSinkFlusher;

impl Drop for ThreadSinkFlusher {
    fn drop(&mut self) {
        let _ = flush_sink();
    }
}

std::thread_local! {
    static SINK_FLUSHER: std::cell::RefCell<Option<ThreadSinkFlusher>> = const { std::cell::RefCell::new(None) };
}

/// Explicitly flush the global trace sink if one is currently active.
pub fn flush_sink() -> Result<(), String> {
    let mut guard = SINK.lock().map_err(|_| "trace sink lock poisoned")?;
    if let Some(w) = guard.as_mut() {
        w.flush()?;
    }
    Ok(())
}

/// Reset the global trace sink, flushing and closing any active writer.
pub fn reset_sink() -> Result<(), String> {
    let mut guard = SINK.lock().map_err(|_| "trace sink lock poisoned")?;
    *guard = None;
    Ok(())
}

/// Buffered writer for NTF v1 trace files.
///
/// Wraps an open appending `File` in `std::io::BufWriter` to batch JSONL line writes,
/// reducing filesystem syscall overhead during execution.
/// Flushes automatically on drop and exposes explicit `flush()` and `into_inner()` methods.
pub struct TraceWriter {
    path: PathBuf,
    writer: Option<std::io::BufWriter<std::fs::File>>,
    seq: u64,
}

pub type TraceSink = TraceWriter;

impl TraceWriter {
    /// Opens or creates the trace file at `path` for appending, seeded with the
    /// sequence counter from any existing records already on disk.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let seq = count_records(&path);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("cannot open trace '{}': {e}", path.display()))?;
        Ok(Self {
            path,
            writer: Some(std::io::BufWriter::with_capacity(64 * 1024, file)),
            seq,
        })
    }

    /// Alias for `open`.
    pub fn for_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        Self::open(path)
    }

    /// Path to the underlying trace file.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Highest sequence number written or seeded so far.
    pub fn current_seq(&self) -> u64 {
        self.seq
    }

    /// Append one NTF v1 record: `{"v": 1, "seq": n, ...record}` with a gapless
    /// 1-based `seq`. The record is buffered in memory.
    pub fn emit(&mut self, record: Value) -> Result<u64, String> {
        let Value::Object(mut fields) = record else {
            return Err("trace record must be a JSON object".into());
        };
        crate::redact::redact_record_fields(&mut fields);
        self.seq += 1;
        let seq = self.seq;
        let mut line = serde_json::Map::new();
        line.insert("v".into(), json!(1));
        line.insert("seq".into(), json!(seq));
        line.append(&mut fields);

        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| "trace writer already closed".to_string())?;

        use std::io::Write;
        writer
            .write_all(line_bytes(&Value::Object(line)).as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .map_err(|e| format!("cannot write trace '{}': {e}", self.path.display()))?;
        Ok(seq)
    }

    /// Explicitly flush any buffered trace records to disk.
    pub fn flush(&mut self) -> Result<(), String> {
        if let Some(w) = self.writer.as_mut() {
            use std::io::Write;
            w.flush()
                .map_err(|e| format!("cannot flush trace '{}': {e}", self.path.display()))?;
        }
        Ok(())
    }

    /// Flush all buffered records and unwrap the underlying `File`.
    pub fn into_inner(mut self) -> Result<std::fs::File, String> {
        let mut w = self
            .writer
            .take()
            .ok_or_else(|| "trace writer already closed".to_string())?;
        use std::io::Write;
        w.flush()
            .map_err(|e| format!("cannot flush trace '{}': {e}", self.path.display()))?;
        w.into_inner().map_err(|e| {
            format!(
                "cannot unwrap buffered trace writer '{}': {e}",
                self.path.display()
            )
        })
    }
}

impl Drop for TraceWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// `nudgec run` enables the default sink: traces land in `NUDGE_TRACE`
/// (default `trace.jsonl`) even when the env var is unset — the Python
/// runtime's default-on behavior.
pub fn enable_default_sink() {
    DEFAULT_SINK.store(true, Ordering::SeqCst);
}

/// Test isolation (thread-local, restored on drop): when a scope is
/// active THIS thread ignores `NUDGE_TRACE` / `NUDGE_REPLAY` entirely and
/// uses exactly the given paths (None = disabled).
struct Overrides {
    sink: Option<PathBuf>,
    replay: Option<PathBuf>,
}

std::thread_local! {
    static OVERRIDE: std::cell::RefCell<Option<Overrides>> = const { std::cell::RefCell::new(None) };
}

#[doc(hidden)]
pub mod testing {
    use super::{Overrides, OVERRIDE};
    use std::path::PathBuf;

    /// Shared lock for tests that mutate GLOBAL trace state — the global sink
    /// (`Scope::drop` calls `reset_sink()`), the gapless sequence counter, or the
    /// NUDGE_TRACE / NUDGE_REPLAY environment variables. `scope` isolation is
    /// thread-local, but emission/reset touch process-global state, so tests that
    /// touch either must be mutually exclusive. Without this they race under
    /// parallel test execution (observed as a 1-in-N flake in
    /// `trace::tests::bridge_records_llm_call_with_request_hash`, whose sibling
    /// computer-replay tests set NUDGE_REPLAY without any lock).
    pub static TEST_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Thread-local trace/replay isolation for tests: inside the scope the
    /// current thread uses exactly `sink` / `replay` (None = disabled) and
    /// ignores the environment. Restored on drop.
    pub struct Scope {
        prev: Option<Overrides>,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            OVERRIDE.with(|o| *o.borrow_mut() = self.prev.take());
            let _ = super::reset_sink();
        }
    }

    pub fn scope(sink: Option<PathBuf>, replay: Option<PathBuf>) -> Scope {
        OVERRIDE.with(|o| {
            let prev = o.borrow_mut().replace(Overrides { sink, replay });
            Scope { prev }
        })
    }
}

fn sink_path() -> Option<PathBuf> {
    OVERRIDE.with(|o| {
        if let Some(ov) = o.borrow().as_ref() {
            return ov.sink.clone();
        }
        match std::env::var("NUDGE_TRACE") {
            Ok(p) if !p.is_empty() => Some(PathBuf::from(p)),
            _ => {
                if DEFAULT_SINK.load(Ordering::SeqCst) {
                    Some(PathBuf::from("trace.jsonl"))
                } else {
                    None
                }
            }
        }
    })
}

/// Query the currently configured sink path (from thread override or environment).
pub fn current_sink_path() -> Option<PathBuf> {
    sink_path()
}

/// Query the currently configured replay path (from thread override or environment).
pub fn current_replay_path() -> Option<PathBuf> {
    replay_path()
}

/// Is trace writing active for the current call?
pub fn sink_active() -> bool {
    sink_path().is_some()
}

fn count_records(path: &std::path::Path) -> u64 {
    std::fs::read_to_string(path)
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count() as u64)
        .unwrap_or(0)
}

/// Append one NTF v1 record: `{"v": 1, "seq": n, ...record}` with a
/// gapless 1-based `seq` across the whole file (the counter is seeded
/// from the records already on disk, then kept in memory — parity with
/// the Python runtime's `_emit_trace`).
pub fn emit(record: Value) -> Result<(), String> {
    let Some(path) = sink_path() else {
        return Ok(());
    };
    SINK_FLUSHER.with(|f| {
        if f.borrow().is_none() {
            *f.borrow_mut() = Some(ThreadSinkFlusher);
        }
    });
    let mut guard = SINK.lock().map_err(|_| "trace sink lock poisoned")?;
    let needs_new = match guard.as_ref() {
        Some(w) => w.path() != path.as_path(),
        None => true,
    };
    if needs_new {
        let writer = TraceWriter::open(&path)?;
        *guard = Some(writer);
    }
    let writer = guard.as_mut().unwrap();
    let is_main_return = if let Value::Object(fields) = &record {
        fields.get("kind").and_then(Value::as_str) == Some("fn.return")
            && fields.get("fn").and_then(Value::as_str) == Some("main")
    } else {
        false
    };
    writer.emit(record)?;
    if is_main_return {
        let _ = writer.flush();
    }
    Ok(())
}

// ── replay: deterministic LLM answers from a recorded trace ──────────

fn replay_path() -> Option<PathBuf> {
    OVERRIDE.with(|o| {
        if let Some(ov) = o.borrow().as_ref() {
            return ov.replay.clone();
        }
        match std::env::var("NUDGE_REPLAY") {
            Ok(p) if !p.is_empty() => Some(PathBuf::from(p)),
            _ => None,
        }
    })
}

/// Is LLM replay active? (`NUDGE_REPLAY` set — any `NUDGE_REPLAY_MODE`,
/// matching the Python runtime: the LLM layer always replays.)
pub fn replay_active() -> bool {
    replay_path().is_some()
}

struct ReplayState {
    path: String,
    records: Vec<Value>,
    consumed: Vec<bool>,
    order_idx: usize,
    tool_records: Vec<Value>,
    tool_consumed: Vec<bool>,
    tool_order_idx: usize,
}

static REPLAY: Mutex<Option<ReplayState>> = Mutex::new(None);

/// Maximum trace file size loaded into memory (defense against a huge or
/// malicious trace exhausting memory). 256 MiB.
const MAX_TRACE_BYTES: u64 = 256 * 1024 * 1024;

/// Read a trace file with a size cap so a huge/malicious trace can't OOM the
/// process. Rejected loudly rather than silently truncating (which would break
/// replay determinism).
fn read_trace_capped(path: &str) -> Result<String, String> {
    let meta =
        std::fs::metadata(path).map_err(|_| format!("ReplayMismatch: trace not found: {path}"))?;
    if meta.len() > MAX_TRACE_BYTES {
        return Err(format!(
            "ReplayMismatch: trace file too large ({} bytes > {MAX_TRACE_BYTES} byte cap): {path}",
            meta.len()
        ));
    }
    std::fs::read_to_string(path)
        .map_err(|e| format!("ReplayMismatch: cannot read trace {path}: {e}"))
}

fn llm_records(path: &str) -> Result<Vec<Value>, String> {
    let text = read_trace_capped(path)?;
    let mut recs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: Value = serde_json::from_str(line)
            .map_err(|e| format!("ReplayMismatch: bad trace record: {e}"))?;
        if rec.get("v").and_then(Value::as_i64) != Some(1) {
            return Err(format!(
                "ReplayMismatch: unsupported trace record version {} (this runtime speaks v1)",
                rec.get("v").cloned().unwrap_or(Value::Null)
            ));
        }
        if rec.get("kind").and_then(Value::as_str) == Some("llm.call") {
            recs.push(rec);
        }
    }
    Ok(recs)
}

fn tool_records(path: &str) -> Result<Vec<Value>, String> {
    let text = read_trace_capped(path)?;
    let mut recs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: Value = serde_json::from_str(line)
            .map_err(|e| format!("ReplayMismatch: bad trace record: {e}"))?;
        if rec.get("v").and_then(Value::as_i64) != Some(1) {
            return Err(format!(
                "ReplayMismatch: unsupported trace record version {} (this runtime speaks v1)",
                rec.get("v").cloned().unwrap_or(Value::Null)
            ));
        }
        if rec.get("kind").and_then(Value::as_str) == Some("tool.call") {
            recs.push(rec);
        }
    }
    Ok(recs)
}

fn ensure_replay_loaded(path: &str, guard: &mut Option<ReplayState>) -> Result<(), String> {
    let _ = flush_sink();
    if guard.as_ref().map(|s| s.path.as_str()) != Some(path) {
        let records = llm_records(path)?;
        let consumed = vec![false; records.len()];
        let tool_records = tool_records(path)?;
        let tool_consumed = vec![false; tool_records.len()];
        *guard = Some(ReplayState {
            path: path.to_string(),
            records,
            consumed,
            order_idx: 0,
            tool_records,
            tool_consumed,
            tool_order_idx: 0,
        });
    }
    Ok(())
}

fn allow_unhashed_replay() -> bool {
    std::env::var("NUDGE_REPLAY_ALLOW_UNHASHED")
        .or_else(|_| std::env::var("NUDGE_REPLAY_ALLOW_LEGACY_FIFO"))
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Identity-first consumption (parity with Python's `_replay_take_llm`):
/// serve the first unconsumed `llm.call` whose `request_hash` matches —
/// a changed program can never consume an old answer. Traces where every
/// record carries a `request_hash` fail loud on a signature mismatch;
/// unhashed traces fail closed unless NUDGE_REPLAY_ALLOW_UNHASHED=1.
/// `Ok(None)` = the trace is exhausted.
pub fn replay_take_llm(hash: &str) -> Result<Option<Value>, String> {
    let path = replay_path()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if path.is_empty() {
        return Err("ReplayMismatch: no NUDGE_REPLAY trace configured".into());
    }
    let mut guard = REPLAY.lock().map_err(|_| "llm replay lock poisoned")?;
    ensure_replay_loaded(&path, &mut guard)?;
    let st = guard.as_mut().unwrap();
    if !st.records.is_empty() {
        if st.records.iter().all(|r| r.get("request_hash").is_some()) {
            let match_idx = st
                .records
                .iter()
                .enumerate()
                .find(|(i, r)| {
                    !st.consumed[*i] && r.get("request_hash").and_then(Value::as_str) == Some(hash)
                })
                .map(|(i, _)| i);
            return match match_idx {
                Some(i) => {
                    st.consumed[i] = true;
                    Ok(Some(st.records[i].clone()))
                }
                None => {
                    if st.consumed.iter().any(|c| !c) {
                        Err(format!(
                            "ReplayMismatch: replay signature mismatch: no recorded llm.call matches \
                             this prompt/model/schema (request_hash {hash}) — the program changed its \
                             LLM calls"
                        ))
                    } else {
                        Ok(None) // everything consumed — exhaustion at the caller
                    }
                }
            };
        } else if !allow_unhashed_replay() {
            return Err(
                "ReplayMismatch: replay trace contains records without request_hash (unvalidated FIFO replay rejected; set NUDGE_REPLAY_ALLOW_UNHASHED=1 to override)"
                    .to_string(),
            );
        } else {
            // legacy trace (records without request_hash): opt-in global record order
            if st.order_idx < st.records.len() {
                let rec = st.records[st.order_idx].clone();
                st.order_idx += 1;
                return Ok(Some(rec));
            } else {
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Deterministic tool replay: serve the recorded `output` for a matching
/// `tool.call` record from `NUDGE_REPLAY`.
///
/// Returns:
/// * `Ok(Some(output))` when a matching record was found and consumed.
/// * `Ok(None)` when replay mode is not active (`NUDGE_REPLAY` is unset or empty),
///   signaling the caller to run the tool live.
/// * `Err(ReplayMismatch)` when the trace has no matching record (mismatch)
///   or has exhausted its records.
pub fn tool_call_replay(
    tool: &str,
    server: Option<&str>,
    input: &Value,
) -> Result<Option<Value>, String> {
    let Some(path_buf) = replay_path() else {
        return Ok(None);
    };
    let path = path_buf.to_string_lossy().to_string();
    if path.is_empty() {
        return Ok(None);
    }

    let mut guard = REPLAY.lock().map_err(|_| "tool replay lock poisoned")?;
    ensure_replay_loaded(&path, &mut guard)?;
    let st = guard.as_mut().unwrap();

    let hash = tool_request_hash(tool, server, input);

    if !st.tool_records.is_empty() {
        if st
            .tool_records
            .iter()
            .all(|r| r.get("request_hash").is_some())
        {
            let match_idx = st
                .tool_records
                .iter()
                .enumerate()
                .find(|(i, r)| {
                    !st.tool_consumed[*i]
                        && r.get("request_hash").and_then(Value::as_str) == Some(&hash)
                })
                .map(|(i, _)| i);
            return match match_idx {
                Some(i) => {
                    st.tool_consumed[i] = true;
                    let output = st.tool_records[i]
                        .get("output")
                        .cloned()
                        .unwrap_or(Value::Null);
                    Ok(Some(output))
                }
                None => {
                    if st.tool_consumed.iter().any(|c| !c) {
                        Err(format!(
                            "ReplayMismatch: replay signature mismatch: no recorded tool.call for \
                             '{tool}' matches this server/arguments (request_hash {hash}) — the \
                             program changed its tool calls"
                        ))
                    } else {
                        let n = st.tool_records.len();
                        Err(format!(
                            "ReplayMismatch: program made more tool calls than the trace holds ({n} records)"
                        ))
                    }
                }
            };
        } else if !allow_unhashed_replay() {
            return Err(
                "ReplayMismatch: replay trace contains tool records without request_hash (unvalidated FIFO replay rejected; set NUDGE_REPLAY_ALLOW_UNHASHED=1 to override)"
                    .to_string(),
            );
        } else {
            // legacy trace (records without request_hash): opt-in global record order
            if st.tool_order_idx < st.tool_records.len() {
                let rec = &st.tool_records[st.tool_order_idx];
                st.tool_order_idx += 1;
                let output = rec.get("output").cloned().unwrap_or(Value::Null);
                return Ok(Some(output));
            } else {
                let n = st.tool_records.len();
                return Err(format!(
                    "ReplayMismatch: program made more tool calls than the trace holds ({n} records)"
                ));
            }
        }
    }

    let n = st.tool_records.len();
    Err(format!(
        "ReplayMismatch: program made more tool calls than the trace holds ({n} records)"
    ))
}

// ── the LLM bridge: one traced (or replayed) model call ─────────────

/// Why an LLM call failed — the interpreter maps each variant to its own
/// `E-native-03xx` diagnostic.
#[derive(Debug)]
pub enum LlmBridgeError {
    Provider(String),
    SchemaParse(String),
    Replay(String),
    Trace(String),
}

impl std::fmt::Display for LlmBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmBridgeError::Provider(e) => write!(f, "{e}"),
            LlmBridgeError::SchemaParse(e) => write!(f, "{e}"),
            LlmBridgeError::Replay(e) => write!(f, "{e}"),
            LlmBridgeError::Trace(e) => write!(f, "{e}"),
        }
    }
}

pub struct LlmBridgeRequest<'a> {
    pub prompt: &'a str,
    /// The `model:` option as written (None = the program did not set one;
    /// recorded/hashed as `"default"`, like the Python runtime).
    pub model: Option<&'a str>,
    /// Model string actually sent to the provider when `model` is None —
    /// surfaces default differently ("fake:default" vs the VM's
    /// `NUDGE_DEFAULT_MODEL`).
    pub default_model: &'a str,
    /// Schema as the provider sees it (the fake provider keys canned
    /// answers off it). Not part of the wire identity.
    pub schema_text: Option<&'a str>,
    /// The JSON-Schema dict the Python runtime would pass — the replay
    /// identity. `Some(_)` also means the output is a structured answer.
    pub schema_json: Option<Value>,
    pub temperature: f64,
    pub budget_usd: Option<f64>,
}

#[derive(Debug)]
pub struct LlmBridgeResponse {
    /// Raw text (no schema) or the schema-validated structured answer.
    pub output: Value,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
}

fn record_provider() -> String {
    std::env::var("NUDGE_PROVIDER").unwrap_or_else(|_| "fake".to_string())
}

/// Strip a ``` / ```json fence and parse the payload as JSON (the
/// interpreter's structured-answer contract).
fn parse_fenced_json(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.strip_suffix("```").unwrap_or(s).trim())
        .unwrap_or(trimmed);
    serde_json::from_str(body).map_err(|e| e.to_string())
}

/// One LLM call through the native bridge: replay (`NUDGE_REPLAY`) serves
/// the recorded answer and never touches a provider; otherwise the call
/// goes to the configured provider and is recorded as one `llm.call`.
pub fn llm_call(req: &LlmBridgeRequest<'_>) -> Result<LlmBridgeResponse, LlmBridgeError> {
    if crate::task::is_current_cancelled() {
        return Err(LlmBridgeError::Provider(
            "error[E-task-0001]: TaskCancelled".to_string(),
        ));
    }
    let record_model = req.model.unwrap_or("default").to_string();
    let hash = llm_request_hash(req.model, req.prompt, req.schema_json.as_ref(), &[]);

    if replay_path().is_some() {
        let rec = replay_take_llm(&hash).map_err(LlmBridgeError::Replay)?;
        let rec = rec.ok_or_else(|| {
            let n = REPLAY
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|s| s.records.len()))
                .unwrap_or(0);
            LlmBridgeError::Replay(format!(
                "ReplayMismatch: program made more llm calls than the trace holds ({n} records)"
            ))
        })?;
        let output = rec.get("output").cloned().unwrap_or(Value::Null);
        let tokens = rec.get("tokens");
        let num = |k: &str| {
            tokens
                .and_then(|t| t.get(k))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        return Ok(LlmBridgeResponse {
            output,
            tokens_in: num("in"),
            tokens_out: num("out"),
            cost_usd: rec.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        });
    }

    // live call — recorded after the fact ("trace complete up to the
    // crash point" carries the failed round too)
    let provider_model = req.model.unwrap_or(req.default_model).to_string();
    let call = provider::LlmRequest {
        prompt: req.prompt.to_string(),
        model: provider_model,
        schema: req.schema_text.map(str::to_string),
        temperature: req.temperature,
        budget_usd: req.budget_usd,
    };
    let provider = provider::get_active_provider();
    let resp = provider.call(&call).map_err(LlmBridgeError::Provider)?;

    let structured = req.schema_json.is_some();
    let mut parse_err: Option<String> = None;
    let (output, outcome) = if structured {
        match parse_fenced_json(&resp.text) {
            Ok(v) => (v, "ok"),
            Err(e) => {
                parse_err = Some(e);
                (Value::String(resp.text.clone()), "schema_violation")
            }
        }
    } else {
        (Value::String(resp.text.clone()), "ok")
    };

    let mut record = serde_json::Map::new();
    record.insert("kind".into(), json!("llm.call"));
    record.insert("model".into(), json!(record_model));
    // parity with the Python runtime's `_trace_call` (params is part of
    // the request_hash payload too)
    record.insert("params".into(), json!({"temperature": 0}));
    record.insert("input".into(), json!(req.prompt));
    record.insert("output".into(), output.clone());
    record.insert(
        "tokens".into(),
        json!({"in": resp.tokens_in, "out": resp.tokens_out}),
    );
    record.insert("cost_usd".into(), json!(resp.cost_usd));
    record.insert("repair_round".into(), json!(0));
    record.insert("outcome".into(), json!(outcome));
    record.insert("provider".into(), json!(record_provider()));
    record.insert("request_hash".into(), json!(hash));
    if let Some(s) = req.schema_text {
        // additive: the schema as the provider saw it (auditing aid —
        // replay identity lives in request_hash)
        record.insert("schema".into(), json!(s));
    }
    emit(Value::Object(record)).map_err(LlmBridgeError::Trace)?;
    let _ = flush_sink();

    if let Some(e) = parse_err {
        return Err(LlmBridgeError::SchemaParse(e));
    }
    Ok(LlmBridgeResponse {
        output,
        tokens_in: resp.tokens_in,
        tokens_out: resp.tokens_out,
        cost_usd: resp.cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Global trace/sink/seq state is shared across tests (and with the
    /// stdlib computer-replay tests that set NUDGE_REPLAY), so all tests that
    /// touch it serialize on the shared lock in `trace::testing`.
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        crate::trace::testing::TEST_STATE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn tmp(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uid = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "nudge-trace-{name}-{}_{}.jsonl",
            std::process::id(),
            uid
        ))
    }

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn request_hash_matches_python_reference_fixtures() {
        // fixtures generated with the Python runtime's exact formula:
        //   "sha256:" + hashlib.sha256(json.dumps(payload, sort_keys=True,
        //   separators=(",", ":"), default=str).encode()).hexdigest()
        assert_eq!(
            llm_request_hash(
                Some("anthropic:sonnet-4.6"),
                "Say hello to the Nudge language in one short sentence.",
                None,
                &[]
            ),
            "sha256:096a074c3f69d73b9abf55445bc8dd22a69655d90388978414b584cfd9d3e91d"
        );
        // schema dict + non-ASCII prompt (Python escapes with ensure_ascii)
        let schema = json!({"type": "array", "items": {"type": "string"}});
        assert_eq!(
            llm_request_hash(None, "3 search angles for: ısı adaları", Some(&schema), &[]),
            "sha256:0174b981f56550e63ab3d8a7a0afeec19d386b48d6ee4c82d5d4349497de436e"
        );
        // astral chars go out as UTF-16 surrogate pairs
        assert_eq!(
            llm_request_hash(None, "emoji 😀 test", None, &[]),
            "sha256:15350d23437d237632a411a8672820d399f15b432b524424e047bdb1581562a1"
        );
    }

    #[test]
    fn emit_writes_gapless_v1_records_and_seeds_seq_from_disk() {
        let _g = guard();
        let path = tmp("emit");
        let _ = std::fs::remove_file(&path);
        let _scope = testing::scope(Some(path.clone()), None);

        emit(json!({"kind": "llm.call", "model": "m", "outcome": "ok"})).unwrap();
        emit(json!({"kind": "fn.return", "fn": "main", "output": 1})).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        for (i, line) in lines.iter().enumerate() {
            let rec: Value = serde_json::from_str(line).unwrap();
            assert_eq!(rec.get("v").and_then(Value::as_i64), Some(1));
            assert_eq!(rec.get("seq").and_then(Value::as_i64), Some(i as i64 + 1));
        }
        // a later session on the same file continues the counter
        drop(_scope);
        let _scope = testing::scope(Some(path.clone()), None);
        emit(json!({"kind": "fn.return", "fn": "main", "output": 2})).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last.get("seq").and_then(Value::as_i64), Some(3));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bridge_records_llm_call_with_request_hash() {
        let _g = guard();
        let path = tmp("record");
        let _ = std::fs::remove_file(&path);
        let _scope = testing::scope(Some(path.clone()), None);

        let out = llm_call(&LlmBridgeRequest {
            prompt: "Say hi",
            model: Some("fake:default"),
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .expect("fake call");
        assert_eq!(out.output, Value::String("fake: Say hi".to_string()));

        let text = std::fs::read_to_string(&path).unwrap();
        let rec: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(rec.get("kind").and_then(Value::as_str), Some("llm.call"));
        assert_eq!(rec.get("v").and_then(Value::as_i64), Some(1));
        assert_eq!(rec.get("seq").and_then(Value::as_i64), Some(1));
        assert_eq!(
            rec.get("model").and_then(Value::as_str),
            Some("fake:default")
        );
        assert_eq!(rec.get("input").and_then(Value::as_str), Some("Say hi"));
        assert_eq!(rec.get("outcome").and_then(Value::as_str), Some("ok"));
        assert_eq!(rec.get("repair_round").and_then(Value::as_i64), Some(0));
        assert!(rec.get("cost_usd").and_then(Value::as_f64).is_some());
        assert!(rec.get("tokens").and_then(|t| t.get("in")).is_some());
        assert_eq!(
            rec.get("request_hash").and_then(Value::as_str),
            Some(llm_request_hash(Some("fake:default"), "Say hi", None, &[]).as_str())
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn replay_serves_recorded_outputs_identity_first() {
        let _g = guard();
        let path = tmp("replay");
        let hash_a = llm_request_hash(None, "prompt A", None, &[]);
        let hash_b = llm_request_hash(None, "prompt B", None, &[]);
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({"v": 1, "seq": 1, "kind": "llm.call", "model": "default",
                       "params": {"temperature": 0}, "input": "prompt A", "output": "answer A",
                       "tokens": {"in": 2, "out": 2}, "cost_usd": 0.001, "repair_round": 0,
                       "outcome": "ok", "provider": "fake", "request_hash": hash_a}),
                json!({"v": 1, "seq": 2, "kind": "llm.call", "model": "default",
                       "params": {"temperature": 0}, "input": "prompt B", "output": "answer B",
                       "tokens": {"in": 2, "out": 2}, "cost_usd": 0.001, "repair_round": 0,
                       "outcome": "ok", "provider": "fake", "request_hash": hash_b}),
            ),
        )
        .unwrap();
        let _scope = testing::scope(None, Some(path.clone()));
        assert!(replay_active());

        // out-of-order consumption: identity, not wall-clock order
        let b = llm_call(&LlmBridgeRequest {
            prompt: "prompt B",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap();
        assert_eq!(b.output, Value::String("answer B".to_string()));
        assert_eq!(b.cost_usd, 0.001);
        let a = llm_call(&LlmBridgeRequest {
            prompt: "prompt A",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap();
        assert_eq!(a.output, Value::String("answer A".to_string()));

        // a changed prompt can NOT consume an old answer — loud mismatch
        let err = llm_call(&LlmBridgeRequest {
            prompt: "prompt C",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap_err();
        assert!(matches!(err, LlmBridgeError::Replay(_)));
        assert!(err.to_string().contains("ReplayMismatch"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn replay_exhaustion_is_a_clear_error() {
        let _g = guard();
        let path = tmp("exhaust");
        let hash_a = llm_request_hash(None, "prompt A", None, &[]);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"v": 1, "seq": 1, "kind": "llm.call", "model": "default",
                       "params": {"temperature": 0}, "input": "prompt A", "output": "answer A",
                       "tokens": {"in": 2, "out": 2}, "cost_usd": 0.001, "repair_round": 0,
                       "outcome": "ok", "provider": "fake", "request_hash": hash_a}),
            ),
        )
        .unwrap();
        let _scope = testing::scope(None, Some(path.clone()));
        let req = LlmBridgeRequest {
            prompt: "prompt A",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        };
        llm_call(&req).unwrap();
        let err = llm_call(&req).unwrap_err();
        assert!(matches!(err, LlmBridgeError::Replay(_)));
        assert!(
            err.to_string().contains("more llm calls than the trace"),
            "{err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unhashed_records_without_opt_in_fails_closed() {
        let _g = guard();
        let path = tmp("unhashed-deny");
        std::fs::write(
            &path,
            concat!(
                "{\"v\": 1, \"seq\": 1, \"kind\": \"llm.call\", \"model\": \"default\",",
                "\"params\": {\"temperature\": 0}, \"input\": \"one\", \"output\": \"first\",",
                "\"tokens\": {\"in\": 1, \"out\": 1}, \"cost_usd\": 0.001, \"repair_round\": 0,",
                "\"outcome\": \"ok\", \"provider\": \"fake\"}\n",
            ),
        )
        .unwrap();
        std::env::remove_var("NUDGE_REPLAY_ALLOW_UNHASHED");
        let _scope = testing::scope(None, Some(path.clone()));
        let req = LlmBridgeRequest {
            prompt: "one",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        };
        let err = llm_call(&req).unwrap_err().to_string();
        assert!(
            err.contains("ReplayMismatch") && err.contains("request_hash"),
            "expected fail closed without request_hash, got: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_records_without_request_hash_replay_in_order() {
        let _g = guard();
        let path = tmp("legacy");
        std::fs::write(
            &path,
            concat!(
                "{\"v\": 1, \"seq\": 1, \"kind\": \"llm.call\", \"model\": \"default\",",
                "\"params\": {\"temperature\": 0}, \"input\": \"one\", \"output\": \"first\",",
                "\"tokens\": {\"in\": 1, \"out\": 1}, \"cost_usd\": 0.001, \"repair_round\": 0,",
                "\"outcome\": \"ok\", \"provider\": \"fake\"}\n",
                "{\"v\": 1, \"seq\": 2, \"kind\": \"llm.call\", \"model\": \"default\",",
                "\"params\": {\"temperature\": 0}, \"input\": \"two\", \"output\": \"second\",",
                "\"tokens\": {\"in\": 1, \"out\": 1}, \"cost_usd\": 0.001, \"repair_round\": 0,",
                "\"outcome\": \"ok\", \"provider\": \"fake\"}\n",
            ),
        )
        .unwrap();
        std::env::set_var("NUDGE_REPLAY_ALLOW_UNHASHED", "1");
        let _scope = testing::scope(None, Some(path.clone()));
        fn req<'a>(p: &'a str) -> LlmBridgeRequest<'a> {
            LlmBridgeRequest {
                prompt: p,
                model: None,
                default_model: "fake:default",
                schema_text: None,
                schema_json: None,
                temperature: 0.7,
                budget_usd: None,
            }
        }
        // global order — even the "wrong" prompt gets the next record
        assert_eq!(
            llm_call(&req("one")).unwrap().output,
            Value::String("first".to_string())
        );
        assert_eq!(
            llm_call(&req("whatever")).unwrap().output,
            Value::String("second".to_string())
        );
        assert!(llm_call(&req("three")).is_err());
        std::env::remove_var("NUDGE_REPLAY_ALLOW_UNHASHED");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn replay_never_calls_a_provider_and_writes_no_trace() {
        let _g = guard();
        let path = tmp("no-provider");
        let sink = tmp("no-provider-sink");
        let _ = std::fs::remove_file(&sink);
        let hash = llm_request_hash(None, "cached", None, &[]);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"v": 1, "seq": 1, "kind": "llm.call", "model": "default",
                       "params": {"temperature": 0}, "input": "cached", "output": "from-trace",
                       "tokens": {"in": 1, "out": 1}, "cost_usd": 0.001, "repair_round": 0,
                       "outcome": "ok", "provider": "fake", "request_hash": hash}),
            ),
        )
        .unwrap();
        let _scope = testing::scope(Some(sink.clone()), Some(path.clone()));
        let out = llm_call(&LlmBridgeRequest {
            prompt: "cached",
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap();
        // the cached answer is served verbatim — the fake provider would
        // have answered "fake: cached"
        assert_eq!(out.output, Value::String("from-trace".to_string()));
        assert!(
            !sink.exists(),
            "replayed calls must not write new trace records"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tool_request_hash_matches_python_reference_fixtures() {
        let input = json!({"query": "test"});
        assert_eq!(
            tool_request_hash("search", Some("kb"), &input),
            "sha256:0bcac4400201e96e98eb51b28337092d64d6d427ded350d2c1bf9f176a460216"
        );
        assert_eq!(
            tool_request_hash("search", None, &input),
            "sha256:8142cde6c03f57cced578e09cd7a778add8fe088577c7672d73cba7bd48390c1"
        );
    }

    #[test]
    fn tool_replay_serves_correct_recorded_output() {
        let _g = guard();
        let path = tmp("tool-match");
        let input = json!({"query": "rust replay"});
        let hash = tool_request_hash("search", Some("kb"), &input);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({
                    "v": 1,
                    "seq": 1,
                    "kind": "tool.call",
                    "tool": "search",
                    "server": "kb",
                    "input": input,
                    "output": ["doc1", "doc2"],
                    "request_hash": hash
                })
            ),
        )
        .unwrap();

        let _scope = testing::scope(None, Some(path.clone()));
        let res = tool_call_replay("search", Some("kb"), &input).unwrap();
        assert_eq!(res, Some(json!(["doc1", "doc2"])));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tool_replay_two_identical_calls_replay_distinct_outputs_in_order() {
        let _g = guard();
        let path = tmp("tool-in-order");
        let input = json!({"tick": 1});
        let hash = tool_request_hash("clock", None, &input);
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({
                    "v": 1,
                    "seq": 1,
                    "kind": "tool.call",
                    "tool": "clock",
                    "server": null,
                    "input": input,
                    "output": 100,
                    "request_hash": hash
                }),
                json!({
                    "v": 1,
                    "seq": 2,
                    "kind": "tool.call",
                    "tool": "clock",
                    "server": null,
                    "input": input,
                    "output": 200,
                    "request_hash": hash
                })
            ),
        )
        .unwrap();

        let _scope = testing::scope(None, Some(path.clone()));
        let first = tool_call_replay("clock", None, &input).unwrap();
        assert_eq!(first, Some(json!(100)));
        let second = tool_call_replay("clock", None, &input).unwrap();
        assert_eq!(second, Some(json!(200)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tool_replay_exhaustion_yields_replay_mismatch() {
        let _g = guard();
        let path = tmp("tool-exhaust");
        let input = json!({"action": "ping"});
        let hash = tool_request_hash("network", None, &input);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({
                    "v": 1,
                    "seq": 1,
                    "kind": "tool.call",
                    "tool": "network",
                    "server": null,
                    "input": input,
                    "output": "pong",
                    "request_hash": hash
                })
            ),
        )
        .unwrap();

        let _scope = testing::scope(None, Some(path.clone()));
        let first = tool_call_replay("network", None, &input).unwrap();
        assert_eq!(first, Some(json!("pong")));

        let err = tool_call_replay("network", None, &input).unwrap_err();
        assert!(
            err.contains("ReplayMismatch: program made more tool calls than the trace holds"),
            "unexpected error message: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tool_replay_no_replay_env_signals_go_live() {
        let _g = guard();
        let _scope = testing::scope(None, None);
        let res = tool_call_replay("search", Some("kb"), &json!({"q": "live"})).unwrap();
        assert_eq!(res, None, "unset replay env must signal go-live (None)");
    }

    #[test]
    fn tool_replay_request_hash_mismatch_yields_mismatch_error() {
        let _g = guard();
        let path = tmp("tool-mismatch");
        let recorded_input = json!({"query": "expected"});
        let hash = tool_request_hash("search", Some("kb"), &recorded_input);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({
                    "v": 1,
                    "seq": 1,
                    "kind": "tool.call",
                    "tool": "search",
                    "server": "kb",
                    "input": recorded_input,
                    "output": "result",
                    "request_hash": hash
                })
            ),
        )
        .unwrap();

        let _scope = testing::scope(None, Some(path.clone()));
        let actual_input = json!({"query": "different"});
        let err = tool_call_replay("search", Some("kb"), &actual_input).unwrap_err();
        assert!(
            err.contains(
                "ReplayMismatch: replay signature mismatch: no recorded tool.call for 'search' matches"
            ),
            "unexpected error message: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tool_and_llm_replay_do_not_interfere() {
        let _g = guard();
        let path = tmp("tool-llm-no-interference");
        let prompt = "What is 2 + 2?";
        let llm_hash = llm_request_hash(None, prompt, None, &[]);
        let tool_input = json!({"expr": "2+2"});
        let tool_hash = tool_request_hash("calc", None, &tool_input);
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({
                    "v": 1,
                    "seq": 1,
                    "kind": "llm.call",
                    "model": "default",
                    "params": {"temperature": 0},
                    "input": prompt,
                    "output": "4",
                    "tokens": {"in": 5, "out": 1},
                    "cost_usd": 0.001,
                    "repair_round": 0,
                    "outcome": "ok",
                    "provider": "fake",
                    "request_hash": llm_hash
                }),
                json!({
                    "v": 1,
                    "seq": 2,
                    "kind": "tool.call",
                    "tool": "calc",
                    "server": null,
                    "input": tool_input,
                    "output": 4,
                    "request_hash": tool_hash
                })
            ),
        )
        .unwrap();

        let _scope = testing::scope(None, Some(path.clone()));
        // Call tool first
        let tool_out = tool_call_replay("calc", None, &tool_input).unwrap();
        assert_eq!(tool_out, Some(json!(4)));

        // Call LLM second
        let llm_out = llm_call(&LlmBridgeRequest {
            prompt,
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap();
        assert_eq!(llm_out.output, Value::String("4".to_string()));

        // Tool exhaustion
        let tool_err = tool_call_replay("calc", None, &tool_input).unwrap_err();
        assert!(
            tool_err.contains("ReplayMismatch: program made more tool calls than the trace holds")
        );

        // LLM exhaustion
        let llm_err = llm_call(&LlmBridgeRequest {
            prompt,
            model: None,
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.7,
            budget_usd: None,
        })
        .unwrap_err();
        assert!(llm_err
            .to_string()
            .contains("more llm calls than the trace holds"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unhashed_tool_records_without_opt_in_fails_closed() {
        let _g = guard();
        let path = tmp("unhashed-tool-deny");
        std::fs::write(
            &path,
            "{\"v\": 1, \"seq\": 1, \"kind\": \"tool.call\", \"tool\": \"search\", \"input\": \"one\", \"output\": \"res1\"}\n",
        )
        .unwrap();
        std::env::remove_var("NUDGE_REPLAY_ALLOW_UNHASHED");
        let _scope = testing::scope(None, Some(path.clone()));
        let err = tool_call_replay("search", None, &json!("different")).unwrap_err();
        assert!(
            err.contains("ReplayMismatch") && err.contains("request_hash"),
            "expected tool replay to fail closed without request_hash, got: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_tool_records_without_request_hash_replay_in_order() {
        let _g = guard();
        let path = tmp("legacy-tool");
        std::fs::write(
            &path,
            concat!(
                "{\"v\": 1, \"seq\": 1, \"kind\": \"tool.call\", \"tool\": \"search\", \"input\": \"one\", \"output\": \"res1\"}\n",
                "{\"v\": 1, \"seq\": 2, \"kind\": \"tool.call\", \"tool\": \"search\", \"input\": \"two\", \"output\": \"res2\"}\n",
            ),
        )
        .unwrap();

        std::env::set_var("NUDGE_REPLAY_ALLOW_UNHASHED", "1");
        let _scope = testing::scope(None, Some(path.clone()));
        assert_eq!(
            tool_call_replay("search", None, &json!("different")).unwrap(),
            Some(json!("res1"))
        );
        assert_eq!(
            tool_call_replay("search", None, &json!("anything")).unwrap(),
            Some(json!("res2"))
        );
        let err = tool_call_replay("search", None, &json!("three")).unwrap_err();
        assert!(err.contains("more tool calls than the trace holds (2 records)"));
        std::env::remove_var("NUDGE_REPLAY_ALLOW_UNHASHED");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_writer_buffered_writes_flush_on_drop_without_loss() {
        let _g = guard();
        let path = tmp("buffered-drop");
        let _ = std::fs::remove_file(&path);

        // Write 50 events through buffered TraceWriter without calling flush()
        {
            let mut writer = TraceWriter::open(&path).expect("open writer");
            for i in 1..=50 {
                writer
                    .emit(json!({
                        "kind": "custom.event",
                        "index": i,
                        "data": format!("payload-{i}")
                    }))
                    .expect("emit record");
            }
            // Explicitly do not call flush(): verify Drop flushes all buffered records
        }

        // Now that writer has dropped, assert all 50 lines are present on disk
        let text = std::fs::read_to_string(&path).expect("read trace file");
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            50,
            "expected all 50 records to be flushed on drop"
        );

        for (i, line) in lines.iter().enumerate() {
            let rec: Value = serde_json::from_str(line).expect("valid json");
            assert_eq!(rec.get("v").and_then(Value::as_i64), Some(1));
            assert_eq!(rec.get("seq").and_then(Value::as_i64), Some(i as i64 + 1));
            assert_eq!(rec.get("index").and_then(Value::as_i64), Some(i as i64 + 1));
            assert_eq!(
                rec.get("data").and_then(Value::as_str),
                Some(format!("payload-{}", i + 1).as_str())
            );
        }

        // Re-open existing trace with another TraceWriter: seq continues from 50 without gap
        {
            let mut writer = TraceWriter::open(&path).expect("reopen writer");
            assert_eq!(writer.current_seq(), 50);
            writer
                .emit(json!({"kind": "custom.event", "index": 51, "data": "payload-51"}))
                .expect("emit 51");
            let _file = writer
                .into_inner()
                .expect("into_inner succeeds and flushes");
        }

        let text2 = std::fs::read_to_string(&path).expect("read trace file 2");
        let lines2: Vec<&str> = text2.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines2.len(), 51);
        let last: Value = serde_json::from_str(lines2.last().unwrap()).unwrap();
        assert_eq!(last.get("seq").and_then(Value::as_i64), Some(51));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_writer_buffered_round_trip_replay_byte_identical() {
        let _g = guard();
        let path = tmp("buffered-roundtrip");
        let _ = std::fs::remove_file(&path);

        let hash_a = llm_request_hash(Some("fake:default"), "prompt A", None, &[]);
        let hash_b = llm_request_hash(Some("fake:default"), "prompt B", None, &[]);
        let tool_input = json!({"cmd": "status"});
        let tool_hash = tool_request_hash("sys", None, &tool_input);

        // Record trace through buffered TraceWriter
        {
            let mut writer = TraceWriter::open(&path).expect("open writer");
            writer
                .emit(json!({
                    "kind": "llm.call",
                    "model": "fake:default",
                    "params": {"temperature": 0},
                    "input": "prompt A",
                    "output": "answer A",
                    "tokens": {"in": 10, "out": 20},
                    "cost_usd": 0.0005,
                    "repair_round": 0,
                    "outcome": "ok",
                    "provider": "fake",
                    "request_hash": hash_a
                }))
                .unwrap();
            writer
                .emit(json!({
                    "kind": "tool.call",
                    "tool": "sys",
                    "server": null,
                    "input": tool_input,
                    "output": {"healthy": true},
                    "request_hash": tool_hash
                }))
                .unwrap();
            writer
                .emit(json!({
                    "kind": "llm.call",
                    "model": "fake:default",
                    "params": {"temperature": 0},
                    "input": "prompt B",
                    "output": "answer B",
                    "tokens": {"in": 5, "out": 15},
                    "cost_usd": 0.0003,
                    "repair_round": 0,
                    "outcome": "ok",
                    "provider": "fake",
                    "request_hash": hash_b
                }))
                .unwrap();
            // Drop flushes writer
        }

        // Replay against recorded trace
        let _scope = testing::scope(None, Some(path.clone()));
        assert!(replay_active());

        // Replay tool call
        let tool_res = tool_call_replay("sys", None, &tool_input).unwrap();
        assert_eq!(tool_res, Some(json!({"healthy": true})));

        // Replay LLM calls
        let resp_b = llm_call(&LlmBridgeRequest {
            prompt: "prompt B",
            model: Some("fake:default"),
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.0,
            budget_usd: None,
        })
        .unwrap();
        assert_eq!(resp_b.output, Value::String("answer B".to_string()));
        assert_eq!(resp_b.cost_usd, 0.0003);

        let resp_a = llm_call(&LlmBridgeRequest {
            prompt: "prompt A",
            model: Some("fake:default"),
            default_model: "fake:default",
            schema_text: None,
            schema_json: None,
            temperature: 0.0,
            budget_usd: None,
        })
        .unwrap();
        assert_eq!(resp_a.output, Value::String("answer A".to_string()));
        assert_eq!(resp_a.cost_usd, 0.0005);

        let _ = std::fs::remove_file(&path);
    }

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::set_var(key, val);
            crate::redact::reset_redactor_cache();
            Self { key, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
            crate::redact::reset_redactor_cache();
        }
    }

    #[test]
    fn test_trace_redaction_masks_secrets_and_preserves_deterministic_replay() {
        let _g = guard();
        let _ws = crate::workspace::WS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let trace_path = tmp("redact_llm_tool");
        let secret_key = "sk-ant-api03-secretkey1234567890abcdef";
        let prompt = format!("Translate query using credentials: {secret_key}");
        let raw_tool_input = json!({
            "api_key": "secret1234567",
            "token": "Bearer 1234567890abcdef1234567890",
            "query": "select * from users"
        });
        let raw_tool_output = json!({"status": "ok", "rows": 10});

        // 1. Live recording run with redaction active
        let live_output = {
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

            // Also emit a tool.call record
            let tool_hash = tool_request_hash("search", Some("kb"), &raw_tool_input);
            emit(json!({
                "kind": "tool.call",
                "tool": "search",
                "server": "kb",
                "input": raw_tool_input,
                "output": raw_tool_output,
                "request_hash": tool_hash,
            }))
            .expect("tool call emit");

            resp.output
        };

        // 2. File inspection: Verify that plaintext secrets are absent and [REDACTED] is present
        let trace_content = std::fs::read_to_string(&trace_path).expect("read trace");
        assert!(
            !trace_content.contains(secret_key),
            "trace file must NOT contain raw secret key"
        );
        assert!(
            !trace_content.contains("secret1234567"),
            "trace file must NOT contain raw api_key"
        );
        assert!(
            !trace_content.contains("1234567890abcdef1234567890"),
            "trace file must NOT contain raw bearer token"
        );
        assert!(
            trace_content.contains("[REDACTED]"),
            "trace file must contain redaction mask"
        );

        // 3. Offline replay run: Verify deterministic replay matches pre-redaction request_hash
        {
            let _scope = testing::scope(None, Some(trace_path.clone()));
            assert!(replay_active());

            // Replay tool call with original pre-redaction input
            let tool_res = tool_call_replay("search", Some("kb"), &raw_tool_input)
                .expect("tool replay must match");
            assert_eq!(tool_res, Some(raw_tool_output));

            // Replay LLM call with original pre-redaction prompt
            let req = LlmBridgeRequest {
                prompt: &prompt,
                model: Some("fake:default"),
                default_model: "fake:default",
                schema_text: None,
                schema_json: None,
                temperature: 0.0,
                budget_usd: None,
            };
            let replay_resp = llm_call(&req).expect("llm replay must match recorded trace");
            // The replay output matches what was recorded into trace
            assert_eq!(
                replay_resp.output,
                Value::String("fake: Translate query using credentials: [REDACTED]".to_string())
            );
            assert_eq!(
                replay_resp.output,
                Value::String(
                    live_output
                        .as_str()
                        .unwrap()
                        .replace(secret_key, "[REDACTED]")
                )
            );
        }

        let _ = std::fs::remove_file(&trace_path);
    }

    #[test]
    fn test_trace_redaction_escape_hatch_verbatim() {
        let _g = guard();
        let _ws = crate::workspace::WS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = EnvVarGuard::set("NUDGE_TRACE_REDACT", "0");
        let trace_path = tmp("redact_verbatim");
        let secret = "sk-ant-api03-verbatimsecret1234567890";

        {
            let mut writer = TraceWriter::open(&trace_path).expect("open writer");
            writer
                .emit(json!({
                    "kind": "llm.call",
                    "model": "fake:default",
                    "input": format!("raw prompt with {secret}"),
                    "output": "ok",
                    "request_hash": "sha256:dummy",
                }))
                .expect("emit");
            writer.flush().expect("flush");
        }

        let trace_content = std::fs::read_to_string(&trace_path).expect("read trace");
        assert!(
            trace_content.contains(secret),
            "when NUDGE_TRACE_REDACT=0, secret must be preserved verbatim"
        );
        assert!(
            !trace_content.contains("[REDACTED]"),
            "when NUDGE_TRACE_REDACT=0, [REDACTED] must not appear"
        );

        let _ = std::fs::remove_file(&trace_path);
    }

    #[test]
    fn test_trace_redaction_fn_return_and_computer_records() {
        let _g = guard();
        let _ws = crate::workspace::WS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let trace_path = tmp("redact_fn_computer");

        {
            let mut writer = TraceWriter::open(&trace_path).expect("open writer");
            // fn.return record
            writer
                .emit(json!({
                    "kind": "fn.return",
                    "fn": "auth_login",
                    "args": ["Authorization: Bearer secret_bearer_token_12345"],
                    "output": {
                        "password": "super_secret_password_123",
                        "token": "secret_token_val_456",
                        "status": "success"
                    }
                }))
                .expect("emit fn.return");

            // computer.act record
            writer
                .emit(json!({
                    "kind": "computer.act",
                    "action": "type",
                    "app": "Terminal",
                    "target": null,
                    "value": "export API_KEY=ghp_012345678901234567890123456789012345",
                    "request_hash": "sha256:comp123",
                    "outcome": "ok",
                    "latency_ms": 10
                }))
                .expect("emit computer.act");

            writer.flush().expect("flush");
        }

        let trace_content = std::fs::read_to_string(&trace_path).expect("read trace");
        assert!(
            !trace_content.contains("secret_bearer_token_12345"),
            "fn.return bearer token must be masked"
        );
        assert!(
            !trace_content.contains("super_secret_password_123"),
            "fn.return password must be masked"
        );
        assert!(
            !trace_content.contains("secret_token_val_456"),
            "fn.return token field must be masked"
        );
        assert!(
            !trace_content.contains("ghp_012345678901234567890123456789012345"),
            "computer.act ghp token must be masked"
        );
        assert!(
            trace_content.contains(r#""password": "[REDACTED]""#),
            "structural field password must be [REDACTED]"
        );
        assert!(
            trace_content.contains("sha256:comp123"),
            "request_hash must be preserved verbatim"
        );

        let _ = std::fs::remove_file(&trace_path);
    }

    #[test]
    fn test_trace_redaction_env_secret_detection() {
        let _g = guard();
        let _ws = crate::workspace::WS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let secret_val = "super_secret_env_token_value_789";
        let _env = EnvVarGuard::set("TEST_RUNTIME_SECRET_TOKEN", secret_val);
        let trace_path = tmp("redact_env_detect");

        {
            let mut writer = TraceWriter::open(&trace_path).expect("open writer");
            writer
                .emit(json!({
                    "kind": "llm.call",
                    "model": "fake:default",
                    "input": format!("Please query with token {secret_val} now"),
                    "output": "ok",
                    "request_hash": "sha256:envhash",
                }))
                .expect("emit");
            writer.flush().expect("flush");
        }

        let trace_content = std::fs::read_to_string(&trace_path).expect("read trace");
        assert!(
            !trace_content.contains(secret_val),
            "trace must NOT contain raw env secret"
        );
        assert!(
            trace_content.contains("[REDACTED:ENV:TEST_RUNTIME_SECRET_TOKEN]"),
            "trace must contain [REDACTED:ENV:NAME]"
        );

        let _ = std::fs::remove_file(&trace_path);
    }
}
