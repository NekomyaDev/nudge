//! Computer use (v1.5, docs/computer-use.md) — native VM host functions.
//!
//! `computer.observe(app)` / `computer.click(target)` / … route to the
//! configured provider: the deterministic fake desktop (default) or a
//! subprocess JSONL bridge via `NUDGE_COMPUTER_SERVERS` +
//! `NUDGE_COMPUTER_PROVIDER`. Options (`allow`, `deadline`, `screenshot`)
//! are kwargs in the Python/TS runtimes; the VM call syntax has no kwargs
//! yet, so the VM surface takes positional arguments only.
//!
//! Bridge wire contract (one request per line on stdin, one response per
//! line on stdout, matched by id) — docs/computer-use.md §3.

use super::Value;
use crate::json::{parse, Json};
use std::collections::HashMap;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

const METHODS: [&str; 9] = [
    "computer.observe",
    "computer.click",
    "computer.type",
    "computer.key",
    "computer.scroll",
    "computer.set_value",
    "computer.drag",
    "computer.perform",
    "computer.paste",
];

pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    for name in METHODS {
        functions.insert(name.to_string(), Value::Native(name));
    }
    functions
}

struct Bridge {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

static BRIDGE: Mutex<Option<Bridge>> = Mutex::new(None);
/// the (app, state_id) of the last successful observation — actions act
/// on what you saw and carry the state_id fail-closed, exactly like the
/// Python/TS runtimes (docs/computer-use.md §3)
static LAST_OBS: Mutex<Option<(String, String)>> = Mutex::new(None);

/// Fake desktop parity with the Python/TS runtimes: one scene, four
/// elements, `s-1` state ids, deterministic results.
fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn fake_call(op: &str, req: &Json) -> Result<Json, String> {
    let app = match req.get("app") {
        Some(Json::Str(s)) => s.clone(),
        _ => String::new(),
    };
    let elements: Vec<Json> = [
        ("window", "FakeApp"),
        ("button", "OK"),
        ("button", "Cancel"),
        ("textfield", "Search"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (role, title))| {
        let mut pairs = vec![
            ("index", Json::Num(i as f64)),
            ("role", Json::Str(role.to_string())),
            ("title", Json::Str(title.to_string())),
        ];
        if i > 0 && i < 3 {
            pairs.push(("pressable", Json::Bool(true)));
        }
        if i == 3 {
            pairs.push(("editable", Json::Bool(true)));
        }
        obj(pairs)
    })
    .collect();
    if op == "observe" {
        return Ok(obj(vec![
            ("state_id", Json::Str("s-1".into())),
            ("title", Json::Str("FakeApp".into())),
            ("app", Json::Str(app)),
            ("elements", Json::Arr(elements)),
        ]));
    }
    // the fake desktop enforces the SAME fail-closed contract as the real
    // bridge: stale state, unknown targets and unadvertised actions fail
    let stale = || {
        Err("stale_state: the state_id this action was decided on is not the current fake state — re-observe".to_string())
    };
    match req.get("state_id") {
        Some(Json::Str(s)) if s == "s-1" => {}
        _ => return stale(),
    }
    let index = match req.get("target").and_then(|t| t.get("index")) {
        Some(Json::Num(n)) => *n as i64,
        _ => -1,
    };
    if index != -1 && !(1..=3).contains(&index) {
        return Err(format!(
            "not_actionable: element {index} is not in the current scene"
        ));
    }
    if op == "perform" {
        let action = match req.get("action") {
            Some(Json::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let advertised: &[&str] = match index {
            1 | 2 => &["press"],
            _ => &[],
        };
        if !advertised.contains(&action.as_str()) {
            return Err(format!(
                "not_actionable: element {index} does not advertise '{action}'"
            ));
        }
    }
    Ok(obj(vec![
        ("ok", Json::Bool(true)),
        ("outcome", Json::Str("ok".into())),
        ("latency_ms", Json::Num(1.0)),
        ("error", Json::Str(String::new())),
    ]))
}

fn bridge_call(command: &str, request: &str) -> Result<Json, String> {
    let mut guard = BRIDGE.lock().map_err(|_| "computer bridge lock poisoned")?;
    let dead = match guard.as_mut() {
        Some(b) => b.child.try_wait().map(|s| s.is_some()).unwrap_or(true),
        None => true,
    };
    if dead {
        let parts = split_command(command);
        let prog = parts.first().ok_or("computer bridge command is empty")?;
        let mut child = Command::new(prog)
            .args(&parts[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("computer bridge failed to start ({prog}): {e}"))?;
        let stdin = child.stdin.take().ok_or("computer bridge has no stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("computer bridge has no stdout")?);
        *guard = Some(Bridge {
            child,
            stdin,
            stdout,
            next_id: 0,
        });
    }
    let sess = guard.as_mut().unwrap();
    sess.next_id += 1;
    let id = sess.next_id;
    let request = request.replacen("\"id\":0", &format!("\"id\":{id}"), 1);
    writeln!(sess.stdin, "{request}").map_err(|e| format!("computer bridge write failed: {e}"))?;
    sess.stdin
        .flush()
        .map_err(|e| format!("computer bridge flush failed: {e}"))?;
    let mut line = String::new();
    sess.stdout
        .read_line(&mut line)
        .map_err(|e| format!("computer bridge read failed: {e}"))?;
    if line.trim().is_empty() {
        return Err("computer bridge closed the pipe mid-call".into());
    }
    let msg = parse(line.trim()).map_err(|e| format!("computer bridge sent invalid JSON: {e}"))?;
    // the response MUST match the request id — a stale/foreign reply must
    // never be served as this call's answer
    let matched = match msg.get("id") {
        Some(Json::Num(n)) => *n as u64 == id,
        _ => false,
    };
    if !matched {
        return Err("computer bridge replied with a mismatched request id".into());
    }
    if msg.get("ok") == Some(&Json::Bool(false)) {
        let err = match msg.get("error") {
            Some(Json::Str(s)) => s.clone(),
            _ => "unknown bridge error".into(),
        };
        return Err(format!("computer bridge error: {err}"));
    }
    // return the inner payload (observation | result) — the language
    // surface is Observation / ActionResult, not a wrapper map
    msg.get("observation")
        .or_else(|| msg.get("result"))
        .cloned()
        .ok_or_else(|| "computer bridge response has no observation/result".to_string())
}

/// Clear the last-observation authority (test isolation: the static is
/// process-global, and tests must not inherit each other's state).
#[doc(hidden)]
#[cfg_attr(not(test), allow(dead_code))]
pub fn reset_authority_for_tests() {
    if let Ok(mut g) = LAST_OBS.lock() {
        *g = None;
    }
}

pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    if !METHODS.contains(&name) {
        return Err(format!("Unknown computer function: {name}"));
    }
    let op = name.strip_prefix("computer.").unwrap().to_string();
    if std::env::var("NUDGE_COMPUTER_KILL").as_deref() == Ok("1") {
        return Err("ComputerDenied: NUDGE_COMPUTER_KILL=1 refuses all computer work".into());
    }
    // canonical surface: only `observe` names an app — every action acts
    // on the app + state of the LAST successful observation (the runtime
    // owns the authority, like the Python/TS runtimes)
    let (app, payload_args): (String, &[Value]) = if op == "observe" {
        let app = args
            .first()
            .ok_or_else(|| format!("{name} requires the app name to observe"))?;
        (value_to_text(app), &args[1..])
    } else {
        let guard = LAST_OBS
            .lock()
            .map_err(|_| "computer authority lock poisoned")?;
        let (app, _state_id) = guard
            .as_ref()
            .ok_or("computer action without a prior computer.observe — observe the app you intend to act on")?;
        (app.clone(), &args[..])
    };
    let request = request_json(&op, &app, payload_args);
    let provider = std::env::var("NUDGE_COMPUTER_PROVIDER").unwrap_or_else(|_| "fake".into());
    if provider == "fake" {
        let response = fake_call(&op, &request)?;
        let value = json_to_value(&response)?;
        if op == "observe" {
            commit_authority(&app, &value);
        }
        return Ok(value);
    }
    let registry = std::env::var("NUDGE_COMPUTER_SERVERS").unwrap_or_default();
    let command = bridge_command(&registry, &provider)?;
    let response = bridge_call(&command, &json_to_string(&request))?;
    let value = json_to_value(&response)?;
    if op == "observe" {
        commit_authority(&app, &value);
    }
    Ok(value)
}

fn commit_authority(app: &str, v: &Value) {
    let state_id = match v {
        Value::Map(m) => match m.get("state_id") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    if let Ok(mut g) = LAST_OBS.lock() {
        *g = Some((app.to_string(), state_id));
    }
}

fn request_json(op: &str, app: &str, args: &[Value]) -> Json {
    // args here are the payload args AFTER the (optional) app — click(1),
    // type("text"), scroll(t, "down", 2), set_value(i, v), perform(i, a),
    // paste(text[, format]), drag(from, to)
    let target = match args.first() {
        Some(Value::Int(i)) => obj(vec![("index", Json::Num(*i as f64))]),
        Some(Value::Map(m)) => {
            // a {x, y} raster coordinate passes through verbatim
            Json::Obj(
                m.iter()
                    .filter_map(|(k, v)| match v {
                        Value::Int(i) => Some((k.clone(), Json::Num(*i as f64))),
                        Value::Float(f) => Some((k.clone(), Json::Num(*f))),
                        _ => None,
                    })
                    .collect(),
            )
        }
        _ => obj(vec![("index", Json::Num(-1.0))]),
    };
    let text_at = |i: usize| args.get(i).map(value_to_text).unwrap_or_default();
    let pairs: Vec<(&str, Json)> = match op {
        "observe" => vec![],
        "click" => vec![("target", target)],
        "drag" => vec![
            ("target", target),
            (
                "to",
                match args.get(1) {
                    Some(Value::Int(i)) => obj(vec![("index", Json::Num(*i as f64))]),
                    _ => obj(vec![("index", Json::Num(-1.0))]),
                },
            ),
        ],
        "type" => vec![
            ("target", obj(vec![("index", Json::Num(-1.0))])),
            ("text", Json::Str(text_at(0))),
        ],
        "key" => vec![
            ("target", obj(vec![("index", Json::Num(-1.0))])),
            ("key", Json::Str(text_at(0))),
        ],
        "scroll" => {
            let pages = match args.get(2) {
                Some(Value::Int(i)) => *i as f64,
                _ => 1.0,
            };
            vec![
                ("target", target),
                ("direction", Json::Str(text_at(1))),
                ("pages", Json::Num(pages)),
            ]
        }
        "set_value" => vec![("target", target), ("value", Json::Str(text_at(1)))],
        "perform" => vec![("target", target), ("action", Json::Str(text_at(1)))],
        "paste" => {
            let mut v = vec![
                ("target", obj(vec![("index", Json::Num(-1.0))])),
                ("text", Json::Str(text_at(0))),
            ];
            if let Some(f) = args.get(1) {
                let fmt = value_to_text(f);
                if !fmt.is_empty() {
                    v.push(("format", Json::Str(fmt)));
                }
            }
            v
        }
        _ => vec![],
    };
    let mut body = vec![
        ("id", Json::Num(0.0)),
        ("op", Json::Str(op.to_string())),
        ("app", Json::Str(app.to_string())),
    ];
    let state_id = LAST_OBS
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|(_, s)| s.clone()))
        .unwrap_or_default();
    if op != "observe" && !state_id.is_empty() {
        body.push(("state_id", Json::Str(state_id)));
    }
    body.extend(pairs);
    obj(body)
}

fn json_to_string(j: &Json) -> String {
    match j {
        Json::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Json::Str(s) => json_str(s),
        Json::Bool(b) => b.to_string(),
        Json::Null => "null".into(),
        Json::Arr(xs) => {
            let inner: Vec<String> = xs.iter().map(json_to_string).collect();
            format!("[{}]", inner.join(","))
        }
        Json::Obj(pairs) => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}:{}", json_str(k), json_to_string(v)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

/// shlex-style split (double/single quotes) — parity with the Python
/// runtime's shlex.split for quoted provider commands
fn split_command(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Extract the provider entry's `command` from the NUDGE_COMPUTER_SERVERS
/// registry JSON.
fn bridge_command(registry: &str, provider: &str) -> Result<String, String> {
    let reg = parse(registry).map_err(|e| format!("bad NUDGE_COMPUTER_SERVERS: {e}"))?;
    let entry = reg.get(provider).ok_or_else(|| {
        format!("computer provider '{provider}' is not configured — set NUDGE_COMPUTER_SERVERS")
    })?;
    entry
        .get("command")
        .and_then(|c| c.as_str().map(String::from))
        .ok_or_else(|| format!("computer provider '{provider}' needs a command in NUDGE_COMPUTER_SERVERS (HTTP transports run on the Python runtime)"))
}

fn value_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => format!("{other}"),
    }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn json_to_value(j: &Json) -> Result<Value, String> {
    Ok(match j {
        Json::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9e15 {
                Value::Int(*n as i64)
            } else {
                Value::Float(*n)
            }
        }
        Json::Str(s) => Value::String(s.clone()),
        Json::Bool(b) => Value::Bool(*b),
        Json::Null => Value::None,
        Json::Arr(xs) => Value::List(xs.iter().map(json_to_value).collect::<Result<_, _>>()?),
        Json::Obj(pairs) => {
            let mut m = HashMap::new();
            for (k, v) in pairs {
                m.insert(k.clone(), json_to_value(v)?);
            }
            Value::Map(m)
        }
    })
}
