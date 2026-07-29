//! Minimal Debug Adapter Protocol server (roadmap v1.2 — DAP groundwork
//! spike): `nudgec debug <trace.jsonl>` speaks DAP over stdio with
//! Content-Length framing, same as the LSP module. The "program" under
//! debug is a *recorded run*: set breakpoints on trace record seq numbers,
//! step record-by-record through llm.call / tool.call / fn.return events,
//! and inspect the current record's fields as variables. Time-travel over
//! the trace at zero token cost (design §6) — the live-run attach lands
//! with the spanned AST; the protocol surface stays the same.

use crate::json::{dumps, parse as parse_json, Json};
use std::io::{Read, Write};

pub struct Dap {
    records: Vec<Json>,
    /// index of the record the session is paused on
    pos: usize,
    /// seq numbers the client broke on
    breakpoints: Vec<f64>,
    configured: bool,
    terminated: bool,
    /// outgoing message counter (DAP wants monotonically increasing seq)
    out_seq: i64,
}

fn num(rec: &Json, key: &str) -> f64 {
    rec.get(key).and_then(Json::as_num).unwrap_or(0.0)
}

fn kind_of(rec: &Json) -> &str {
    rec.get("kind").and_then(Json::as_str).unwrap_or("?")
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

impl Dap {
    pub fn new(trace_src: &str) -> Dap {
        let records = trace_src
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| parse_json(l).ok())
            .collect();
        Dap {
            records,
            pos: 0,
            breakpoints: Vec::new(),
            configured: false,
            terminated: false,
            out_seq: 0,
        }
    }

    fn next_seq(&mut self) -> f64 {
        self.out_seq += 1;
        self.out_seq as f64
    }

    fn respond(&mut self, req: &Json, body: Json) -> Json {
        obj(vec![
            ("seq", Json::Num(self.next_seq())),
            ("type", Json::str("response")),
            ("request_seq", Json::Num(num(req, "seq"))),
            ("success", Json::Bool(true)),
            ("command", Json::str(
                req.get("command").and_then(Json::as_str).unwrap_or(""),
            )),
            ("body", body),
        ])
    }

    fn event(&mut self, name: &str, body: Json) -> Json {
        obj(vec![
            ("seq", Json::Num(self.next_seq())),
            ("type", Json::str("event")),
            ("event", Json::str(name)),
            ("body", body),
        ])
    }

    fn stopped(&mut self, reason: &str) -> Json {
        self.event(
            "stopped",
            obj(vec![
                ("reason", Json::str(reason)),
                ("threadId", Json::Num(1.0)),
                ("allThreadsStopped", Json::Bool(true)),
            ]),
        )
    }

    fn current(&self) -> Option<&Json> {
        self.records.get(self.pos)
    }

    /// After configurationDone: pause on the first record (entry) — or on
    /// the first breakpoint if one was set before launch.
    fn entry_stop(&mut self) -> Json {
        if let Some(idx) = self
            .records
            .iter()
            .position(|r| self.breakpoints.contains(&num(r, "seq")))
        {
            self.pos = idx;
            return self.stopped("breakpoint");
        }
        self.pos = 0;
        self.stopped("entry")
    }

    fn variables(&self) -> Json {
        let vars = match self.current() {
            Some(Json::Obj(fields)) => fields
                .iter()
                .map(|(k, v)| {
                    obj(vec![
                        ("name", Json::str(k)),
                        ("value", Json::str(dumps(v))),
                        ("variablesReference", Json::Num(0.0)),
                    ])
                })
                .collect(),
            _ => Vec::new(),
        };
        Json::Arr(vars)
    }

    pub fn dispatch(&mut self, req: &Json) -> Vec<Json> {
        let command = req.get("command").and_then(Json::as_str).unwrap_or("");
        let args = req.get("arguments").cloned().unwrap_or(Json::Null);
        match command {
            "initialize" => vec![
                self.respond(
                    req,
                    obj(vec![
                        ("supportsConfigurationDoneRequest", Json::Bool(true)),
                        ("supportsSetVariable", Json::Bool(false)),
                    ]),
                ),
                self.event("initialized", Json::Obj(vec![])),
            ],
            "launch" | "attach" => vec![self.respond(req, Json::Obj(vec![]))],
            "setBreakpoints" => {
                self.breakpoints = match args.get("breakpoints") {
                    Some(Json::Arr(bps)) => bps
                        .iter()
                        .filter_map(|b| b.get("line").and_then(Json::as_num))
                        .collect(),
                    _ => Vec::new(),
                };
                let verified: Vec<Json> = self
                    .breakpoints
                    .iter()
                    .map(|l| {
                        obj(vec![
                            ("verified", Json::Bool(true)),
                            ("line", Json::Num(*l)),
                        ])
                    })
                    .collect();
                vec![self.respond(
                    req,
                    obj(vec![("breakpoints", Json::Arr(verified))]),
                )]
            }
            "configurationDone" => {
                self.configured = true;
                let resp = self.respond(req, Json::Obj(vec![]));
                vec![resp, self.entry_stop()]
            }
            "threads" => vec![self.respond(
                req,
                obj(vec![(
                    "threads",
                    Json::Arr(vec![obj(vec![
                        ("id", Json::Num(1.0)),
                        ("name", Json::str("trace replay")),
                    ])]),
                )]),
            )],
            "stackTrace" => {
                let frame = match self.current() {
                    Some(rec) => obj(vec![
                        ("id", Json::Num(1.0)),
                        (
                            "name",
                            Json::str(format!("{} seq {}", kind_of(rec), num(rec, "seq"))),
                        ),
                        ("line", Json::Num(num(rec, "seq"))),
                        ("column", Json::Num(1.0)),
                    ]),
                    None => obj(vec![
                        ("id", Json::Num(1.0)),
                        ("name", Json::str("<end of trace>")),
                        ("line", Json::Num(0.0)),
                        ("column", Json::Num(0.0)),
                    ]),
                };
                vec![self.respond(
                    req,
                    obj(vec![
                        ("stackFrames", Json::Arr(vec![frame])),
                        ("totalFrames", Json::Num(1.0)),
                    ]),
                )]
            }
            "scopes" => vec![self.respond(
                req,
                obj(vec![(
                    "scopes",
                    Json::Arr(vec![obj(vec![
                        ("name", Json::str("record")),
                        ("variablesReference", Json::Num(1000.0)),
                        ("expensive", Json::Bool(false)),
                    ])]),
                )]),
            )],
            "variables" => vec![self.respond(
                req,
                obj(vec![("variables", self.variables())]),
            )],
            "next" => {
                let resp = self.respond(req, Json::Obj(vec![]));
                self.pos += 1;
                if self.pos >= self.records.len() {
                    self.terminated = true;
                    vec![resp, self.event("terminated", Json::Obj(vec![]))]
                } else {
                    vec![resp, self.stopped("step")]
                }
            }
            "continue" => {
                let resp = self.respond(req, Json::Obj(vec![]));
                let hit = self
                    .records
                    .iter()
                    .enumerate()
                    .skip(self.pos + 1)
                    .find(|(_, r)| self.breakpoints.contains(&num(r, "seq")))
                    .map(|(i, _)| i);
                match hit {
                    Some(idx) => {
                        self.pos = idx;
                        vec![resp, self.stopped("breakpoint")]
                    }
                    None => {
                        self.terminated = true;
                        vec![resp, self.event("terminated", Json::Obj(vec![]))]
                    }
                }
            }
            "disconnect" | "terminate" => {
                self.terminated = true;
                vec![self.respond(req, Json::Obj(vec![]))]
            }
            _ => vec![{
                let mut r = self.respond(req, Json::Obj(vec![]));
                if let Json::Obj(fields) = &mut r {
                    for (k, v) in fields.iter_mut() {
                        if k == "success" {
                            *v = Json::Bool(false);
                        }
                    }
                    fields.push((
                        "message".into(),
                        Json::str(format!("command not implemented: {command}")),
                    ));
                }
                r
            }],
        }
    }

    pub fn is_terminated(&self) -> bool {
        self.terminated
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

/// stdio loop: read DAP requests, dispatch, write responses + events until
/// the client disconnects or EOF.
pub fn run(trace_src: &str) {
    let mut dap = Dap::new(trace_src);
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    while let Some(body) = read_frame(&mut input) {
        match parse_json(&body) {
            Ok(msg) => {
                for out in dap.dispatch(&msg) {
                    write_frame(&mut output, &out);
                }
                if dap.is_terminated() {
                    return;
                }
            }
            Err(e) => {
                eprintln!("dap: bad frame: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = r#"{"v": 1, "seq": 1, "kind": "llm.call", "model": "m", "outcome": "ok"}
{"v": 1, "seq": 2, "kind": "tool.call", "tool": "web"}
{"v": 1, "seq": 3, "kind": "fn.return", "fn": "main", "output": "done"}"#;

    fn request(seq: i64, command: &str, arguments: Json) -> Json {
        obj(vec![
            ("seq", Json::Num(seq as f64)),
            ("type", Json::str("request")),
            ("command", Json::str(command)),
            ("arguments", arguments),
        ])
    }

    #[test]
    fn full_session_steps_to_the_end() {
        let mut dap = Dap::new(TRACE);
        let out = dap.dispatch(&request(1, "initialize", Json::Obj(vec![])));
        assert!(dumps(&out[0]).contains("supportsConfigurationDoneRequest"));
        assert!(dumps(&out[1]).contains("\"event\": \"initialized\""));

        let out = dap.dispatch(&request(2, "configurationDone", Json::Obj(vec![])));
        assert!(dumps(&out[1]).contains("\"reason\": \"entry\""));

        // stack frame names the first record
        let out = dap.dispatch(&request(3, "stackTrace", Json::Obj(vec![])));
        assert!(dumps(&out[0]).contains("llm.call seq 1"), "{}", dumps(&out[0]));

        // variables expose the record's fields
        let out = dap.dispatch(&request(4, "variables", Json::Obj(vec![])));
        let s = dumps(&out[0]);
        assert!(s.contains("\"name\": \"model\"") && s.contains("\"name\": \"outcome\""), "{s}");

        // step to the end → terminated
        dap.dispatch(&request(5, "next", Json::Obj(vec![])));
        dap.dispatch(&request(6, "next", Json::Obj(vec![])));
        let out = dap.dispatch(&request(7, "next", Json::Obj(vec![])));
        assert!(dumps(&out[1]).contains("\"event\": \"terminated\""));
        assert!(dap.is_terminated());
    }

    #[test]
    fn continue_lands_on_breakpoints() {
        let mut dap = Dap::new(TRACE);
        dap.dispatch(&request(1, "initialize", Json::Obj(vec![])));
        let bps = obj(vec![(
            "breakpoints",
            Json::Arr(vec![obj(vec![("line", Json::Num(3.0))])]),
        )]);
        let out = dap.dispatch(&request(2, "setBreakpoints", bps));
        assert!(dumps(&out[0]).contains("\"verified\": true"));
        // entry stop honours the pre-set breakpoint
        let out = dap.dispatch(&request(3, "configurationDone", Json::Obj(vec![])));
        assert!(dumps(&out[1]).contains("\"reason\": \"breakpoint\""));
        let out = dap.dispatch(&request(4, "stackTrace", Json::Obj(vec![])));
        assert!(dumps(&out[0]).contains("fn.return seq 3"));
        // continuing past the last breakpoint terminates
        let out = dap.dispatch(&request(5, "continue", Json::Obj(vec![])));
        assert!(dumps(&out[1]).contains("\"event\": \"terminated\""));
    }

    #[test]
    fn unknown_commands_fail_cleanly() {
        let mut dap = Dap::new(TRACE);
        let out = dap.dispatch(&request(1, "evaluate", Json::Obj(vec![])));
        let s = dumps(&out[0]);
        assert!(s.contains("\"success\": false") && s.contains("not implemented"), "{s}");
    }
}
