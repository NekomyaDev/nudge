//! Minimal LSP server (design §10, v1.0): `nudgec lsp` speaks JSON-RPC over
//! stdio (Content-Length framing) — full-document sync with
//! `publishDiagnostics` backed by the real lex → parse → check pipeline.
//! Dependency-free; hover/completion land post-v1.0.

use crate::json::{dumps, parse as parse_json, Json};
use std::collections::HashMap;
use std::io::{Read, Write};

fn line_col(src: &str, at: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for ch in src[..at.min(src.len())].chars() {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn diag(line: usize, col: usize, code: &str, msg: &str) -> Json {
    let pos = |l: usize, c: usize| Json::Obj(vec![("line".into(), Json::Num(l as f64)), ("character".into(), Json::Num(c as f64))]);
    Json::Obj(vec![
        (
            "range".into(),
            Json::Obj(vec![("start".into(), pos(line, col)), ("end".into(), pos(line, col + 1))]),
        ),
        ("severity".into(), Json::Num(1.0)),
        ("source".into(), Json::str("nudge")),
        ("code".into(), Json::str(code)),
        ("message".into(), Json::str(msg)),
    ])
}

/// The full pipeline as LSP diagnostics (0-based positions, as LSP expects).
pub fn diagnostics(src: &str) -> Vec<Json> {
    match crate::lexer::lex(src) {
        Err(e) => {
            let (l, c) = line_col(src, e.at);
            vec![diag(l, c, "E0001", &e.msg)]
        }
        Ok(tokens) => match crate::parser::parse(tokens) {
            Err(e) => {
                let (l, c) = line_col(src, e.at);
                vec![diag(l, c, "E0002", &e.msg)]
            }
            Ok(items) => crate::check::check(&items)
                .iter()
                // check errors carry no span yet (spanned AST is post-MVP) —
                // point at the file start with the stable code attached
                .map(|e| diag(0, 0, e.code, &e.msg))
                .collect(),
        },
    }
}

fn notify_diagnostics(uri: &str, diags: Vec<Json>) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::str("2.0")),
        ("method".into(), Json::str("textDocument/publishDiagnostics")),
        (
            "params".into(),
            Json::Obj(vec![("uri".into(), Json::str(uri)), ("diagnostics".into(), Json::Arr(diags))]),
        ),
    ])
}

fn respond(id: &Json, result: Json) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::str("2.0")),
        ("id".into(), id.clone()),
        ("result".into(), result),
    ])
}

fn respond_error(id: &Json, code: f64, msg: &str) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::str("2.0")),
        ("id".into(), id.clone()),
        (
            "error".into(),
            Json::Obj(vec![("code".into(), Json::Num(code)), ("message".into(), Json::str(msg))]),
        ),
    ])
}

pub struct Lsp {
    docs: HashMap<String, String>,
    shutdown: bool,
}

impl Lsp {
    pub fn new() -> Self {
        Lsp { docs: HashMap::new(), shutdown: false }
    }

    /// Handle one incoming message; returns the messages to send back.
    pub fn dispatch(&mut self, msg: &Json) -> Vec<Json> {
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(Json::Null);
        match method {
            "initialize" => id
                .map(|i| {
                    vec![respond(
                        &i,
                        Json::Obj(vec![(
                            "capabilities".into(),
                            Json::Obj(vec![("textDocumentSync".into(), Json::Num(1.0))]),
                        )]),
                    )]
                })
                .unwrap_or_default(),
            "initialized" | "$/cancelRequest" => vec![],
            "shutdown" => {
                self.shutdown = true;
                id.map(|i| vec![respond(&i, Json::Null)]).unwrap_or_default()
            }
            "exit" => std::process::exit(if self.shutdown { 0 } else { 1 }),
            "textDocument/didOpen" => {
                let doc = params.get("textDocument").cloned().unwrap_or(Json::Null);
                let uri = doc.get("uri").and_then(Json::as_str).unwrap_or("").to_string();
                let text = doc.get("text").and_then(Json::as_str).unwrap_or("").to_string();
                let diags = diagnostics(&text);
                self.docs.insert(uri.clone(), text);
                vec![notify_diagnostics(&uri, diags)]
            }
            "textDocument/didChange" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                // full sync: the single content change carries the whole text
                let text = params
                    .get("contentChanges")
                    .and_then(|c| c.idx(0))
                    .and_then(|c| c.get("text"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let diags = diagnostics(&text);
                self.docs.insert(uri.clone(), text);
                vec![notify_diagnostics(&uri, diags)]
            }
            "textDocument/didClose" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                self.docs.remove(&uri);
                vec![notify_diagnostics(&uri, vec![])]
            }
            // requests we don't implement get a JSON-RPC error; unknown
            // notifications are ignored per the LSP spec
            _ => match id {
                Some(i) => vec![respond_error(&i, -32601.0, &format!("method not implemented: {method}"))],
                None => vec![],
            },
        }
    }
}

