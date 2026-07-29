//! Minimal LSP server (design §10, v1.0): `nudgec lsp` speaks JSON-RPC over
//! stdio (Content-Length framing) — full-document sync with
//! `publishDiagnostics` backed by the real lex → parse → check pipeline.
//! v1.1d adds hover, go-to-definition and completion from a per-document
//! symbol index (declarations found by line scan — the spanned AST lands
//! later and will replace the scan without changing the protocol surface).

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
    diag_sev(line, col, code, msg, 1.0)
}

fn diag_sev(line: usize, col: usize, code: &str, msg: &str, severity: f64) -> Json {
    let pos = |l: usize, c: usize| {
        Json::Obj(vec![
            ("line".into(), Json::Num(l as f64)),
            ("character".into(), Json::Num(c as f64)),
        ])
    };
    Json::Obj(vec![
        (
            "range".into(),
            Json::Obj(vec![
                ("start".into(), pos(line, col)),
                ("end".into(), pos(line, col + 1)),
            ]),
        ),
        ("severity".into(), Json::Num(severity)),
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
            Ok(items) => {
                let mut out: Vec<Json> = crate::check::check(&items)
                    .iter()
                    // spanned AST (stage 1): statement-level errors point at
                    // their statement; item-level ones fall back to file start
                    .map(|e| {
                        let (l, c) = e.span.map(|sp| line_col(src, sp.start)).unwrap_or((0, 0));
                        diag(l, c, e.code, &e.msg)
                    })
                    .collect();
                // Prompt Clippy (design §20): W-code warnings surface in the
                // editor as severity-2 diagnostics on clean files too
                if out.is_empty() {
                    out.extend(
                        crate::lint::lint_items(&items)
                            .iter()
                            .map(|l| diag_sev(0, 0, l.code, &l.msg, 2.0)),
                    );
                }
                out
            }
        },
    }
}

fn notify_diagnostics(uri: &str, diags: Vec<Json>) -> Json {
    Json::Obj(vec![
        ("jsonrpc".into(), Json::str("2.0")),
        (
            "method".into(),
            Json::str("textDocument/publishDiagnostics"),
        ),
        (
            "params".into(),
            Json::Obj(vec![
                ("uri".into(), Json::str(uri)),
                ("diagnostics".into(), Json::Arr(diags)),
            ]),
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
            Json::Obj(vec![
                ("code".into(), Json::Num(code)),
                ("message".into(), Json::str(msg)),
            ]),
        ),
    ])
}

pub struct Lsp {
    docs: HashMap<String, String>,
    shutdown: bool,
}

// ---- v1.1d: symbol index + editor features -------------------------------

/// A declared symbol found by the line scan (name, kind, 0-based position,
/// and the full declaration line for hover display).
struct Sym {
    kind: &'static str,
    name: String,
    line: usize,
    col: usize,
    detail: String,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Index top-level-ish declarations: `fn`/`type`/`agent`/`tool`/`let` NAME.
/// A line scan (not the AST) — good enough for editor navigation and robust
/// against partial/broken input, which is the common case while typing.
fn index(src: &str) -> Vec<Sym> {
    let mut out = Vec::new();
    for (line_no, raw) in src.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") {
            continue;
        }
        let indent = raw.len() - line.len();
        for (kw, kind) in [
            ("fn ", "function"),
            ("type ", "type"),
            ("agent ", "agent"),
            ("tool ", "tool"),
            ("let ", "variable"),
        ] {
            if let Some(rest) = line.strip_prefix(kw) {
                let name: String = rest.chars().take_while(|&c| is_ident_char(c)).collect();
                if !name.is_empty() && is_ident_start(name.chars().next().unwrap()) {
                    let col = indent + kw.len();
                    let detail = line.trim_end().trim_end_matches('{').trim_end().to_string();
                    out.push(Sym {
                        kind,
                        name,
                        line: line_no,
                        col,
                        detail,
                    });
                }
            }
        }
    }
    out
}

/// The identifier under an LSP position (0-based line/character).
fn word_at(src: &str, line: usize, col: usize) -> Option<String> {
    let text = src.lines().nth(line)?;
    let chars: Vec<char> = text.chars().collect();
    if col > chars.len() {
        return None;
    }
    let mut start = col.min(chars.len().saturating_sub(1));
    let mut end = col;
    while start > 0 && is_ident_char(chars[start - 1]) {
        start -= 1;
    }
    while end < chars.len() && is_ident_char(chars[end]) {
        end += 1;
    }
    if start >= end {
        return None;
    }
    let w: String = chars[start..end].iter().collect();
    if is_ident_start(w.chars().next()?) {
        Some(w)
    } else {
        None
    }
}

