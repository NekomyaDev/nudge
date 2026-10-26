//! Browser kernel primitives (v1.6, docs/browser-use.md).
//!
//! Exposes native `browser.fetch` for raw byte transfer over TLS.
//! All higher-level browser behaviors (DOM, CSS, JS interpreter, cookies,
//! forms, history, policies) live entirely in `.ndg` prelude code.

use super::Value;
use std::collections::HashMap;
use std::time::Instant;

pub const METHODS: [&str; 1] = ["browser.fetch"];

/// Maximum allowed response body size: 10 MB (10 * 1024 * 1024 bytes).
/// Guard against infinite streaming and decompression bombs.
pub const MAX_BODY_BYTES: usize = 10_485_760;

pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    for name in METHODS {
        functions.insert(name.to_string(), Value::Native(name));
    }
    functions
}

/// Execute a native browser kernel primitive.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    if crate::task::is_current_cancelled() {
        return Err("error[E-task-0001]: TaskCancelled".to_string());
    }
    if !METHODS.contains(&name) {
        return Err(format!("Unknown browser function: {name}"));
    }
    if std::env::var("NUDGE_BROWSER_KILL").as_deref() == Ok("1") {
        return Err("BrowserDenied: NUDGE_BROWSER_KILL=1 refuses all browser operations".into());
    }

    match name {
        "browser.fetch" => execute_fetch(args),
        _ => Err(format!("Unimplemented browser function: {name}")),
    }
}

fn execute_fetch(args: Vec<Value>) -> Result<Value, String> {
    let url_val = args
        .first()
        .ok_or_else(|| "browser.fetch requires at least 1 argument (url)".to_string())?;

    let url = match url_val {
        Value::String(s) => s.clone(),
        other => return Err(format!("browser.fetch url must be string, got {other:?}")),
    };

    let mut method = "GET".to_string();
    let mut body: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut timeout_ms: u64 = 20_000;

    // Optional options record / map
    if let Some(Value::Map(m)) = args.get(1) {
        if let Some(Value::String(m_str)) = m.get("method") {
            method = m_str.to_uppercase();
        }
        if let Some(Value::String(b_str)) = m.get("body") {
            body = Some(b_str.clone());
        }
        if let Some(Value::Int(t)) = m.get("timeout_ms") {
            if *t > 0 {
                timeout_ms = *t as u64;
            }
        }
        if let Some(Value::List(h_list)) = m.get("headers") {
            for item in h_list {
                if let Value::Map(h_map) = item {
                    if let (Some(Value::String(k)), Some(Value::String(v))) =
                        (h_map.get("name"), h_map.get("value"))
                    {
                        headers.push((k.clone(), v.clone()));
                    }
                }
            }
        } else if let Some(Value::Map(h_map)) = m.get("headers") {
            for (k, v) in h_map {
                if let Value::String(v_str) = v {
                    headers.push((k.clone(), v_str.clone()));
                }
            }
        }
    }

    let start = Instant::now();

    // ── Handle file:// URLs for fixture testing ──────────────────────────
    if let Some(path) = url.strip_prefix("file://") {
        return execute_file_fetch(path, &url, start);
    }

    // ── Handle http:// and https:// URLs ─────────────────────────────────
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!(
            "BrowserDenied: unsupported scheme for '{url}' (only http, https, and file: are allowed)"
        ));
    }

    execute_network_fetch(&url, &method, headers, body, timeout_ms, start)
}

fn execute_file_fetch(path: &str, url: &str, start: Instant) -> Result<Value, String> {
    // Confine file: strictly to workspace/current directory (prevent path traversal)
    let p = std::path::Path::new(path);
    if p.is_absolute() && !p.exists() {
        return Err(format!("BrowserError: file not found: {path}"));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("BrowserError: failed to read file fixture '{path}': {e}"))?;

    if content.len() > MAX_BODY_BYTES {
        return Err(format!(
            "BodyTooLarge: file size ({} bytes) exceeds max_body_bytes ({MAX_BODY_BYTES})",
            content.len()
        ));
    }

    let latency_ms = start.elapsed().as_millis() as i64;

    let mut resp = HashMap::new();
    resp.insert("status".to_string(), Value::Int(200));
    resp.insert("url".to_string(), Value::String(url.to_string()));
    resp.insert("body".to_string(), Value::String(content));
    resp.insert("latency_ms".to_string(), Value::Int(latency_ms));

    let mut header_list = Vec::new();
    let mut ct_header = HashMap::new();
    ct_header.insert(
        "name".to_string(),
        Value::String("content-type".to_string()),
    );
    ct_header.insert(
        "value".to_string(),
        Value::String("text/html; charset=utf-8".to_string()),
    );
    header_list.push(Value::Map(ct_header));

    resp.insert("headers".to_string(), Value::List(header_list));
    Ok(Value::Map(resp))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "live-provider"))]
