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

const METHODS: [&str; 7] = [
    "computer.observe",
    "computer.click",
    "computer.type",
    "computer.key",
    "computer.scroll",
    "computer.set_value",
    "computer.drag",
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

/// Fake desktop parity with the Python/TS runtimes: one scene, four
/// elements, `s-1` state ids, deterministic results.
fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn fake_call(op: &str, app: &str) -> Json {
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
        return obj(vec![
            ("ok", Json::Bool(true)),
            (
                "observation",
                obj(vec![
                    ("state_id", Json::Str("s-1".into())),
                    ("title", Json::Str("FakeApp".into())),
                    ("app", Json::Str(app.to_string())),
                    ("elements", Json::Arr(elements)),
                ]),
            ),
        ]);
    }
    obj(vec![
        ("ok", Json::Bool(true)),
        (
            "result",
            obj(vec![
                ("ok", Json::Bool(true)),
                ("outcome", Json::Str("ok".into())),
                ("latency_ms", Json::Num(1.0)),
                ("error", Json::Str(String::new())),
            ]),
        ),
    ])
}

fn bridge_call(command: &str, request: &str) -> Result<Json, String> {
    let mut guard = BRIDGE.lock().map_err(|_| "computer bridge lock poisoned")?;
    let dead = match guard.as_mut() {
        Some(b) => b.child.try_wait().map(|s| s.is_some()).unwrap_or(true),
        None => true,
    };
    if dead {
        let mut parts = command.split_whitespace();
        let prog = parts.next().ok_or("computer bridge command is empty")?;
        let mut child = Command::new(prog)
            .args(parts)
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
    parse(line.trim()).map_err(|e| format!("computer bridge sent invalid JSON: {e}"))
}

pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    if !METHODS.contains(&name) {
        return Err(format!("Unknown computer function: {name}"));
    }
    let op = name.strip_prefix("computer.").unwrap().to_string();
    if args.is_empty() {
        return Err(format!(
            "{name} requires the app name — observe the app you intend to act on"
        ));
    }
    let app = value_to_text(&args[0]);
    if std::env::var("NUDGE_COMPUTER_KILL").as_deref() == Ok("1") {
        return Err("ComputerDenied: NUDGE_COMPUTER_KILL=1 refuses all computer work".into());
    }
    let provider = std::env::var("NUDGE_COMPUTER_PROVIDER").unwrap_or_else(|_| "fake".into());
    let response = if provider == "fake" {
        fake_call(&op, &app)
    } else {
        let registry = std::env::var("NUDGE_COMPUTER_SERVERS").unwrap_or_default();
        let command = bridge_command(&registry, &provider)?;
        let request = build_request(&op, &app, &args)?;
        bridge_call(&command, &request)?
    };
    json_to_value(&response)
}

fn build_request(op: &str, app: &str, args: &[Value]) -> Result<String, String> {
    let target = match args.get(1) {
        Some(Value::Int(i)) => format!("{{\"index\":{i}}}"),
        _ => "{\"index\":-1}".to_string(),
    };
    let body = match op {
        "observe" => format!("\"app\":{}", json_str(app)),
        "click" | "drag" => format!("\"app\":{},\"target\":{target}", json_str(app)),
        "type" => format!(
            "\"app\":{},\"target\":{{\"index\":-1}},\"text\":{}",
            json_str(app),
            json_str(&args.get(1).map(value_to_text).unwrap_or_default())
        ),
        "key" => format!(
            "\"app\":{},\"target\":{{\"index\":-1}},\"key\":{}",
            json_str(app),
            json_str(&args.get(1).map(value_to_text).unwrap_or_default())
        ),
        "scroll" => format!(
            "\"app\":{},\"target\":{target},\"direction\":{},\"pages\":1",
            json_str(app),
            json_str(
                &args
                    .get(2)
                    .map(value_to_text)
                    .unwrap_or_else(|| "down".into())
            )
        ),
        "set_value" => format!(
            "\"app\":{},\"target\":{target},\"value\":{}",
            json_str(app),
            json_str(&args.get(2).map(value_to_text).unwrap_or_default())
        ),
        _ => return Err(format!("computer op '{op}' is not wired in the VM")),
    };
    Ok(format!("{{\"id\":0,\"op\":\"{op}\",{body}}}"))
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