/// One-line docs for reserved + contextual keywords (design §12).
const KEYWORD_DOCS: &[(&str, &str)] = &[
    ("fn", "Declare a function. `fn name(args) -> ret uses LLM { … }` — effects listed after `uses`."),
    ("let", "Bind a value: `let x = expr`."),
    ("type", "Declare a named type: `type Name = { field: string }` or an alias with constraints (`@format(url)`)."),
    ("tool", "Declare an external tool binding."),
    ("agent", "An `agent` block with checkpointed `state` (design §7)."),
    ("state", "Declare checkpointed agent state (design §7)."),
    ("uses", "Effect list on a signature: `uses LLM`, `uses Tool`, `uses IO` (design §3)."),
    ("with", "Call options on an llm call: `with { model: …, schema: …, budget: … USD }`."),
    ("par", "Parallel combinator: `par map`, `par race`, `par all` with compile-time race safety."),
    ("map", "`par map(items, concurrency = n) |x| -> f(x)` — bounded-concurrency parallel map."),
    ("all", "`par all [f(), g()]` — run all, wait for all."),
    ("race", "`par race [f(), g()]` — first result wins."),
    ("test", "A replayable test block with `assert`s — runs from a trace at zero token cost."),
    ("assert", "`assert cond` — test/runtime assertion."),
    ("export", "Export a declaration from the module."),
    ("use", "Import from another module."),
    ("LLM", "The LLM effect — required on signatures that make llm calls (E0301/E0302)."),
    ("schema", "`with`-key: the output type of an llm call. Violations trigger automatic repair."),
    ("model", "`with`-key: model name; `provider:model` prefix picks a real provider (design §4.6)."),
    ("budget", "`with`-key: USD ceiling for the call, enforced at runtime (E0501)."),
    ("retry", "`with`-key: repair-loop retry policy for schema violations."),
    ("concurrency", "`par map` bound: max in-flight calls."),
    ("replay", "Replay mode: full / `NUDGE_REPLAY_MODE=llm` hybrid / live (design §6.1)."),
    ("zip", "Zip two lists pairwise: `par map(a zip b, …) |(x, y)| -> …`."),
    ("merge", "Reducer join `l | merge r` — dict union, list append-dedup (design §7)."),
];

/// Completion items: keywords, primitive types, `with`-keys, index symbols.
fn completions(src: &str) -> Json {
    let mut items = Vec::new();
    let mut push = |label: &str, kind: f64, detail: &str| {
        items.push(Json::Obj(vec![
            ("label".into(), Json::str(label)),
            ("kind".into(), Json::Num(kind)),
            ("detail".into(), Json::str(detail)),
        ]));
    };
    for (kw, doc) in KEYWORD_DOCS {
        push(kw, 14.0, doc); // 14 = Keyword
    }
    for t in ["string", "float", "int", "bool"] {
        push(t, 25.0, "primitive type"); // 25 = TypeParameter
    }
    for s in index(src) {
        let kind = match s.kind {
            "function" => 3.0,
            "type" => 7.0,
            "agent" => 7.0,
            "tool" => 3.0,
            _ => 6.0,
        };
        push(&s.name.clone(), kind, &s.detail.clone());
    }
    Json::Obj(vec![
        ("isIncomplete".into(), Json::Bool(false)),
        ("items".into(), Json::Arr(items)),
    ])
}

fn hover(src: &str, line: usize, col: usize) -> Json {
    let Some(word) = word_at(src, line, col) else {
        return Json::Null;
    };
    let md = |v: String| {
        Json::Obj(vec![(
            "contents".into(),
            Json::Obj(vec![
                ("kind".into(), Json::str("markdown")),
                ("value".into(), Json::str(v)),
            ]),
        )])
    };
    if let Some(s) = index(src).iter().find(|s| s.name == word) {
        return md(format!(
            "```nudge\n{}\n```\n*{}* — declared on line {}",
            s.detail,
            s.kind,
            s.line + 1
        ));
    }
    if ["string", "float", "int", "bool"].contains(&word.as_str()) {
        return md(format!("`{word}` — primitive type"));
    }
    if let Some((_, doc)) = KEYWORD_DOCS.iter().find(|(kw, _)| *kw == word) {
        return md(format!("`{word}` — {doc}"));
    }
    Json::Null
}

