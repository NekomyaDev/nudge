//! Trace viewer (design §6 follow-up, v1.2): `nudgec trace-view <trace.jsonl>`
//! serves a self-contained local web UI over a frozen-v1 trace — timeline of
//! every llm.call / tool.call / fn.return record, token + cost stats, repair
//! highlighting, and a detail pane per record. Zero dependencies: the server
//! is a stdlib TcpListener, the UI is a single embedded HTML file.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process;

const INDEX_HTML: &str = include_str!("traceview.html");

pub const DEFAULT_PORT: u16 = 8321;

fn open_browser(url: &str) {
    let (cmd, arg): (&str, &str) = if cfg!(target_os = "macos") {
        ("open", url)
    } else if cfg!(target_os = "windows") {
        ("cmd", url)
    } else {
        ("xdg-open", url)
    };
    let result = if cfg!(target_os = "windows") {
        process::Command::new(cmd).args(["/c", "start", "", arg]).spawn()
    } else {
        process::Command::new(cmd).arg(arg).spawn()
    };
    if result.is_err() {
        eprintln!("note: could not open a browser — visit {url} manually");
    }
}

fn http_response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// Serve the viewer until Ctrl-C. Validates the trace first (frozen v1
/// schema) — a trace full of schema errors is reported on stderr and the
/// server refuses to start, matching `trace-check` behaviour.
pub fn run(trace_path: &str, trace_src: &str, port: u16, no_open: bool) -> ! {
    let problems = crate::tracecheck::validate(trace_src);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("error: {p}");
        }
        eprintln!("error: trace does not conform to the frozen v1 schema — fix it or run `nudgec trace-check` for details");
        process::exit(1);
    }

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind 127.0.0.1:{port}: {e}");
            process::exit(1);
        }
    };
    let url = format!("http://127.0.0.1:{port}");
    eprintln!("serving {trace_path} at {url}  (Ctrl-C to stop)");
    if !no_open {
        open_browser(&url);
    }

    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let mut buf = [0u8; 4096];
        let n = match s.read(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/");
        let resp = match path {
            "/trace" => http_response("200 OK", "application/x-ndjson; charset=utf-8", trace_src),
            "/" | "/index.html" => http_response("200 OK", "text/html; charset=utf-8", INDEX_HTML),
            _ => http_response("404 Not Found", "text/plain; charset=utf-8", "not found"),
        };
        let _ = s.write_all(&resp);
        let _ = s.flush();
    }
    process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_response_has_status_type_and_length() {
        let r = String::from_utf8(http_response("200 OK", "text/plain", "hello")).unwrap();
        assert!(r.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(r.contains("content-type: text/plain\r\n"));
        assert!(r.contains("content-length: 5\r\n"));
        assert!(r.ends_with("hello"));
    }

    #[test]
    fn viewer_html_is_embedded_and_self_contained() {
        assert!(INDEX_HTML.contains("Nudge Trace Viewer"));
        assert!(INDEX_HTML.contains("/trace"));
        // no external assets — the page must work fully offline
        assert!(!INDEX_HTML.contains("https://"));
        assert!(!INDEX_HTML.contains("<script src"));
        assert!(!INDEX_HTML.contains("link rel"));
    }
}
