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

const METHODS: [&str; 15] = [
    "computer.observe",
    "computer.click",
    "computer.type",
    "computer.key",
    "computer.scroll",
    "computer.set_value",
    "computer.drag",
    "computer.perform",
    "computer.paste",
    "computer.screen_size",
    "computer.window_info",
    "computer.mouse_move",
    "computer.mouse_down",
    "computer.mouse_up",
    "computer.cursor_pos",
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
/// Replay state (`NUDGE_REPLAY=all`): the computer.observe/act records of
/// the trace being replayed, keyed by trace path (a new path resets the
/// consumption cursors — replay loops, tests).
struct ReplayRecords {
    obs: Vec<Json>,
    acts: Vec<Json>,
    obs_idx: usize,
    act_idx: usize,
}

static REPLAY: Mutex<Option<(String, ReplayRecords)>> = Mutex::new(None);

struct FakeMouseState {
    x: f64,
    y: f64,
    buttons: [bool; 3],
}

static FAKE_MOUSE: Mutex<FakeMouseState> = Mutex::new(FakeMouseState {
    x: 0.0,
    y: 0.0,
    buttons: [false; 3],
});

/// Fake desktop parity with the Python/TS runtimes: one scene, four
/// elements, `s-1` state ids, deterministic results.
fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// JSON truthiness for the tree renderer (parity with the Python/TS
/// runtimes: missing `enabled` defaults to enabled).
fn truthy(j: Option<&Json>) -> bool {
    match j {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(Json::Num(n)) => *n != 0.0,
        Some(Json::Str(s)) => !s.is_empty(),
        Some(Json::Arr(xs)) => !xs.is_empty(),
        Some(Json::Obj(_)) => true,
    }
}

/// Rendered AX-style tree (zcode shape): `[N] role "title" = value
/// (flags)` — the text a model reads. Providers MAY send their own
/// `tree`; this renders it from elements when they don't (parity with
/// `_render_tree` in the Python/TS runtimes — same order and flags).
fn render_tree(elements: &[Json]) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(elements.len());
    for el in elements {
        let mut flags: Vec<&str> = Vec::new();
        if truthy(el.get("focused")) {
            flags.push("focused");
        }
        if truthy(el.get("selected")) {
            flags.push("selected");
        }
        if truthy(el.get("editable")) {
            flags.push("editable");
        }
        if truthy(el.get("pressable")) {
            flags.push("pressable");
        }
        if !el.get("enabled").map(|v| truthy(Some(v))).unwrap_or(true) {
            flags.push("disabled");
        }
        let index = match el.get("index") {
            Some(Json::Num(n)) if n.fract() == 0.0 && n.is_finite() && n.abs() < 9e15 => {
                format!("{}", *n as i64)
            }
            Some(Json::Num(n)) => format!("{n}"),
            _ => "0".to_string(),
        };
        let role = el.get("role").and_then(Json::as_str).unwrap_or("element");
        let title = el.get("title").and_then(Json::as_str).unwrap_or("");
        let mut head = format!("[{index}] {role}");
        if !title.is_empty() {
            head.push_str(&format!(" \"{title}\""));
        }
        if truthy(el.get("value")) {
            head.push_str(&format!(" = {}", json_scalar_text(el.get("value"))));
        }
        if truthy(el.get("actions")) {
            let acts: Vec<String> = match el.get("actions") {
                Some(Json::Arr(xs)) => xs
                    .iter()
                    .map(|x| format!("'{}'", x.as_str().unwrap_or("")))
                    .collect(),
                _ => Vec::new(),
            };
            head.push_str(&format!(" actions=[{}]", acts.join(", ")));
        }
        if !flags.is_empty() {
            head.push_str(&format!(" ({})", flags.join(",")));
        }
        lines.push(head);
    }
    lines.join("\n")
}