fn definition(src: &str, uri: &str, line: usize, col: usize) -> Json {
    let Some(word) = word_at(src, line, col) else {
        return Json::Null;
    };
    let Some(s) = index(src).into_iter().find(|s| s.name == word) else {
        return Json::Null;
    };
    let pos = |l: usize, c: usize| {
        Json::Obj(vec![
            ("line".into(), Json::Num(l as f64)),
            ("character".into(), Json::Num(c as f64)),
        ])
    };
    Json::Obj(vec![
        ("uri".into(), Json::str(uri)),
        (
            "range".into(),
            Json::Obj(vec![
                ("start".into(), pos(s.line, s.col)),
                ("end".into(), pos(s.line, s.col + s.name.len())),
            ]),
        ),
    ])
}

impl Lsp {
    pub fn new() -> Self {
        Lsp {
            docs: HashMap::new(),
            shutdown: false,
        }
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
                            Json::Obj(vec![
                                ("textDocumentSync".into(), Json::Num(1.0)),
                                ("hoverProvider".into(), Json::Bool(true)),
                                ("definitionProvider".into(), Json::Bool(true)),
                                (
                                    "completionProvider".into(),
                                    Json::Obj(vec![(
                                        "triggerCharacters".into(),
                                        Json::Arr(vec![]),
                                    )]),
                                ),
                            ]),
                        )]),
                    )]
                })
                .unwrap_or_default(),
            "initialized" | "$/cancelRequest" => vec![],
            "shutdown" => {
                self.shutdown = true;
                id.map(|i| vec![respond(&i, Json::Null)])
                    .unwrap_or_default()
            }
            "exit" => std::process::exit(if self.shutdown { 0 } else { 1 }),
            "textDocument/didOpen" => {
                let doc = params.get("textDocument").cloned().unwrap_or(Json::Null);
                let uri = doc
                    .get("uri")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let text = doc
                    .get("text")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
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
            // v1.1d: hover / go-to-definition / completion from the index
            "textDocument/hover" | "textDocument/definition" | "textDocument/completion" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                let line = params
                    .get("position")
                    .and_then(|p| p.get("line"))
                    .and_then(Json::as_num)
                    .unwrap_or(0.0) as usize;
                let col = params
                    .get("position")
                    .and_then(|p| p.get("character"))
                    .and_then(Json::as_num)
                    .unwrap_or(0.0) as usize;
                let Some(src) = self.docs.get(&uri) else {
                    return match id {
                        Some(i) => vec![respond(&i, Json::Null)],
                        None => vec![],
                    };
                };
                let result = match method {
                    "textDocument/hover" => hover(src, line, col),
                    "textDocument/definition" => definition(src, &uri, line, col),
                    _ => completions(src),
                };
                match id {
                    Some(i) => vec![respond(&i, result)],
                    None => vec![],
                }
            }
            // requests we don't implement get a JSON-RPC error; unknown
            // notifications are ignored per the LSP spec
            _ => match id {
                Some(i) => vec![respond_error(
                    &i,
                    -32601.0,
                    &format!("method not implemented: {method}"),
                )],
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
        assert!(
            s.contains("\"id\": 1") && s.contains("\"textDocumentSync\": 1"),
            "{s}"
        );
        let out2 = lsp.dispatch(&request(2, "shutdown", Json::Null));
        assert!(dumps(&out2[0]).contains("\"result\": null"));
        // unimplemented request → JSON-RPC error
        let out3 = lsp.dispatch(&request(3, "textDocument/references", Json::Obj(vec![])));
        assert!(dumps(&out3[0]).contains("-32601"));
    }

    const DOC: &str = "type Finding = { claim: string, confidence: float }\nfn find(q: string) -> Finding uses LLM {\n    let x = 1\n}\nfn main() -> string { find(\"a\").claim }\n";

    fn pos_params(uri: &str, line: usize, col: usize) -> Json {
        Json::Obj(vec![
            (
                "textDocument".into(),
                Json::Obj(vec![("uri".into(), Json::str(uri))]),
            ),
            (
                "position".into(),
                Json::Obj(vec![
                    ("line".into(), Json::Num(line as f64)),
                    ("character".into(), Json::Num(col as f64)),
                ]),
            ),
        ])
    }

    fn open_doc(lsp: &mut Lsp, uri: &str, text: &str) {
        lsp.dispatch(&Json::Obj(vec![
            ("jsonrpc".into(), Json::str("2.0")),
            ("method".into(), Json::str("textDocument/didOpen")),
            (
                "params".into(),
                Json::Obj(vec![(
                    "textDocument".into(),
                    Json::Obj(vec![
                        ("uri".into(), Json::str(uri)),
                        ("text".into(), Json::str(text)),
                    ]),
                )]),
            ),
        ]));
    }

    #[test]
    fn index_finds_declarations() {
        let syms = index(DOC);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Finding"), "{names:?}");
        assert!(names.contains(&"find"), "{names:?}");
        assert!(names.contains(&"main"), "{names:?}");
        assert!(names.contains(&"x"), "{names:?}");
        let f = syms.iter().find(|s| s.name == "find").unwrap();
        assert_eq!((f.line, f.col), (1, 3));
        assert_eq!(f.kind, "function");
    }

    #[test]
    fn word_at_extracts_identifier() {
        assert_eq!(word_at("fn find(q) { 0 }", 0, 4).as_deref(), Some("find"));
        assert_eq!(word_at("fn find(q) { 0 }", 0, 6).as_deref(), Some("find"));
        assert_eq!(word_at("   ", 0, 1), None);
    }

    #[test]
    fn hover_definition_completion_work() {
        let mut lsp = Lsp::new();
        let uri = "file:///t.ndg";
        open_doc(&mut lsp, uri, DOC);
        // initialize advertises the new capabilities
        let init = lsp.dispatch(&request(1, "initialize", Json::Obj(vec![])));
        let s = dumps(&init[0]);
        assert!(
            s.contains("hoverProvider")
                && s.contains("definitionProvider")
                && s.contains("completionProvider"),
            "{s}"
        );
        // hover over `find` call site (line 4, col 26) → signature markdown
        let h = lsp.dispatch(&request(2, "textDocument/hover", pos_params(uri, 4, 26)));
        let s = dumps(&h[0]);
        assert!(s.contains("fn find(q: string) -> Finding uses LLM"), "{s}");
        // hover over a keyword
        let h2 = lsp.dispatch(&request(3, "textDocument/hover", pos_params(uri, 0, 1)));
        assert!(
            dumps(&h2[0]).contains("Declare a named type"),
            "{}",
            dumps(&h2[0])
        );
        // go-to-definition on `find` → line 1, col 3
        let d = lsp.dispatch(&request(
            4,
            "textDocument/definition",
            pos_params(uri, 4, 26),
        ));
        let s = dumps(&d[0]);
        assert!(
            s.contains(uri) && s.contains("\"line\": 1") && s.contains("\"character\": 3"),
            "{s}"
        );
        // completion lists keywords + user symbols
        let c = lsp.dispatch(&request(
            5,
            "textDocument/completion",
            pos_params(uri, 4, 26),
        ));
        let s = dumps(&c[0]);
        assert!(
            s.contains("\"label\": \"schema\"")
                && s.contains("\"label\": \"find\"")
                && s.contains("\"label\": \"Finding\""),
            "{s}"
        );
        // hover over nothing meaningful → null
        let n = lsp.dispatch(&request(6, "textDocument/hover", pos_params(uri, 2, 14)));
        assert!(
            dumps(&n[0]).contains("\"result\": null"),
            "{}",
            dumps(&n[0])
        );
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
    fn prompt_clippy_warnings_reach_the_editor() {
        // a type-correct file with prompt smells → severity-2 diagnostics
        let diags = diagnostics("fn analyze() -> string uses LLM {\n    llm\"\"\"do it\"\"\" with { model: \"fake\" }\n}");
        assert_eq!(diags.len(), 2, "{diags:?}");
        let s = dumps(&Json::Arr(diags));
        assert!(s.contains("\"severity\": 2"), "{s}");
        assert!(s.contains("W0001") && s.contains("W0002"), "{s}");
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
                        (
                            "textDocument".into(),
                            Json::Obj(vec![("uri".into(), Json::str(uri))]),
                        ),
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

