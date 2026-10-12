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

fn sha256(data: &[u8]) -> [u8; 32] {
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

fn sha256_hex(data: &[u8]) -> String {
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

// ── sink: NTF v1 trace writing ─────────────────────────────────────

static DEFAULT_SINK: AtomicBool = AtomicBool::new(false);
static SINK: Mutex<Option<(String, u64)>> = Mutex::new(None);

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

    /// Thread-local trace/replay isolation for tests: inside the scope the
    /// current thread uses exactly `sink` / `replay` (None = disabled) and
    /// ignores the environment. Restored on drop.
    pub struct Scope {
        prev: Option<Overrides>,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            OVERRIDE.with(|o| *o.borrow_mut() = self.prev.take());
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
    let key = path.to_string_lossy().to_string();
    let mut guard = SINK.lock().map_err(|_| "trace sink lock poisoned")?;
    let seq = {
        let state = guard.as_mut();
        match state {
            Some((k, n)) if *k == key => {
                *n += 1;
                *n
            }
            _ => {
                let n = count_records(&path) + 1;
                *guard = Some((key, n));
                n
            }
        }
    };
    let Value::Object(mut fields) = record else {
        return Err("trace record must be a JSON object".into());
    };
    // Python emits {"v": 1, "seq": n, **record} — v and seq first
    let mut line = serde_json::Map::new();
    line.insert("v".into(), json!(1));
    line.insert("seq".into(), json!(seq));
    line.append(&mut fields);
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("cannot open trace '{}': {e}", path.display()))?;
    f.write_all(line_bytes(&Value::Object(line)).as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .map_err(|e| format!("cannot write trace '{}': {e}", path.display()))?;
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
}

static REPLAY: Mutex<Option<ReplayState>> = Mutex::new(None);

fn llm_records(path: &str) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| format!("ReplayMismatch: trace not found: {path}"))?;
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

/// Identity-first consumption (parity with Python's `_replay_take_llm`):
/// serve the first unconsumed `llm.call` whose `request_hash` matches —
/// a changed program can never consume an old answer. Traces where every
/// record carries a `request_hash` fail loud on a signature mismatch;
/// legacy traces (any record without one) consume in global record
/// order. `Ok(None)` = the trace is exhausted.
pub fn replay_take_llm(hash: &str) -> Result<Option<Value>, String> {
    let path = replay_path()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if path.is_empty() {
        return Err("ReplayMismatch: no NUDGE_REPLAY trace configured".into());
    }
    let mut guard = REPLAY.lock().map_err(|_| "llm replay lock poisoned")?;
    if guard.as_ref().map(|s| s.path.as_str()) != Some(path.as_str()) {
        let records = llm_records(&path)?;
        let consumed = vec![false; records.len()];
        *guard = Some(ReplayState {
            path: path.clone(),
            records,
            consumed,
            order_idx: 0,
        });
    }
    let st = guard.as_mut().unwrap();
    if !st.records.is_empty() && st.records.iter().all(|r| r.get("request_hash").is_some()) {
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
    }
    // legacy trace (records without request_hash): global record order
    if st.order_idx < st.records.len() {
        let rec = st.records[st.order_idx].clone();
        st.order_idx += 1;
        Ok(Some(rec))
    } else {
        Ok(None)
    }
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
    use std::sync::Mutex;

    /// trace/replay state and the test scopes are process-global — one
    /// lock keeps the tests from stepping on each other.
    static TLOCK: Mutex<()> = Mutex::new(());

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        TLOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nudge-trace-{name}-{}.jsonl", std::process::id()))
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
}