/// Scalar display text for tree values (str/num/bool pass through).
fn json_scalar_text(j: Option<&Json>) -> String {
    match j {
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Num(n)) => json_to_string(&Json::Num(*n)),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// The empty `Drift` record every Observation carries outside drift mode
/// (parity with the Python/TS runtimes' normalized Observation).
fn empty_drift() -> Json {
    obj(vec![
        ("changed", Json::Bool(false)),
        ("screenshot_changed", Json::Bool(false)),
        ("added", Json::Arr(Vec::new())),
        ("removed", Json::Arr(Vec::new())),
        ("summary", Json::Str(String::new())),
    ])
}

fn fake_call(op: &str, req: &Json) -> Result<Json, String> {
    let app = match req.get("app") {
        Some(Json::Str(s)) => s.clone(),
        _ => String::new(),
    };
    // canonical Element records (parity with the Python/TS fake scene and
    // the `Element` builtin record): every field the language can read is
    // present, so `obs.elements[i].actions`-style access never misses
    let elements: Vec<Json> = [
        ("window", "FakeApp"),
        ("button", "OK"),
        ("button", "Cancel"),
        ("textfield", "Search"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (role, title))| {
        let pressable = (1..=2).contains(&i);
        let editable = i == 3;
        let actions: Vec<Json> = if pressable {
            vec![Json::Str("press".into())]
        } else {
            Vec::new()
        };
        obj(vec![
            ("index", Json::Num(i as f64)),
            ("role", Json::Str(role.to_string())),
            ("title", Json::Str(title.to_string())),
            ("value", Json::Str(String::new())),
            ("pressable", Json::Bool(pressable)),
            ("editable", Json::Bool(editable)),
            ("focused", Json::Bool(i == 3)),
            ("enabled", Json::Bool(true)),
            ("actions", Json::Arr(actions)),
            ("bounds", Json::Arr(Vec::new())),
        ])
    })
    .collect();
    if op == "screen_size" {
        return Ok(obj(vec![
            ("width", Json::Num(1920.0)),
            ("height", Json::Num(1080.0)),
        ]));
    }
    if op == "cursor_pos" {
        let mouse = FAKE_MOUSE.lock().unwrap_or_else(|e| e.into_inner());
        return Ok(obj(vec![
            ("x", Json::Num(mouse.x)),
            ("y", Json::Num(mouse.y)),
        ]));
    }
    if op == "window_info" {
        let target_app = if app.is_empty() { "FakeApp" } else { &app };
        return Ok(obj(vec![
            ("app", Json::Str(target_app.to_string())),
            ("title", Json::Str(format!("{target_app} Window"))),
            ("width", Json::Num(1920.0)),
            ("height", Json::Num(1080.0)),
            ("x", Json::Num(0.0)),
            ("y", Json::Num(0.0)),
            ("focused", Json::Bool(true)),
        ]));
    }
    if op == "observe" {
        // canonical Observation record (the `Observation` builtin type):
        // tree rendered zcode-style, screenshot fields empty (the fake
        // desktop has no pixels unless a scenario supplies them)
        let tree = render_tree(&elements);
        return Ok(obj(vec![
            ("state_id", Json::Str("s-1".into())),
            ("title", Json::Str("FakeApp".into())),
            ("app", Json::Str(app)),
            ("elements", Json::Arr(elements)),
            ("tree", Json::Str(tree)),
            ("screenshot_hash", Json::Str(String::new())),
            ("screenshot", Json::Str(String::new())),
            ("drift", empty_drift()),
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
    if op == "mouse_move" {
        let x = match req.get("x") {
            Some(Json::Num(n)) => *n,
            _ => 0.0,
        };
        let y = match req.get("y") {
            Some(Json::Num(n)) => *n,
            _ => 0.0,
        };
        if x < 0.0 || y < 0.0 || x > 1920.0 || y > 1080.0 {
            return Err(format!(
                "out_of_bounds: coordinates ({x}, {y}) are outside screen boundaries (0..1920, 0..1080)"
            ));
        }
        let mut mouse = FAKE_MOUSE.lock().unwrap_or_else(|e| e.into_inner());
        mouse.x = x;
        mouse.y = y;
        return Ok(obj(vec![
            ("ok", Json::Bool(true)),
            ("outcome", Json::Str("ok".into())),
            ("latency_ms", Json::Num(0.0)),
            ("error", Json::Str(String::new())),
            // the in-process fake dispatches by construction
            ("action_sent", Json::Bool(true)),
        ]));
    }
    if op == "mouse_down" || op == "mouse_up" {
        let btn = match req.get("button") {
            Some(Json::Num(n)) => *n as i64,
            _ => 1,
        };
        if !(1..=3).contains(&btn) {
            return Err(format!(
                "not_actionable: mouse button must be 1, 2, or 3, got {btn}"
            ));
        }
        let mut mouse = FAKE_MOUSE.lock().unwrap_or_else(|e| e.into_inner());
        mouse.buttons[(btn - 1) as usize] = op == "mouse_down";
        return Ok(obj(vec![
            ("ok", Json::Bool(true)),
            ("outcome", Json::Str("ok".into())),
            ("latency_ms", Json::Num(0.0)),
            ("error", Json::Str(String::new())),
            // the in-process fake dispatches by construction
            ("action_sent", Json::Bool(true)),
        ]));
    }
    Ok(obj(vec![
        ("ok", Json::Bool(true)),
        ("outcome", Json::Str("ok".into())),
        ("latency_ms", Json::Num(1.0)),
        ("error", Json::Str(String::new())),
        // the in-process fake dispatches by construction
        ("action_sent", Json::Bool(true)),
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
    if let Ok(mut g) = REPLAY.lock() {
        *g = None;
    }
    if let Ok(mut g) = FAKE_MOUSE.lock() {
        *g = FakeMouseState {
            x: 0.0,
            y: 0.0,
            buttons: [false; 3],
        };
    }
}

/// Replay takes precedence over the configured provider: with
/// `NUDGE_REPLAY=all` the recorded observation IS the observation and the
/// recorded ActionResult is returned dry-run — with signature checks so a
/// program that diverges from the trace fails loudly instead of replaying
/// a decision that was never made (parity with the Python/TS runtimes).
fn replaying() -> bool {
    if std::env::var("NUDGE_REPLAY").unwrap_or_default().is_empty() {
        return false;
    }
    std::env::var("NUDGE_REPLAY_MODE").unwrap_or_else(|_| "all".into()) == "all"
}

fn replay_records(path: &str) -> Result<ReplayRecords, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("ReplayMismatch: cannot read the NUDGE_REPLAY trace '{path}': {e}"))?;
    let mut recs = ReplayRecords {
        obs: Vec::new(),
        acts: Vec::new(),
        obs_idx: 0,
        act_idx: 0,
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(rec) = parse(line) else { continue };
        match rec.get("kind").and_then(Json::as_str) {
            Some("computer.observe") => recs.obs.push(rec),
            Some("computer.act") => recs.acts.push(rec),
            _ => {}
        }
    }
    Ok(recs)
}

/// Consume the next recorded `kind` ("obs" | "acts") — None when the trace
/// is exhausted (replay exhaustion raises like llm replay).
fn replay_take(kind: &str) -> Result<Option<Json>, String> {
    let path = std::env::var("NUDGE_REPLAY").unwrap_or_default();
    let mut guard = REPLAY.lock().map_err(|_| "computer replay lock poisoned")?;
    if guard.as_ref().map(|(p, _)| p.as_str()) != Some(path.as_str()) {
        *guard = Some((path.clone(), replay_records(&path)?));
    }
    let state = &mut guard.as_mut().unwrap().1;
    let (list, idx) = match kind {
        "obs" => (&mut state.obs, &mut state.obs_idx),
        _ => (&mut state.acts, &mut state.act_idx),
    };
    if *idx < list.len() {
        let rec = list[*idx].clone();
        *idx += 1;
        Ok(Some(rec))
    } else {
        Ok(None)
    }
}

/// Canonical comparison form for replay signature checks — an index target
/// and an {x, y} raster target normalize to fixed shapes (no int/float
/// mismatch false positives), parity with `_canonical_computer_target`.
fn canonical_target(t: Option<&Json>) -> Json {
    match t {
        Some(Json::Obj(_))
            if t.and_then(|t| t.get("x")).is_some() || t.and_then(|t| t.get("y")).is_some() =>
        {
            let num = |k: &str| {
                t.and_then(|t| t.get(k))
                    .and_then(Json::as_num)
                    .unwrap_or(0.0)
            };
            obj(vec![("x", Json::Num(num("x"))), ("y", Json::Num(num("y")))])
        }
        Some(Json::Obj(_)) => {
            let index = t
                .and_then(|t| t.get("index"))
                .and_then(Json::as_num)
                .unwrap_or(-1.0);
            obj(vec![("index", Json::Num(index as i64 as f64))])
        }
        _ => obj(vec![("index", Json::Num(-1.0))]),
    }
}

fn drift_diff(live: &Json, recorded: &Json) -> Json {
    // mechanical observation diff: removed/added tree rows (by element
    // index + role + title) and the screenshot hash. Evidence, not
    // judgment — interpreting the summary is the model's job.
    let key = |el: &Json| {
        format!(
            "{}:{}:{}",
            el.get("index").and_then(Json::as_num).unwrap_or(-1.0),
            el.get("role").and_then(Json::as_str).unwrap_or(""),
            el.get("title").and_then(Json::as_str).unwrap_or("")
        )
    };
    let elements = |j: &Json| -> Vec<String> {
        match j.get("elements") {
            Some(Json::Arr(els)) => els.iter().map(&key).collect(),
            _ => Vec::new(),
        }
    };
    let recorded_keys: std::collections::BTreeSet<String> =
        elements(recorded).into_iter().collect();
    let live_keys: std::collections::BTreeSet<String> = elements(live).into_iter().collect();
    let added: Vec<String> = live_keys.difference(&recorded_keys).cloned().collect();
    let removed: Vec<String> = recorded_keys.difference(&live_keys).cloned().collect();
    let recorded_hash = recorded
        .get("screenshot_hash")
        .and_then(Json::as_str)
        .unwrap_or("");
    let live_hash = live
        .get("screenshot_hash")
        .and_then(Json::as_str)
        .unwrap_or("");
    let shot_changed =
        !live_hash.is_empty() && !recorded_hash.is_empty() && live_hash != recorded_hash;
    let tree_changed = recorded.get("tree") != live.get("tree")
        && (recorded.get("tree").is_some() || live.get("tree").is_some());
    let changed = !added.is_empty() || !removed.is_empty() || shot_changed || tree_changed;
    let mut parts: Vec<String> = Vec::new();
    if !removed.is_empty() {
        parts.push(format!("{} element(s) gone", removed.len()));
    }
    if !added.is_empty() {
        parts.push(format!("{} new element(s)", added.len()));
    }
    if shot_changed {
        parts.push("screenshot changed".into());
    }
    obj(vec![
        ("changed", Json::Bool(changed)),
        ("screenshot_changed", Json::Bool(shot_changed)),
        (
            "added",
            Json::Arr(added.into_iter().map(Json::Str).collect()),
        ),
        (
            "removed",
            Json::Arr(removed.into_iter().map(Json::Str).collect()),
        ),
        ("summary", Json::Str(parts.join("; "))),
    ])
}

/// The screenshot pixels of a recorded observation live in the
/// content-addressed sidecar next to the trace — restore them so replay
/// sees the same Observation the record saw. The asset name must be
/// EXACTLY a 64-hex digest + ".txt": a crafted trace cannot traverse out
/// of the assets directory (parity with the Python/TS runtimes).
fn read_trace_asset(name: &str) -> String {
    if name.len() != 68
        || !name.ends_with(".txt")
        || !name[..64]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return String::new();
    }
    let path = std::env::var("NUDGE_REPLAY").unwrap_or_default();
    if path.is_empty() {
        return String::new();
    }
    let p = std::path::Path::new(&path);
    let file = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let asset = p
        .parent()
        .unwrap_or(p)
        .join(format!("{file}.assets/{name}"));
    std::fs::read_to_string(asset).unwrap_or_default()
}

/// Replay path for `computer.observe`: the recorded observation IS the
/// observation (app signature checked), or a LIVE re-observation with a
/// mechanical `drift` diff attached when NUDGE_COMPUTER_DRIFT=1 — actions
/// stay dry-run in drift mode; the model interprets the drift.
fn replay_observe(app: &str) -> Result<Value, String> {
    let recorded = match replay_take("obs")? {
        Some(rec) => rec,
        None => {
            return Err("ReplayMismatch: program made more computer.observe calls than the trace holds (replay exhaustion raises like llm replay)".into())
        }
    };
    let rec_app = recorded.get("app").and_then(Json::as_str).unwrap_or("");
    if rec_app != app {
        return Err(format!(
            "ReplayMismatch: replay signature mismatch: trace observed app '{rec_app}', program observed '{app}'"
        ));
    }
    let drift_mode = std::env::var("NUDGE_COMPUTER_DRIFT").as_deref() == Ok("1");
    if drift_mode {
        if std::env::var("NUDGE_COMPUTER_KILL").as_deref() == Ok("1") {
            return Err("ComputerDenied: NUDGE_COMPUTER_KILL=1 refuses all computer work".into());
        }
        // live re-observation (read-only, safe) compared to the recording
        let live = live_call("observe", app, &[])?;
        let drift = drift_diff(&live, &recorded);
        let mut value = json_to_value(&live)?;
        if let Value::Map(m) = &mut value {
            m.insert("drift".into(), json_to_value(&drift)?);
        }
        if let Value::Map(m) = &value {
            let state_id = match m.get("state_id") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            if let Ok(mut g) = LAST_OBS.lock() {
                *g = Some((app.to_string(), state_id));
            }
        }
        return Ok(value);
    }
    // plain replay: the recorded observation IS the observation — but the
    // language always sees the canonical Observation record (a missing
    // recorded `tree` is rendered from the elements, like the Python/TS
    // runtimes' `_normalize_observation`)
    let elements = recorded
        .get("elements")
        .cloned()
        .unwrap_or(Json::Arr(Vec::new()));
    let tree = match recorded.get("tree").filter(|t| truthy(Some(t))) {
        Some(t) => t.clone(),
        None => {
            let els: Vec<Json> = match &elements {
                Json::Arr(xs) => xs.clone(),
                _ => Vec::new(),
            };
            Json::Str(render_tree(&els))
        }
    };
    let asset = recorded
        .get("screenshot_asset")
        .and_then(Json::as_str)
        .unwrap_or("");
    let shot = read_trace_asset(asset);
    let str_field = |name: &str| {
        Json::Str(
            recorded
                .get(name)
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        )
    };
    let pairs = vec![
        (
            "state_id",
            Json::Str(
                recorded
                    .get("state_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
        ),
        ("app", Json::Str(app.to_string())),
        ("title", str_field("title")),
        ("elements", elements),
        ("tree", tree),
        ("screenshot_hash", str_field("screenshot_hash")),
        ("screenshot", Json::Str(shot)),
        ("drift", empty_drift()),
    ];
    let value = json_to_value(&obj(pairs))?;
    commit_authority(app, &value);
    Ok(value)
}

/// Replay path for actions: the recorded ActionResult is returned
/// DRY-RUN (nothing executes) after the call signature — action name,
/// app and target — is verified against the record.
fn replay_act(op: &str, app: &str, payload_args: &[Value]) -> Result<Value, String> {
    let recorded = match replay_take("acts")? {
        Some(rec) => rec,
        None => {
            return Err("ReplayMismatch: program made more computer actions than the trace holds (replay exhaustion raises like llm replay)".into())
        }
    };
    let rec_action = recorded.get("action").and_then(Json::as_str).unwrap_or("");
    if rec_action != op {
        return Err(format!(
            "ReplayMismatch: replay signature mismatch: trace action '{rec_action}', program called '{op}'"
        ));
    }
    let rec_app = recorded.get("app").and_then(Json::as_str).unwrap_or("");
    if !rec_app.is_empty() && rec_app != app {
        return Err(format!(
            "ReplayMismatch: replay signature mismatch: trace app '{rec_app}', program acted on '{app}'"
        ));
    }
    // only ops that carry a real target have one to verify (type/key/paste
    // carry the recorded null target, like the Python/TS records)
    let carries_target = matches!(op, "click" | "scroll" | "set_value" | "perform" | "drag");
    let payload_text = |i: usize| payload_args.get(i).map(value_to_text).unwrap_or_default();
    let expect = |name: &str, expected: Option<String>| -> Result<(), String> {
        let rec = recorded.get(name).and_then(Json::as_str).unwrap_or("");
        if rec.is_empty() {
            return Ok(());
        }
        if expected.as_deref() != Some(rec) {
            return Err(format!(
                "ReplayMismatch: replay signature mismatch: trace {name} '{rec}', program {name} '{}'",
                expected.unwrap_or_default()
            ));
        }
        Ok(())
    };
    if op == "mouse_move" {
        expect("x", Some(payload_text(0)))?;
        expect("y", Some(payload_text(1)))?;
    }
    if op == "mouse_down" || op == "mouse_up" {
        expect("button", Some(payload_text(0)))?;
    }
    if carries_target {
        let rec_target = recorded.get("target");
        let target = target_json(payload_args.first());
        if rec_target.map(Json::is_obj).unwrap_or(false)
            && canonical_target(rec_target) != canonical_target(Some(&target))
        {
            return Err(format!(
                "ReplayMismatch: replay signature mismatch: trace target {}, program target {}",
                json_to_string(rec_target.unwrap_or(&Json::Null)),
                json_to_string(&target)
            ));
        }
    }
    // full-payload identity (parity with the Python/TS request_hash check):
    // the recorded action payload fields must match the program's call —
    // `type("A")` must never replay a `type("B")` record
    expect("value", {
        let v = payload_text(0);
        if v.is_empty() {
            let v = payload_text(1);
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        } else {
            Some(v)
        }
    })?;
    if op == "key" {
        expect("key", Some(payload_text(0)))?;
    }
    if op == "scroll" {
        expect("direction", Some(payload_text(1)))?;
    }
    if op == "paste" {
        let fmt = payload_text(1);
        if !fmt.is_empty() {
            expect("format", Some(fmt))?;
        }
    }
    if let Some(rec_pages) = recorded.get("pages").and_then(Json::as_num) {
        let prog_pages = payload_args
            .get(2)
            .map(|v| match v {
                Value::Int(i) => *i as f64,
                _ => 1.0,
            })
            .unwrap_or(1.0);
        if rec_pages != prog_pages {
            return Err(format!(
                "ReplayMismatch: replay signature mismatch: trace pages {rec_pages}, program pages {prog_pages}"
            ));
        }
    }
    if op == "drag" {
        let rec_to = recorded.get("to");
        let to = target_json(payload_args.get(1));
        if rec_to.map(Json::is_obj).unwrap_or(false)
            && canonical_target(rec_to) != canonical_target(Some(&to))
        {
            return Err(format!(
                "ReplayMismatch: replay signature mismatch: trace destination {}, program destination {}",
                json_to_string(rec_to.unwrap_or(&Json::Null)),
                json_to_string(&to)
            ));
        }
    }
    // the recorded result IS the result, dry-run
    let ok = match recorded.get("ok") {
        Some(Json::Bool(b)) => *b,
        _ => false,
    };
    let outcome = recorded
        .get("outcome")
        .and_then(Json::as_str)
        .unwrap_or("unknown");
    let latency = recorded
        .get("latency_ms")
        .and_then(Json::as_num)
        .unwrap_or(0.0);
    // dispatch receipt parity with the Python/TS `_normalize_result`:
    // false only when the record KNOWS the input was not dispatched
    let action_sent = recorded.get("dispatch").and_then(Json::as_str) != Some("not_sent");
    json_to_value(&obj(vec![
        ("ok", Json::Bool(ok)),
        ("outcome", Json::Str(outcome.to_string())),
        ("latency_ms", Json::Num(latency)),
        ("error", Json::Str(String::new())),
        ("action_sent", Json::Bool(action_sent)),
    ]))
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
    let (app, payload_args): (String, &[Value]) = if op == "screen_size" || op == "cursor_pos" {
        (String::new(), &args[..])
    } else if op == "observe" || op == "window_info" {
        let app = args.first();
        let app_str = match app {
            Some(v) => value_to_text(v),
            None => String::new(),
        };
        let slice_start = if app.is_some() && (op == "observe" || op == "window_info") {
            1
        } else {
            0
        };
        (app_str, &args[slice_start..])
    } else {
        let guard = LAST_OBS
            .lock()
            .map_err(|_| "computer authority lock poisoned")?;
        let (app, _state_id) = guard
            .as_ref()
            .ok_or("computer action without a prior computer.observe — observe the app you intend to act on")?;
        (app.clone(), &args[..])
    };
    if replaying() {
        return if op == "observe" {
            replay_observe(&app)
        } else {
            replay_act(&op, &app, payload_args)
        };
    }
    let started = std::time::Instant::now();
    let live = live_call(&op, &app, payload_args)?;
    let latency_ms = started.elapsed().as_millis() as u64;
    trace_live_call(&op, &app, payload_args, &live, latency_ms)?;
    let value = json_to_value(&live)?;
    if op == "observe" {
        commit_authority(&app, &value);
    }
    Ok(value)
}

/// The `computer.act` actions the frozen NTF v1 `action` enum covers
/// (docs/ntf-spec.md) — every one becomes a `computer.act` record.
const TRACED_ACTS: [&str; 8] = [
    "click",
    "type",
    "key",
    "scroll",
    "set_value",
    "drag",
    "perform",
    "paste",
];

/// json → serde_json for trace records. Integral numbers come out as
/// JSON ints (parity with the Python runtime's element tables).
fn json_to_serde(j: &Json) -> serde_json::Value {
    use serde_json::Value as S;
    match j {
        Json::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9e15 {
                S::Number((*n as i64).into())
            } else {
                serde_json::Number::from_f64(*n)
                    .map(S::Number)
                    .unwrap_or(S::Null)
            }
        }
        Json::Str(s) => S::String(s.clone()),
        Json::Bool(b) => S::Bool(*b),
        Json::Null => S::Null,
        Json::Arr(xs) => S::Array(xs.iter().map(json_to_serde).collect()),
        Json::Obj(pairs) => S::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), json_to_serde(v)))
                .collect(),
        ),
    }
}

fn json_str_field(j: &Json, key: &str) -> String {
    j.get(key)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

/// One `computer.observe` / `computer.act` NTF record per successful live
/// call (field set mirrors the Python runtime's `computer_observe` /
/// `_computer_act` records). Replay never writes: a replayed run is
/// distinguishable by the absence of fresh records.
fn trace_live_call(
    op: &str,
    app: &str,
    args: &[Value],
    result: &Json,
    latency_ms: u64,
) -> Result<(), String> {
    if !crate::trace::sink_active() {
        return Ok(());
    }
    let provider = std::env::var("NUDGE_COMPUTER_PROVIDER").unwrap_or_else(|_| "fake".into());
    if op == "observe" {
        let elements = result
            .get("elements")
            .cloned()
            .unwrap_or(Json::Arr(Vec::new()));
        let element_count = match &elements {
            Json::Arr(xs) => xs.len(),
            _ => 0,
        };
        let mut record = serde_json::json!({
            "kind": "computer.observe",
            "app": app,
            "state_id": json_str_field(result, "state_id"),
            "element_count": element_count,
            "outcome": "ok",
            "latency_ms": latency_ms,
            "provider": provider,
            // additive + replay-critical: the element table lets replay
            // rebuild the full Observation (not just the tree text)
            "elements": json_to_serde(&elements),
            "snapshot_mode": "full",
        });
        let obj = record.as_object_mut().unwrap();
        for (key, out) in [
            ("title", "title"),
            ("tree", "tree"),
            ("screenshot_hash", "screenshot_hash"),
        ] {
            let v = json_str_field(result, key);
            if !v.is_empty() {
                obj.insert(out.into(), serde_json::Value::String(v));
            }
        }
        return crate::trace::emit(record);
    }
    if !TRACED_ACTS.contains(&op) {
        // screen_size / window_info / cursor_pos / mouse_* are outside the
        // frozen `action` enum — not trace records (yet)
        return Ok(());
    }
    let request = request_json(op, app, args);
    let mut record = serde_json::json!({
        "kind": "computer.act",
        "action": op,
        "app": app,
        "target": request
            .get("target")
            .map(json_to_serde)
            .unwrap_or(serde_json::Value::Null),
        "outcome": result
            .get("outcome")
            .and_then(Json::as_str)
            .unwrap_or("ok"),
        "latency_ms": latency_ms,
        "ok": result
            .get("ok")
            .and_then(|b| match b {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(true),
        // dispatch receipt: false only when the runtime KNOWS the input
        // was not dispatched (parity with `_normalize_result`)
        "action_sent": result
            .get("action_sent")
            .and_then(|b| match b {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(true),
        "provider": provider,
    });
    let obj = record.as_object_mut().unwrap();
    let error = json_str_field(result, "error");
    if !error.is_empty() {
        obj.insert("error".into(), serde_json::Value::String(error));
    }
    // the action payload (parity with the Python record's `value` +
    // `key`/`direction`/`pages`/`to`/`format` fields)
    for key in ["text", "value"] {
        if let Some(Json::Str(v)) = request.get(key) {
            if !v.is_empty() {
                obj.insert("value".into(), serde_json::Value::String(v.clone()));
            }
        }
    }
    for key in ["key", "direction", "pages", "to", "format"] {
        if let Some(v) = request.get(key) {
            obj.insert(key.into(), json_to_serde(v));
        }
    }
    crate::trace::emit(record)
}

/// The configured provider (fake desktop by default, subprocess JSONL
/// bridge via NUDGE_COMPUTER_SERVERS + NUDGE_COMPUTER_PROVIDER) — also the
/// live re-observation source of drift mode.
fn live_call(op: &str, app: &str, payload_args: &[Value]) -> Result<Json, String> {
    let request = request_json(op, app, payload_args);
    let provider = std::env::var("NUDGE_COMPUTER_PROVIDER").unwrap_or_else(|_| "fake".into());
    if provider == "fake" {
        return fake_call(op, &request);
    }
    let registry = std::env::var("NUDGE_COMPUTER_SERVERS").unwrap_or_default();
    let command = bridge_command(&registry, &provider)?;
    bridge_call(&command, &json_to_string(&request))
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

fn target_json(v: Option<&Value>) -> Json {
    match v {
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
    }
}

fn request_json(op: &str, app: &str, args: &[Value]) -> Json {
    // args here are the payload args AFTER the (optional) app — click(1),
    // type("text"), scroll(t, "down", 2), set_value(i, v), perform(i, a),
    // paste(text[, format]), drag(from, to)
    let target = target_json(args.first());
    let text_at = |i: usize| args.get(i).map(value_to_text).unwrap_or_default();
    let pairs: Vec<(&str, Json)> = match op {
        "observe" | "screen_size" | "window_info" | "cursor_pos" => vec![],
        "click" => vec![("target", target)],
        "drag" => vec![("target", target), ("to", target_json(args.get(1)))],
        "mouse_move" => {
            let num_at = |i: usize| -> f64 {
                match args.get(i) {
                    Some(Value::Int(n)) => *n as f64,
                    Some(Value::Float(f)) => *f,
                    _ => 0.0,
                }
            };
            vec![
                ("target", obj(vec![("index", Json::Num(-1.0))])),
                ("x", Json::Num(num_at(0))),
                ("y", Json::Num(num_at(1))),
            ]
        }
        "mouse_down" | "mouse_up" => {
            let int_at = |i: usize| -> i64 {
                match args.get(i) {
                    Some(Value::Int(n)) => *n,
                    Some(Value::Float(f)) => *f as i64,
                    _ => 1,
                }
            };
            vec![
                ("target", obj(vec![("index", Json::Num(-1.0))])),
                ("button", Json::Num(int_at(0) as f64)),
            ]
        }
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