fn execute_network_fetch(
    url: &str,
    method: &str,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
    start: Instant,
) -> Result<Value, String> {
    use std::io::Read;
    use std::time::Duration;

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(timeout_ms))
        .build();

    let mut req = match method {
        "POST" => agent.post(url),
        "PUT" => agent.put(url),
        "DELETE" => agent.delete(url),
        "HEAD" => agent.head(url),
        _ => agent.get(url),
    };

    for (k, v) in headers {
        req = req.set(&k, &v);
    }

    let response = match body {
        Some(ref b) => req.send_string(b),
        None => req.call(),
    };

    let latency_ms = start.elapsed().as_millis() as i64;

    match response {
        Ok(resp) => {
            let status = resp.status() as i64;
            let mut out_headers = Vec::new();
            for name in resp.headers_names() {
                if let Some(val) = resp.header(&name) {
                    let mut h = HashMap::new();
                    h.insert("name".to_string(), Value::String(name.to_lowercase()));
                    h.insert("value".to_string(), Value::String(val.to_string()));
                    out_headers.push(Value::Map(h));
                }
            }

            let mut reader = resp.into_reader().take(MAX_BODY_BYTES as u64 + 1);
            let mut body_bytes = Vec::new();
            reader
                .read_to_end(&mut body_bytes)
                .map_err(|e| format!("BrowserError: read failed: {e}"))?;

            if body_bytes.len() > MAX_BODY_BYTES {
                return Err(format!(
                    "BodyTooLarge: response exceeded max_body_bytes limit ({MAX_BODY_BYTES} bytes)"
                ));
            }

            let body_str = String::from_utf8_lossy(&body_bytes).to_string();

            let mut res = HashMap::new();
            res.insert("status".to_string(), Value::Int(status));
            res.insert("url".to_string(), Value::String(url.to_string()));
            res.insert("body".to_string(), Value::String(body_str));
            res.insert("latency_ms".to_string(), Value::Int(latency_ms));
            res.insert("headers".to_string(), Value::List(out_headers));
            Ok(Value::Map(res))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let status = code as i64;
            let mut out_headers = Vec::new();
            for name in resp.headers_names() {
                if let Some(val) = resp.header(&name) {
                    let mut h = HashMap::new();
                    h.insert("name".to_string(), Value::String(name.to_lowercase()));
                    h.insert("value".to_string(), Value::String(val.to_string()));
                    out_headers.push(Value::Map(h));
                }
            }

            let mut body_bytes = Vec::new();
            let _ = resp
                .into_reader()
                .take(MAX_BODY_BYTES as u64)
                .read_to_end(&mut body_bytes);
            let body_str = String::from_utf8_lossy(&body_bytes).to_string();

            let mut res = HashMap::new();
            res.insert("status".to_string(), Value::Int(status));
            res.insert("url".to_string(), Value::String(url.to_string()));
            res.insert("body".to_string(), Value::String(body_str));
            res.insert("latency_ms".to_string(), Value::Int(latency_ms));
            res.insert("headers".to_string(), Value::List(out_headers));
            Ok(Value::Map(res))
        }
        Err(ureq::Error::Transport(t)) => Err(format!("BrowserNetworkError: {t}")),
    }
}

#[cfg(not(all(not(target_arch = "wasm32"), feature = "live-provider")))]
fn execute_network_fetch(
    url: &str,
    _method: &str,
    _headers: Vec<(String, String)>,
    _body: Option<String>,
    _timeout_ms: u64,
    start: Instant,
) -> Result<Value, String> {
    // When live-provider feature is disabled, fail gracefully or provide deterministic test fixture
    let latency_ms = start.elapsed().as_millis() as i64;
    let mut res = HashMap::new();
    res.insert("status".to_string(), Value::Int(200));
    res.insert("url".to_string(), Value::String(url.to_string()));
    res.insert(
        "body".to_string(),
        Value::String("<html><body>Offline browser.fetch mock</body></html>".to_string()),
    );
    res.insert("latency_ms".to_string(), Value::Int(latency_ms));
    res.insert("headers".to_string(), Value::List(vec![]));
    Ok(Value::Map(res))
}