/// Read one Content-Length-framed message; None on clean EOF.
fn read_frame(input: &mut impl Read) -> Option<String> {
    let mut byte = [0u8; 1];
    let mut header = String::new();
    loop {
        match input.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                header.push(byte[0] as char);
                if header.ends_with("\r\n\r\n") {
                    break;
                }
                if header.len() > 4096 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    let len = header
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())?;
    let mut body = vec![0u8; len];
    input.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

fn write_frame(output: &mut impl Write, msg: &Json) {
    let body = dumps(msg);
    let _ = write!(output, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = output.flush();
}

/// stdio loop: read frames, dispatch, write responses until EOF / exit.
pub fn run() {
    let mut lsp = Lsp::new();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    while let Some(body) = read_frame(&mut input) {
        match parse_json(&body) {
            Ok(msg) => {
                for out in lsp.dispatch(&msg) {
                    write_frame(&mut output, &out);
                }
            }
            Err(e) => {
                write_frame(
                    &mut output,
                    &respond_error(&Json::Null, -32700.0, &format!("parse error: {e}")),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: i64, method: &str, params: Json) -> Json {
        Json::Obj(vec![
            ("jsonrpc".into(), Json::str("2.0")),
            ("id".into(), Json::Num(id as f64)),
            ("method".into(), Json::str(method)),
            ("params".into(), params),
        ])
    }

    #[test]
    fn initialize_and_shutdown_answer() {
        let mut lsp = Lsp::new();
        let out = lsp.dispatch(&request(1, "initialize", Json::Obj(vec![])));
        let s = dumps(&out[0]);
        assert!(s.contains("\"id\": 1") && s.contains("\"textDocumentSync\": 1"), "{s}");
        let out2 = lsp.dispatch(&request(2, "shutdown", Json::Null));
        assert!(dumps(&out2[0]).contains("\"result\": null"));
        // unimplemented request → JSON-RPC error
        let out3 = lsp.dispatch(&request(3, "textDocument/hover", Json::Obj(vec![])));
        assert!(dumps(&out3[0]).contains("-32601"));
    }

    #[test]
    fn did_open_publishes_real_diagnostics() {
        let mut lsp = Lsp::new();
        // unknown type name → E0101 from the checker
        let params = Json::Obj(vec![(
            "textDocument".into(),
            Json::Obj(vec![
                ("uri".into(), Json::str("file:///t.ndg")),
                ("text".into(), Json::str("fn f() -> Strnig { 0 }")),
            ]),
        )]);
        let out = lsp.dispatch(&Json::Obj(vec![
            ("jsonrpc".into(), Json::str("2.0")),
            ("method".into(), Json::str("textDocument/didOpen")),
            ("params".into(), params),
        ]));
        let s = dumps(&out[0]);
        assert!(s.contains("textDocument/publishDiagnostics"), "{s}");
        assert!(s.contains("E0101"), "{s}");
        assert!(s.contains("file:///t.ndg"), "{s}");
        // a parse error points at its byte position as line/character
        let diags = diagnostics("fn broken( {\n");
        assert_eq!(diags.len(), 1);
        assert!(dumps(&diags[0]).contains("E0002"), "{diags:?}");
    }

    #[test]
    fn did_change_full_sync_rechecks() {
        let mut lsp = Lsp::new();
        let change = |uri: &str, text: &str| {
            Json::Obj(vec![
                ("jsonrpc".into(), Json::str("2.0")),
                ("method".into(), Json::str("textDocument/didChange")),
                (
                    "params".into(),
                    Json::Obj(vec![
                        ("textDocument".into(), Json::Obj(vec![("uri".into(), Json::str(uri))])),
                        (
                            "contentChanges".into(),
                            Json::Arr(vec![Json::Obj(vec![("text".into(), Json::str(text))])]),
                        ),
                    ]),
                ),
            ])
        };
        let bad = lsp.dispatch(&change("file:///t.ndg", "fn f() -> Strnig { 0 }"));
        assert!(dumps(&bad[0]).contains("E0101"));
        let good = lsp.dispatch(&change("file:///t.ndg", "fn f() -> string { \"ok\" }"));
        let s = dumps(&good[0]);
        assert!(s.contains("\"diagnostics\": []"), "{s}");
    }
}
