//! A2A agent-card export (design §9, v1.0) and A2A protocol serving (v2.0):
//! `nudgec a2a <file.ndg>` emits `out/<name>.agent.json`.
//! `nudgec a2a-serve <file.ndg> [--port N]` serves the agent as a network-addressable
//! A2A peer with agent card, health, and skill execution endpoints.

use crate::ast::*;
use crate::eval::val::NudgeVal;
use crate::json::dumps;
use crate::json::Json;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};

pub const DEFAULT_PORT: u16 = 9000;
pub const MAX_BODY_SIZE: usize = 2 * 1024 * 1024; // 2MB

fn ty_name(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::List(inner) => format!("[{}]", ty_name(inner)),
        TypeExpr::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", ty_name(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Refine(inner, name, _) => format!("{} @{name}(…)", ty_name(inner)),
        TypeExpr::Generic(name, args) => format!(
            "{name}[{}]",
            args.iter().map(ty_name).collect::<Vec<_>>().join(", ")
        ),
        TypeExpr::Optional(inner) => format!("{}?", ty_name(inner)),
        TypeExpr::Union(variants) => variants.iter().map(ty_name).collect::<Vec<_>>().join(" | "),
    }
}

fn skill(name: &str, params: &[Param], ret: &TypeExpr, effects: &[String]) -> Json {
    let sig = format!(
        "{}({}) -> {}{}",
        name,
        params
            .iter()
            .map(|p| format!("{}: {}", p.name, ty_name(&p.ty)))
            .collect::<Vec<_>>()
            .join(", "),
        ty_name(ret),
        if effects.is_empty() {
            String::new()
        } else {
            format!(" uses {}", effects.join(", "))
        },
    );
    Json::Obj(vec![
        ("id".into(), Json::str(name)),
        ("name".into(), Json::str(name)),
        ("description".into(), Json::str(format!("Nudge fn `{sig}`"))),
        (
            "tags".into(),
            Json::Arr(if effects.is_empty() {
                vec![Json::str("pure")]
            } else {
                effects
                    .iter()
                    .map(|e| Json::str(e.to_lowercase()))
                    .collect()
            }),
        ),
    ])
}

pub fn card_with_url(name: &str, skills: Vec<Json>, url: &str) -> Json {
    Json::Obj(vec![
        ("name".into(), Json::str(name)),
        (
            "description".into(),
            Json::str(format!(
                "Nudge agent '{name}' -- compiled from .ndg (https://github.com/NekomyaDev/nudge)"
            )),
        ),
        ("version".into(), Json::str("1.0.0")),
        ("url".into(), Json::str(url)),
        (
            "capabilities".into(),
            Json::Obj(vec![
                ("streaming".into(), Json::Bool(false)),
                ("pushNotifications".into(), Json::Bool(false)),
                ("stateTransitionHistory".into(), Json::Bool(true)),
            ]),
        ),
        (
            "defaultInputModes".into(),
            Json::Arr(vec![Json::str("text")]),
        ),
        (
            "defaultOutputModes".into(),
            Json::Arr(vec![Json::str("text")]),
        ),
        ("skills".into(), Json::Arr(skills)),
    ])
}

pub fn card(name: &str, skills: Vec<Json>) -> Json {
    card_with_url(name, skills, "http://localhost:9999/")
}

/// `(file_stem, card)` pairs -- one per agent block, or one for the file.
pub fn cards_with_url(items: &[Item], file_stem: &str, url: &str) -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for item in items {
        if let Item::Agent { name, fns, .. } = item {
            let skills = fns
                .iter()
                .filter_map(|f| match f {
                    Item::Fn {
                        name,
                        params,
                        ret,
                        effects,
                        ..
                    } => Some(skill(name, params, ret, effects)),
                    _ => None,
                })
                .collect();
            out.push((name.to_lowercase(), card_with_url(name, skills, url)));
        }
    }
    if out.is_empty() {
        let skills = items
            .iter()
            .filter_map(|f| match f {
                Item::Fn {
                    name,
                    params,
                    ret,
                    effects,
                    ..
                } => Some(skill(name, params, ret, effects)),
                _ => None,
            })
            .collect();
        out.push((file_stem.to_string(), card_with_url(file_stem, skills, url)));
    }
    out
}

/// `(file_stem, card)` pairs -- one per agent block, or one for the file.
pub fn cards(items: &[Item], file_stem: &str) -> Vec<(String, Json)> {
    cards_with_url(items, file_stem, "http://localhost:9999/")
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub const A2A_SERVER_PY: &str = r#""""Generated by nudgec -- A2A protocol server, do not edit."""
import base64
import importlib.util
import inspect
import json
import sys
import urllib.parse
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

spec = importlib.util.spec_from_file_location("nudge_mod", "__MODULE__")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

AGENT_CARD = json.loads(base64.b64decode("__AGENT_CARD_B64__").decode("utf-8"))
AGENT_NAME = AGENT_CARD.get("name") or "__AGENT_NAME__"
PORT = __PORT__

TASKS = {}


def _plain(v):
    if isinstance(v, dict):
        return {k: _plain(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [_plain(x) for x in v]
    if hasattr(v, "__dict__") and not isinstance(v, type):
        return {k: _plain(x) for k, x in vars(v).items()}
    return v


def _find_fn(name):
    if not name:
        return None
    for cand in (name, name.lower(), name.replace("-", "_")):
        fn = getattr(m, cand, None)
        if fn is not None and callable(fn):
            return fn
    return None


def _default_fn():
    for cand in ("main", "step", "run"):
        fn = getattr(m, cand, None)
        if fn is not None and callable(fn):
            return cand, fn
    skills = AGENT_CARD.get("skills", [])
    if skills:
        first_id = skills[0].get("id") or skills[0].get("name")
        fn = _find_fn(first_id)
        if fn is not None:
            return first_id, fn
    for attr in sorted(dir(m)):
        if not attr.startswith("_") and not attr.startswith("nudge_test_"):
            fn = getattr(m, attr, None)
            if fn is not None and callable(fn) and hasattr(fn, "__code__"):
                return attr, fn
    return None, None


def _call_fn(fn, payload):
    data = payload
    if isinstance(payload, dict):
        if "input" in payload:
            data = payload["input"]
        elif "args" in payload and isinstance(payload["args"], list):
            data = payload["args"]
        elif "arguments" in payload:
            data = payload["arguments"]
        else:
            reserved = {"skill", "skill_name", "id", "task_id", "method", "action", "name"}
            filtered = {k: v for k, v in payload.items() if k not in reserved}
            if filtered:
                data = filtered

    sig = inspect.signature(fn)
    params = list(sig.parameters.values())

    if len(params) == 0:
        return fn()

    if isinstance(data, list):
        return fn(*data)

    if isinstance(data, dict):
        param_names = [p.name for p in params]
        if all(k in data for k in param_names):
            return fn(**{k: data[k] for k in param_names})
        if len(params) == 1:
            return fn(data)

    if data is not None and not (isinstance(data, dict) and len(data) == 0):
        return fn(data)
    return fn()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _json(self, code, payload):
        body = json.dumps(payload, ensure_ascii=False, default=str).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        self.end_headers()

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path.rstrip("/")
        if not path:
            path = "/"

        if path in ("/.well-known/agent.json", "/agent.json"):
            self._json(200, AGENT_CARD)
        elif path == "/health":
            self._json(200, {"status": "ok", "agent": AGENT_NAME, "version": "1.0.0"})
        elif path.startswith("/tasks/"):
            task_id = path[len("/tasks/"):].strip("/")
            if task_id in TASKS:
                self._json(200, TASKS[task_id])
            else:
                self._json(404, {"status": "failed", "error": f"task '{task_id}' not found"})
        elif path == "/tasks":
            self._json(200, {"tasks": list(TASKS.values())})
        else:
            self.send_error(404)

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path.rstrip("/")
        if not path:
            path = "/"

        try:
            content_len = int(self.headers.get("Content-Length", 0))
        except (ValueError, TypeError):
            content_len = 0

        body_bytes = self.rfile.read(content_len) if content_len > 0 else b""
        if body_bytes.strip():
            try:
                req = json.loads(body_bytes.decode("utf-8"))
            except Exception as e:
                self._json(400, {"status": "failed", "error": f"bad JSON: {e}"})
                return
        else:
            req = {}

        if path == "/tasks":
            skill_name = None
            if isinstance(req, dict):
                skill_name = (
                    req.get("skill")
                    or req.get("skill_name")
                    or req.get("name")
                    or req.get("method")
                    or req.get("action")
                )
            task_id = (
                str(req.get("id") or req.get("task_id") or uuid.uuid4().hex[:12])
                if isinstance(req, dict)
                else uuid.uuid4().hex[:12]
            )
            if skill_name:
                fn = _find_fn(skill_name)
            else:
                skill_name, fn = _default_fn()

            if fn is None:
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "failed",
                    "error": f"skill '{skill_name or ''}' not found",
                }
                TASKS[task_id] = record
                self._json(404, record)
                return

            try:
                out = _call_fn(fn, req)
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "completed",
                    "output": _plain(out),
                }
                TASKS[task_id] = record
                self._json(200, record)
            except Exception as e:
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "failed",
                    "error": f"{type(e).__name__}: {e}",
                }
                TASKS[task_id] = record
                self._json(500, record)

        elif path.startswith("/skills/"):
            skill_name = path[len("/skills/"):].strip("/")
            task_id = (
                str(req.get("id") or req.get("task_id") or uuid.uuid4().hex[:12])
                if isinstance(req, dict)
                else uuid.uuid4().hex[:12]
            )
            fn = _find_fn(skill_name)
            if fn is None:
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "failed",
                    "error": f"skill '{skill_name}' not found",
                }
                TASKS[task_id] = record
                self._json(404, record)
                return

            try:
                out = _call_fn(fn, req)
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "completed",
                    "output": _plain(out),
                }
                TASKS[task_id] = record
                self._json(200, record)
            except Exception as e:
                record = {
                    "id": task_id,
                    "task_id": task_id,
                    "status": "failed",
                    "error": f"{type(e).__name__}: {e}",
                }
                TASKS[task_id] = record
                self._json(500, record)

        elif path == "/run":
            task_id = uuid.uuid4().hex[:12]
            skill_name, fn = _default_fn()
            if fn is None:
                self._json(404, {"ok": False, "status": "failed", "error": "no function found"})
                return
            try:
                out = _call_fn(fn, req)
                record = {
                    "ok": True,
                    "id": task_id,
                    "task_id": task_id,
                    "status": "completed",
                    "output": _plain(out),
                    "result": _plain(out),
                }
                TASKS[task_id] = record
                self._json(200, record)
            except Exception as e:
                record = {
                    "ok": False,
                    "id": task_id,
                    "task_id": task_id,
                    "status": "failed",
                    "error": f"{type(e).__name__}: {e}",
                }
                TASKS[task_id] = record
                self._json(500, record)
        else:
            self.send_error(404)


if __name__ == "__main__":
    server = HTTPServer(("127.0.0.1", PORT), H)
    actual_port = server.server_address[1]
    if PORT == 0 or not AGENT_CARD.get("url") or f":{actual_port}" not in AGENT_CARD.get("url", ""):
        AGENT_CARD["url"] = f"http://127.0.0.1:{actual_port}/"
    print(f"A2A agent '{AGENT_NAME}' serving on http://127.0.0.1:{actual_port}")
    sys.stdout.flush()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
"#;

pub fn server_source(module_path: &str, agent_name: &str, card_json: &str, port: u16) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let b64_card = base64_encode(card_json.as_bytes());
    A2A_SERVER_PY
        .replace("__MODULE__", &esc(module_path))
        .replace("__AGENT_NAME__", &esc(agent_name))
        .replace("__AGENT_CARD_B64__", &b64_card)
        .replace("__PORT__", &port.to_string())
}

pub fn server_py(module_path: &str, agent_name: &str, card_json: &str, port: u16) -> String {
    server_source(module_path, agent_name, card_json, port)
}

fn generate_task_id() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let s = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("task_{now}_{s}")
}

fn body_is_jsonrpc(body: &[u8]) -> bool {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        v.get("jsonrpc").and_then(|s| s.as_str()) == Some("2.0")
    } else {
        false
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing)]
    pub created_at: u64,
}

#[derive(Clone)]
struct ServerContext {
    items: Vec<Item>,
    card_json: serde_json::Value,
    agent_name: String,
    whitelisted_skills: HashSet<String>,
    skill_params: HashMap<String, Vec<String>>,
    tasks: Arc<RwLock<HashMap<String, TaskRecord>>>,
    cancel_tokens: Arc<RwLock<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>>,
    api_key: Option<String>,
    active_connections: Arc<std::sync::atomic::AtomicUsize>,
    max_connections: usize,
    max_tasks: usize,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ServerContext {
    fn insert_and_prune_task(&self, id: String, rec: TaskRecord) {
        let mut tasks = self.tasks.write().unwrap();
        if tasks.len() >= self.max_tasks {
            let mut candidates: Vec<(String, u64)> = tasks
                .iter()
                .filter(|(_, t)| {
                    t.status == "completed" || t.status == "cancelled" || t.status == "failed"
                })
                .map(|(k, t)| (k.clone(), t.created_at))
                .collect();
            candidates.sort_by_key(|(_, ts)| *ts);
            let to_remove = (tasks.len() + 1).saturating_sub(self.max_tasks);
            for (k, _) in candidates.into_iter().take(to_remove) {
                tasks.remove(&k);
                let mut tokens = self.cancel_tokens.write().unwrap();
                tokens.remove(&k);
            }
        }
        tasks.insert(id, rec);
    }

    fn build_args(&self, skill: &str, payload: &serde_json::Value) -> Vec<NudgeVal> {
        let params = self.skill_params.get(skill).cloned().unwrap_or_default();
        if params.is_empty() {
            return Vec::new();
        }

        let data = if payload.is_object() {
            if let Some(inp) = payload.get("input") {
                inp
            } else if let Some(args_arr) = payload.get("args") {
                args_arr
            } else if let Some(params_val) = payload.get("params") {
                params_val
            } else {
                payload
            }
        } else {
            payload
        };

        if params.len() == 1 {
            if let Some(arr) = data.as_array() {
                if arr.len() == 1 {
                    return vec![crate::eval::val::json_to_nudgeval(arr[0].clone())];
                }
            }
            if let Some(obj) = data.as_object() {
                if let Some(val) = obj.get(&params[0]) {
                    return vec![crate::eval::val::json_to_nudgeval(val.clone())];
                }
            }
            return vec![crate::eval::val::json_to_nudgeval(data.clone())];
        }

        if let Some(arr) = data.as_array() {
            return arr
                .iter()
                .map(|v| crate::eval::val::json_to_nudgeval(v.clone()))
                .collect();
        }
        if let Some(obj) = data.as_object() {
            let mut out = Vec::with_capacity(params.len());
            for p in &params {
                let v = obj.get(p).cloned().unwrap_or(serde_json::Value::Null);
                out.push(crate::eval::val::json_to_nudgeval(v));
            }
            return out;
        }

        vec![crate::eval::val::json_to_nudgeval(data.clone())]
    }

    fn send_response(
        &self,
        stream: &mut TcpStream,
        code: u16,
        text: &str,
        content_type: &str,
        body: &[u8],
    ) -> std::io::Result<()> {
        let resp = format!(
            "HTTP/1.1 {code} {text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(resp.as_bytes())?;
        stream.write_all(body)?;
        stream.flush()?;
        Ok(())
    }

    fn send_json(
        &self,
        stream: &mut TcpStream,
        code: u16,
        val: &serde_json::Value,
    ) -> std::io::Result<()> {
        let body = serde_json::to_vec(val)?;
        let text = match code {
            200 => "OK",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            431 => "Request Header Fields Too Large",
            _ => "Error",
        };
        self.send_response(stream, code, text, "application/json; charset=utf-8", &body)
    }

    fn send_error(&self, stream: &mut TcpStream, code: u16, msg: &str) -> std::io::Result<()> {
        let err_json = serde_json::json!({ "error": msg, "status": "failed" });
        self.send_json(stream, code, &err_json)
    }

    fn handle_jsonrpc(&self, rpc_val: serde_json::Value) -> serde_json::Value {
        let id = rpc_val
            .get("id")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let Some(method) = rpc_val.get("method").and_then(|v| v.as_str()) else {
            return serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32600, "message": "Invalid Request: missing method" }
            });
        };
        let params = rpc_val
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        match method {
            "agent.card" | "agent.getCard" => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": self.card_json
                })
            }
            "tasks.create" | "task.create" => {
                let skill_name = params
                    .get("skill")
                    .or_else(|| params.get("name"))
                    .or_else(|| params.get("method"))
                    .or_else(|| params.get("skill_id"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| {
                        if self.whitelisted_skills.len() == 1 {
                            self.whitelisted_skills.iter().next().cloned()
                        } else {
                            None
                        }
                    });
                let Some(skill) = skill_name else {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": "Invalid params: missing skill name" }
                    });
                };
                if !self.whitelisted_skills.contains(&skill) {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("Method not found: skill '{skill}' is not exposed") }
                    });
                }
                let task_id = params
                    .get("id")
                    .or_else(|| params.get("task_id"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(generate_task_id);

                let is_async = params
                    .get("async")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let args = self.build_args(&skill, &params);
                let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.cancel_tokens
                    .write()
                    .unwrap()
                    .insert(task_id.clone(), cancel_token.clone());

                if is_async {
                    let rec = TaskRecord {
                        id: task_id.clone(),
                        status: "running".to_string(),
                        output: None,
                        error: None,
                        created_at: now_millis(),
                    };
                    self.insert_and_prune_task(task_id.clone(), rec);

                    let ctx = self.clone();
                    let tid = task_id.clone();
                    let sk = skill;
                    let ctok = cancel_token;
                    std::thread::spawn(move || {
                        if ctok.load(std::sync::atomic::Ordering::SeqCst) {
                            return;
                        }
                        let res = crate::eval::run_program(&ctx.items, &sk, &args);
                        let mut tasks = ctx.tasks.write().unwrap();
                        if let Some(t) = tasks.get_mut(&tid) {
                            if ctok.load(std::sync::atomic::Ordering::SeqCst)
                                || t.status == "cancelled"
                            {
                                t.status = "cancelled".to_string();
                                return;
                            }
                            match res {
                                Ok(val) => {
                                    t.status = "completed".to_string();
                                    t.output = Some(crate::eval::val::nudgeval_to_json(&val));
                                }
                                Err(e) => {
                                    t.status = "failed".to_string();
                                    t.error = Some(e);
                                }
                            }
                        }
                    });

                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "id": task_id,
                            "status": "running"
                        }
                    })
                } else {
                    let rec = TaskRecord {
                        id: task_id.clone(),
                        status: "running".to_string(),
                        output: None,
                        error: None,
                        created_at: now_millis(),
                    };
                    self.insert_and_prune_task(task_id.clone(), rec);

                    let res = crate::eval::run_program(&self.items, &skill, &args);
                    let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(t) = tasks.get_mut(&task_id) {
                        if cancel_token.load(std::sync::atomic::Ordering::SeqCst)
                            || t.status == "cancelled"
                        {
                            t.status = "cancelled".to_string();
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "id": task_id,
                                    "status": "cancelled"
                                }
                            })
                        } else {
                            match res {
                                Ok(val) => {
                                    let out_json = crate::eval::val::nudgeval_to_json(&val);
                                    t.status = "completed".to_string();
                                    t.output = Some(out_json.clone());
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "id": task_id,
                                            "status": "completed",
                                            "output": out_json
                                        }
                                    })
                                }
                                Err(e) => {
                                    t.status = "failed".to_string();
                                    t.error = Some(e.clone());
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": id,
                                        "result": {
                                            "id": task_id,
                                            "status": "failed",
                                            "error": e
                                        }
                                    })
                                }
                            }
                        }
                    } else {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32603,
                                "message": format!("task {task_id} missing from registry")
                            }
                        })
                    }
                }
            }
            "tasks.get" | "task.get" => {
                let task_id = if let Some(s) = params.as_str() {
                    Some(s)
                } else {
                    params
                        .get("id")
                        .or_else(|| params.get("task_id"))
                        .and_then(|v| v.as_str())
                };
                let Some(tid) = task_id else {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": "Invalid params: missing task id" }
                    });
                };
                if let Some(task) = self.tasks.read().unwrap().get(tid) {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": task
                    })
                } else {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": format!("Task '{tid}' not found") }
                    })
                }
            }
            "tasks.cancel" | "task.cancel" => {
                let task_id = if let Some(s) = params.as_str() {
                    Some(s)
                } else {
                    params
                        .get("id")
                        .or_else(|| params.get("task_id"))
                        .and_then(|v| v.as_str())
                };
                let Some(tid) = task_id else {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": "Invalid params: missing task id" }
                    });
                };
                let mut tasks = self.tasks.write().unwrap();
                if let Some(task) = tasks.get_mut(tid) {
                    task.status = "cancelled".to_string();
                    if let Some(tok) = self.cancel_tokens.read().unwrap().get(tid) {
                        tok.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "id": tid,
                            "status": "cancelled"
                        }
                    })
                } else {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": format!("Task '{tid}' not found") }
                    })
                }
            }
            "tasks.list" | "task.list" => {
                let tasks: Vec<TaskRecord> = self.tasks.read().unwrap().values().cloned().collect();
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tasks": tasks }
                })
            }
            other => {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Method not found: '{other}'") }
                })
            }
        }
    }

    fn execute_task_and_reply(
        &self,
        stream: &mut TcpStream,
        task_id: String,
        skill: &str,
        args: &[NudgeVal],
        is_async: bool,
    ) -> std::io::Result<()> {
        let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_tokens
            .write()
            .unwrap()
            .insert(task_id.clone(), cancel_token.clone());

        if is_async {
            let rec = TaskRecord {
                id: task_id.clone(),
                status: "running".to_string(),
                output: None,
                error: None,
                created_at: now_millis(),
            };
            self.insert_and_prune_task(task_id.clone(), rec);

            let ctx = self.clone();
            let tid = task_id.clone();
            let sk = skill.to_string();
            let args_vec = args.to_vec();
            let ctok = cancel_token;
            std::thread::spawn(move || {
                if ctok.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let res = crate::eval::run_program(&ctx.items, &sk, &args_vec);
                let mut tasks = ctx.tasks.write().unwrap();
                if let Some(t) = tasks.get_mut(&tid) {
                    if ctok.load(std::sync::atomic::Ordering::SeqCst) || t.status == "cancelled" {
                        t.status = "cancelled".to_string();
                        return;
                    }
                    match res {
                        Ok(val) => {
                            t.status = "completed".to_string();
                            t.output = Some(crate::eval::val::nudgeval_to_json(&val));
                        }
                        Err(e) => {
                            t.status = "failed".to_string();
                            t.error = Some(e);
                        }
                    }
                }
            });

            self.send_json(
                stream,
                200,
                &serde_json::json!({
                    "id": task_id,
                    "task_id": task_id,
                    "status": "running"
                }),
            )
        } else {
            let rec = TaskRecord {
                id: task_id.clone(),
                status: "running".to_string(),
                output: None,
                error: None,
                created_at: now_millis(),
            };
            self.insert_and_prune_task(task_id.clone(), rec);

            let res = crate::eval::run_program(&self.items, skill, args);
            let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = tasks.get_mut(&task_id) {
                if cancel_token.load(std::sync::atomic::Ordering::SeqCst) || t.status == "cancelled"
                {
                    t.status = "cancelled".to_string();
                    self.send_json(
                        stream,
                        200,
                        &serde_json::json!({
                            "id": task_id,
                            "task_id": task_id,
                            "status": "cancelled"
                        }),
                    )
                } else {
                    match res {
                        Ok(val) => {
                            let out_json = crate::eval::val::nudgeval_to_json(&val);
                            t.status = "completed".to_string();
                            t.output = Some(out_json.clone());
                            self.send_json(
                                stream,
                                200,
                                &serde_json::json!({
                                    "id": task_id,
                                    "task_id": task_id,
                                    "status": "completed",
                                    "output": out_json
                                }),
                            )
                        }
                        Err(e) => {
                            t.status = "failed".to_string();
                            t.error = Some(e.clone());
                            self.send_json(
                                stream,
                                500,
                                &serde_json::json!({
                                    "id": task_id,
                                    "task_id": task_id,
                                    "status": "failed",
                                    "error": e
                                }),
                            )
                        }
                    }
                }
            } else {
                self.send_json(
                    stream,
                    500,
                    &serde_json::json!({
                        "error": {
                            "code": 500,
                            "message": format!("task {task_id} missing from registry")
                        }
                    }),
                )
            }
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(30)));

        let mut head_buf = Vec::new();
        let mut buf = [0u8; 2048];

        let head_end = loop {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            head_buf.extend_from_slice(&buf[..n]);
            if let Some(pos) = head_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            } else if let Some(pos) = head_buf.windows(2).position(|w| w == b"\n\n") {
                break pos + 2;
            }
            if head_buf.len() > 65536 {
                return self.send_error(&mut stream, 431, "Request Header Fields Too Large");
            }
        };

        let header_str = String::from_utf8_lossy(&head_buf[..head_end]).to_string();
        let mut lines = header_str.lines();
        let req_line = lines.next().unwrap_or("");
        let mut parts = req_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_uppercase();
        let full_path = parts.next().unwrap_or("/");

        let path = full_path
            .split('?')
            .next()
            .unwrap_or(full_path)
            .trim_end_matches('/');
        let path = if path.is_empty() { "/" } else { path };

        let mut headers = HashMap::new();
        let mut content_length: usize = 0;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                let k_lower = k.trim().to_lowercase();
                let val = v.trim().to_string();
                if k_lower == "content-length" {
                    content_length = val.parse().unwrap_or(0);
                }
                headers.insert(k_lower, val);
            }
        }

        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nConnection: close\r\n\r\n";
            stream.write_all(resp.as_bytes())?;
            stream.flush()?;
            return Ok(());
        }

        // Security: Request body cap (2MB max)
        if content_length > MAX_BODY_SIZE {
            let _ = self.send_error(
                &mut stream,
                413,
                "Payload Too Large: maximum allowed body size is 2MB",
            );
            let mut drain_buf = [0u8; 16384];
            let mut remaining = content_length.saturating_sub(head_buf.len() - head_end);
            while remaining > 0 {
                let to_read = remaining.min(drain_buf.len());
                match stream.read(&mut drain_buf[..to_read]) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => remaining = remaining.saturating_sub(n),
                }
            }
            return Ok(());
        }

        let mut body = Vec::new();
        if head_end < head_buf.len() {
            body.extend_from_slice(&head_buf[head_end..]);
        }

        while body.len() < content_length {
            let to_read = (content_length - body.len()).min(buf.len());
            let n = stream.read(&mut buf[..to_read])?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
            if body.len() > MAX_BODY_SIZE {
                return self.send_error(
                    &mut stream,
                    413,
                    "Payload Too Large: maximum allowed body size is 2MB",
                );
            }
        }

        // Security: Authentication via NUDGE_A2A_API_KEY
        if let Some(ref expected_key) = self.api_key {
            let auth = headers
                .get("authorization")
                .map(|s| s.as_str())
                .unwrap_or("");
            let expected_auth = format!("Bearer {expected_key}");
            if auth != expected_auth {
                if path == "/rpc" || path == "/" && body_is_jsonrpc(&body) {
                    return self.send_response(
                        &mut stream,
                        401,
                        "Unauthorized",
                        "application/json; charset=utf-8",
                        br#"{"jsonrpc": "2.0", "id": null, "error": {"code": -32001, "message": "Unauthorized: invalid or missing Bearer token"}}"#,
                    );
                } else {
                    return self.send_error(
                        &mut stream,
                        401,
                        "Unauthorized: invalid or missing Bearer token",
                    );
                }
            }
        }

        // Route: JSON-RPC 2.0 (POST /rpc or POST /)
        if method == "POST" && (path == "/rpc" || path == "/" && body_is_jsonrpc(&body)) {
            match serde_json::from_slice::<serde_json::Value>(&body) {
                Ok(rpc_req) => {
                    let rpc_resp = self.handle_jsonrpc(rpc_req);
                    return self.send_json(&mut stream, 200, &rpc_resp);
                }
                Err(e) => {
                    let err_resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {e}") }
                    });
                    return self.send_json(&mut stream, 200, &err_resp);
                }
            }
        }

        // Route: REST
        match method.as_str() {
            "GET" => {
                if path == "/.well-known/agent.json" || path == "/agent.json" {
                    self.send_json(&mut stream, 200, &self.card_json)
                } else if path == "/health" {
                    let resp = serde_json::json!({
                        "status": "ok",
                        "agent": self.agent_name,
                        "version": "1.0.0"
                    });
                    self.send_json(&mut stream, 200, &resp)
                } else if let Some(stripped) = path.strip_prefix("/tasks/") {
                    let task_id = stripped.trim_matches('/');
                    if let Some(t) = self.tasks.read().unwrap().get(task_id) {
                        let resp = serde_json::to_value(t).unwrap_or(serde_json::Value::Null);
                        self.send_json(&mut stream, 200, &resp)
                    } else {
                        self.send_json(
                            &mut stream,
                            404,
                            &serde_json::json!({
                                "status": "failed",
                                "error": format!("task '{task_id}' not found")
                            }),
                        )
                    }
                } else if path == "/tasks" {
                    let tasks: Vec<TaskRecord> =
                        self.tasks.read().unwrap().values().cloned().collect();
                    self.send_json(&mut stream, 200, &serde_json::json!({ "tasks": tasks }))
                } else {
                    self.send_error(&mut stream, 404, "Not Found")
                }
            }
            "POST" => {
                if let Some(rest) = path.strip_prefix("/tasks/") {
                    if let Some(task_id_raw) = rest.strip_suffix("/cancel") {
                        let task_id = task_id_raw.trim_matches('/');
                        let mut tasks = self.tasks.write().unwrap();
                        if let Some(t) = tasks.get_mut(task_id) {
                            t.status = "cancelled".to_string();
                            if let Some(tok) = self.cancel_tokens.read().unwrap().get(task_id) {
                                tok.store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                            return self.send_json(
                                &mut stream,
                                200,
                                &serde_json::json!({ "id": task_id, "status": "cancelled" }),
                            );
                        } else {
                            return self.send_json(
                                &mut stream,
                                404,
                                &serde_json::json!({
                                    "status": "failed",
                                    "error": format!("task '{task_id}' not found")
                                }),
                            );
                        }
                    }
                }
                if path == "/tasks" {
                    let req_val: serde_json::Value = match serde_json::from_slice(&body) {
                        Ok(v) => v,
                        Err(e) => {
                            return self.send_json(
                                &mut stream,
                                400,
                                &serde_json::json!({
                                    "status": "failed",
                                    "error": format!("bad JSON: {e}")
                                }),
                            );
                        }
                    };
                    let skill_name = req_val
                        .get("skill")
                        .or_else(|| req_val.get("skill_name"))
                        .or_else(|| req_val.get("name"))
                        .or_else(|| req_val.get("method"))
                        .or_else(|| req_val.get("action"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| {
                            if self.whitelisted_skills.len() == 1 {
                                self.whitelisted_skills.iter().next().cloned()
                            } else {
                                None
                            }
                        });
                    let Some(skill) = skill_name else {
                        return self.send_json(
                            &mut stream,
                            400,
                            &serde_json::json!({
                                "status": "failed",
                                "error": "missing skill name in task creation"
                            }),
                        );
                    };

                    // Skill Whitelist Check
                    if !self.whitelisted_skills.contains(&skill) {
                        return self.send_json(
                            &mut stream,
                            404,
                            &serde_json::json!({
                                "status": "failed",
                                "error": format!("skill '{skill}' not found")
                            }),
                        );
                    }

                    let task_id = req_val
                        .get("id")
                        .or_else(|| req_val.get("task_id"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(generate_task_id);

                    let is_async = req_val
                        .get("async")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let args = self.build_args(&skill, &req_val);
                    self.execute_task_and_reply(&mut stream, task_id, &skill, &args, is_async)
                } else if let Some(skill_raw) = path.strip_prefix("/skills/") {
                    let skill = skill_raw.trim_matches('/');
                    // Skill Whitelist Check
                    if !self.whitelisted_skills.contains(skill) {
                        return self.send_json(
                            &mut stream,
                            404,
                            &serde_json::json!({
                                "status": "failed",
                                "error": format!("skill '{skill}' not found")
                            }),
                        );
                    }
                    let req_val: serde_json::Value = if body.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
                    };
                    let task_id = req_val
                        .get("id")
                        .or_else(|| req_val.get("task_id"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(generate_task_id);

                    let is_async = req_val
                        .get("async")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let args = self.build_args(skill, &req_val);
                    self.execute_task_and_reply(&mut stream, task_id, skill, &args, is_async)
                } else {
                    self.send_error(&mut stream, 404, "Not Found")
                }
            }
            _ => self.send_error(&mut stream, 405, "Method Not Allowed"),
        }
    }
}

pub struct NativeServer {
    listener: TcpListener,
    port: u16,
    context: ServerContext,
}

impl NativeServer {
    pub fn bind(items: &[Item], port: u16, bind_addr: Option<&str>) -> Result<Self, String> {
        let api_key = std::env::var("NUDGE_A2A_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());
        Self::bind_with_key(items, port, bind_addr, api_key)
    }

    pub fn bind_with_key(
        items: &[Item],
        port: u16,
        bind_addr: Option<&str>,
        api_key: Option<String>,
    ) -> Result<Self, String> {
        let addr = format!("{}:{}", bind_addr.unwrap_or("127.0.0.1"), port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("cannot bind A2A server to {addr}: {e}"))?;
        let actual_port = listener
            .local_addr()
            .map_err(|e| format!("cannot get local addr: {e}"))?
            .port();

        let stem = "agent";
        let cards = cards_with_url(items, stem, &format!("http://127.0.0.1:{actual_port}/"));
        let (card_agent_stem, card_json_val) = cards
            .into_iter()
            .next()
            .unwrap_or_else(|| (stem.to_string(), card(stem, vec![])));
        let card_raw_str = dumps(&card_json_val);
        let card_json: serde_json::Value =
            serde_json::from_str(&card_raw_str).map_err(|e| format!("invalid card JSON: {e}"))?;

        let agent_name = card_json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&card_agent_stem)
            .to_string();

        let mut whitelisted_skills = HashSet::new();
        let mut skill_params = HashMap::new();

        if let Some(skills_arr) = card_json.get("skills").and_then(|v| v.as_array()) {
            for sk in skills_arr {
                if let Some(id) = sk.get("id").and_then(|v| v.as_str()) {
                    whitelisted_skills.insert(id.to_string());
                }
                if let Some(name) = sk.get("name").and_then(|v| v.as_str()) {
                    whitelisted_skills.insert(name.to_string());
                }
            }
        }

        for item in items {
            match item {
                Item::Fn { name, params, .. } => {
                    let ps = params.iter().map(|p| p.name.clone()).collect();
                    skill_params.insert(name.clone(), ps);
                    whitelisted_skills.insert(name.clone());
                }
                Item::Agent { fns, .. } => {
                    for f in fns {
                        if let Item::Fn { name, params, .. } = f {
                            let ps = params.iter().map(|p| p.name.clone()).collect();
                            skill_params.insert(name.clone(), ps);
                            whitelisted_skills.insert(name.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        let context = ServerContext {
            items: items.to_vec(),
            card_json,
            agent_name,
            whitelisted_skills,
            skill_params,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
            api_key,
            active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max_connections: 128,
            max_tasks: 5000,
        };

        Ok(NativeServer {
            listener,
            port: actual_port,
            context,
        })
    }

    pub fn set_max_connections(&mut self, limit: usize) {
        self.context.max_connections = limit;
    }

    pub fn set_max_tasks(&mut self, limit: usize) {
        self.context.max_tasks = limit;
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn agent_name(&self) -> &str {
        &self.context.agent_name
    }

    pub fn serve(&self) -> Result<(), String> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let active = self
                        .context
                        .active_connections
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if active >= self.context.max_connections {
                        self.context
                            .active_connections
                            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        let resp = "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nServer Busy: max concurrent connections reached";
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        let mut drain = [0u8; 1024];
                        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(50)));
                        while let Ok(n) = stream.read(&mut drain) {
                            if n == 0 {
                                break;
                            }
                        }
                        continue;
                    }
                    let counter = self.context.active_connections.clone();
                    let ctx = self.context.clone();
                    std::thread::spawn(move || {
                        struct ConnGuard(Arc<std::sync::atomic::AtomicUsize>);
                        impl Drop for ConnGuard {
                            fn drop(&mut self) {
                                self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                        let _guard = ConnGuard(counter);
                        let _ = ctx.handle_connection(stream);
                    });
                }
                Err(e) => {
                    return Err(format!("accept error: {e}"));
                }
            }
        }
        Ok(())
    }

    pub fn serve_with_shutdown(
        &self,
        shutdown_rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<(), String> {
        let _ = self.listener.set_nonblocking(true);
        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let active = self
                        .context
                        .active_connections
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if active >= self.context.max_connections {
                        self.context
                            .active_connections
                            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        let resp = "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nServer Busy: max concurrent connections reached";
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        let mut drain = [0u8; 1024];
                        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(50)));
                        while let Ok(n) = stream.read(&mut drain) {
                            if n == 0 {
                                break;
                            }
                        }
                        continue;
                    }
                    let counter = self.context.active_connections.clone();
                    let ctx = self.context.clone();
                    std::thread::spawn(move || {
                        struct ConnGuard(Arc<std::sync::atomic::AtomicUsize>);
                        impl Drop for ConnGuard {
                            fn drop(&mut self) {
                                self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                        let _guard = ConnGuard(counter);
                        let _ = ctx.handle_connection(stream);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(format!("accept error: {e}"));
                }
            }
        }
        Ok(())
    }
}

pub fn run_native_server(items: &[Item], port: u16, bind_addr: Option<&str>) -> Result<(), String> {
    let server = NativeServer::bind(items, port, bind_addr)?;
    println!(
        "A2A agent '{}' serving on http://127.0.0.1:{}",
        server.agent_name(),
        server.port()
    );
    println!(
        "  REST:     GET /.well-known/agent.json, GET /health, POST /tasks, POST /skills/{{name}}"
    );
    println!("  JSON-RPC: POST /rpc, POST /");
    server.serve()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::dumps;
    use crate::lexer::lex;
    use crate::parser::parse;

    #[test]
    fn agent_block_exports_an_a2a_card() {
        let src = "agent Researcher {\n    state {\n        round: int = 0,\n    }\n    fn step(q: string) -> string uses LLM {\n        llm\"\"\"go {q}\"\"\" with { model: \"m\" }\n    }\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        let cs = cards(&items, "demo");
        assert_eq!(cs.len(), 1);
        let (stem, c) = &cs[0];
        assert_eq!(stem, "researcher");
        let s = dumps(c);
        assert!(s.contains("\"name\": \"Researcher\""), "{s}");
        assert!(s.contains("\"version\": \"1.0.0\""), "{s}");
        assert!(s.contains("\"stateTransitionHistory\": true"), "{s}");
        assert!(s.contains("\"id\": \"step\""), "{s}");
        assert!(s.contains("step(q: string) -> string uses LLM"), "{s}");
        assert!(s.contains("\"tags\": [\"llm\"]"), "{s}");
        // the emitted card is valid JSON
        assert!(crate::json::parse(&s).unwrap().is_obj());
    }

    #[test]
    fn a_file_without_agents_exports_one_card_for_its_fns() {
        let src = "fn run(q: string) -> [string] uses Tool {\n    web(q)\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        let cs = cards(&items, "research_agent");
        assert_eq!(cs.len(), 1);
        let s = dumps(&cs[0].1);
        assert!(s.contains("\"name\": \"research_agent\""), "{s}");
        assert!(s.contains("run(q: string) -> [string] uses Tool"), "{s}");
        assert!(s.contains("\"defaultInputModes\": [\"text\"]"), "{s}");
    }

    #[test]
    fn cards_with_url_sets_custom_url() {
        let src = "agent EchoBot {\n    fn echo(s: string) -> string {\n        s\n    }\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        let cs = cards_with_url(&items, "echobot", "http://127.0.0.1:9090/");
        assert_eq!(cs.len(), 1);
        let s = dumps(&cs[0].1);
        assert!(s.contains("\"url\": \"http://127.0.0.1:9090/\""), "{s}");
    }

    #[test]
    fn server_source_generates_script() {
        let src = server_source(
            "/path/to/mod.py",
            "TestAgent",
            "{\"name\":\"TestAgent\"}",
            9000,
        );
        assert!(src.contains("AGENT_NAME = AGENT_CARD.get(\"name\") or \"TestAgent\""));
        assert!(src.contains("PORT = 9000"));
        assert!(src.contains("/.well-known/agent.json"));
        assert!(src.contains("/health"));
        assert!(src.contains("/tasks"));
        assert!(src.contains("/skills/"));
    }

    fn http_request(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
        http_request_raw(port, method, path, &[], body.map(|s| s.as_bytes()))
    }

    fn http_request_raw(
        port: u16,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> (u16, String) {
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to server");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let body_bytes = body.unwrap_or(b"");
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            body_bytes.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        stream.write_all(req.as_bytes()).unwrap();
        if !body_bytes.is_empty() {
            stream.write_all(body_bytes).unwrap();
        }
        stream.flush().unwrap();

        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        let resp_str = String::from_utf8_lossy(&resp).to_string();

        let mut lines = resp_str.lines();
        let status_line = lines.next().unwrap_or("");
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let resp_body = if let Some(idx) = resp_str.find("\r\n\r\n") {
            &resp_str[idx + 4..]
        } else if let Some(idx) = resp_str.find("\n\n") {
            &resp_str[idx + 2..]
        } else {
            ""
        };
        (status_code, resp_body.to_string())
    }

    #[test]
    fn test_native_server_rest_and_jsonrpc() {
        let src = r#"agent Calculator {
    fn add(a: int, b: int) -> int {
        a + b
    }
    fn multiply(x: int, y: int) -> int {
        x * y
    }
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // 1. REST GET /health
        let (code, body) = http_request(port, "GET", "/health", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"ok\""), "{body}");
        assert!(body.contains("\"agent\":\"Calculator\""), "{body}");

        // 2. REST GET /.well-known/agent.json and /agent.json
        let (code, body) = http_request(port, "GET", "/.well-known/agent.json", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"name\":\"Calculator\""), "{body}");
        let (code, body) = http_request(port, "GET", "/agent.json", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"name\":\"Calculator\""), "{body}");

        // 3. REST POST /tasks
        let (code, body) = http_request(
            port,
            "POST",
            "/tasks",
            Some(r#"{"skill": "add", "input": [18, 24], "id": "task-test-1"}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"completed\""), "{body}");
        assert!(body.contains("\"output\":42"), "{body}");
        assert!(body.contains("\"id\":\"task-test-1\""), "{body}");

        // 4. REST GET /tasks/task-test-1
        let (code, body) = http_request(port, "GET", "/tasks/task-test-1", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"completed\""), "{body}");
        assert!(body.contains("\"output\":42"), "{body}");

        // 5. REST POST /skills/multiply
        let (code, body) = http_request(
            port,
            "POST",
            "/skills/multiply",
            Some(r#"{"x": 6, "y": 7}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"completed\""), "{body}");
        assert!(body.contains("\"output\":42"), "{body}");

        // 6. REST cancel task
        let (code, body) = http_request(port, "POST", "/tasks/task-test-1/cancel", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"cancelled\""), "{body}");

        // 7. JSON-RPC POST /rpc: agent.card
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(r#"{"jsonrpc": "2.0", "id": 1, "method": "agent.card"}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"name\":\"Calculator\""), "{body}");
        assert!(body.contains("\"id\":1"), "{body}");

        // 8. JSON-RPC POST /: tasks.create
        let (code, body) = http_request(
            port,
            "POST",
            "/",
            Some(
                r#"{"jsonrpc": "2.0", "id": 2, "method": "tasks.create", "params": {"skill": "multiply", "input": [9, 9]}}"#,
            ),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"output\":81"), "{body}");
        let rpc_val: serde_json::Value = serde_json::from_str(&body).unwrap();
        let tid = rpc_val["result"]["id"].as_str().unwrap().to_string();

        // 9. JSON-RPC POST /rpc: tasks.get
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(&format!(
                r#"{{"jsonrpc": "2.0", "id": 3, "method": "tasks.get", "params": {{"id": "{tid}"}}}}"#
            )),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"output\":81"), "{body}");

        // 10. JSON-RPC POST /rpc: tasks.cancel
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(&format!(
                r#"{{"jsonrpc": "2.0", "id": 4, "method": "tasks.cancel", "params": {{"id": "{tid}"}}}}"#
            )),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"cancelled\""), "{body}");

        // 11. JSON-RPC unknown method
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(r#"{"jsonrpc": "2.0", "id": 5, "method": "nonexistent"}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("-32601"), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_security_body_cap_413() {
        let src = r#"fn ping() -> string { "pong" }"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        let large_payload = vec![b'a'; 2_200_000]; // 2.2MB > 2MB limit
        let (code, body) = http_request_raw(port, "POST", "/tasks", &[], Some(&large_payload));
        assert_eq!(code, 413);
        assert!(body.contains("Payload Too Large"), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_security_auth_401() {
        let src = r#"fn ping() -> string { "pong" }"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server =
            NativeServer::bind_with_key(&items, 0, None, Some("secret-test-token".to_string()))
                .expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // 1. Missing auth header -> 401
        let (code, body) = http_request(port, "GET", "/health", None);
        assert_eq!(code, 401);
        assert!(body.contains("Unauthorized"), "{body}");

        // 2. Wrong auth header -> 401
        let (code, body) = http_request_raw(
            port,
            "GET",
            "/health",
            &[("Authorization", "Bearer wrong-token")],
            None,
        );
        assert_eq!(code, 401);
        assert!(body.contains("Unauthorized"), "{body}");

        // 3. Correct auth header -> 200
        let (code, body) = http_request_raw(
            port,
            "GET",
            "/health",
            &[("Authorization", "Bearer secret-test-token")],
            None,
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"ok\""), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_security_skill_whitelist() {
        let src = r#"agent PublicAgent {
    fn allowed() -> string {
        "ok"
    }
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // 1. REST unwhitelisted skill -> 404
        let (code, body) = http_request(port, "POST", "/skills/secret_eval", Some("{}"));
        assert_eq!(code, 404);
        assert!(body.contains("skill 'secret_eval' not found"), "{body}");

        // 2. JSON-RPC unwhitelisted skill -> -32601
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(
                r#"{"jsonrpc": "2.0", "id": 10, "method": "tasks.create", "params": {"skill": "secret_eval"}}"#,
            ),
        );
        assert_eq!(code, 200);
        assert!(body.contains("-32601"), "{body}");
        assert!(body.contains("is not exposed"), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_concurrency() {
        let src = r#"fn compute(x: int) -> int {
    x * 10
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        let mut client_threads = Vec::new();
        for i in 1..=5 {
            let p = port;
            let th = std::thread::spawn(move || {
                let payload = format!(r#"{{"skill": "compute", "input": {i}}}"#);
                let (code, body) = http_request(p, "POST", "/tasks", Some(&payload));
                assert_eq!(code, 200);
                assert!(body.contains(&format!("\"output\":{}", i * 10)), "{body}");
            });
            client_threads.push(th);
        }

        for th in client_threads {
            th.join().unwrap();
        }

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn a2a_server_integration_endpoints() {
        use std::io::BufRead;
        use std::process::{Command, Stdio};

        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let temp_dir = std::env::temp_dir().join(format!("nudge_a2a_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let src = r#"agent MathAgent {
    fn double(x: int) -> int {
        x * 2
    }
    fn greet(name: string) -> string {
        "hello " + name
    }
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let py_code = crate::codegen::emit(&items);
        let mod_path = temp_dir.join("math_agent.py");
        std::fs::write(&mod_path, py_code).unwrap();

        let cards = cards_with_url(&items, "math_agent", "http://127.0.0.1:0/");
        let (agent_name, card_val) = &cards[0];
        let card_json = dumps(card_val);

        let srv_code = server_source(&mod_path.to_string_lossy(), agent_name, &card_json, 0);
        let srv_path = temp_dir.join("math_agent_server.py");
        std::fs::write(&srv_path, srv_code).unwrap();

        let mut cmd = Command::new("python3");
        cmd.arg(&srv_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        for candidate in &["runtime", "../runtime", "../../runtime", "../../../runtime"] {
            let p = std::path::Path::new(candidate);
            if p.join("nudge_runtime").is_dir() {
                if let Ok(abs_rt) = p.canonicalize() {
                    cmd.env("PYTHONPATH", abs_rt);
                    break;
                }
            }
        }
        cmd.env("NUDGE_PROVIDER", "fake");

        let mut child = cmd.spawn().expect("spawn a2a server");

        let stdout = child.stdout.take().expect("capture stdout");
        let mut reader = std::io::BufReader::new(stdout);
        let mut port: Option<u16> = None;

        let mut line = String::new();
        for _ in 0..100 {
            line.clear();
            if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                if let Some(pos) = line.find("http://127.0.0.1:") {
                    let port_str = line[pos + "http://127.0.0.1:".len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    if let Ok(p) = port_str.parse::<u16>() {
                        port = Some(p);
                        break;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let port = port.expect("failed to obtain server port from stdout");

        // 1. GET /.well-known/agent.json
        let (code, body) = http_request(port, "GET", "/.well-known/agent.json", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"name\": \"MathAgent\""), "{body}");
        assert!(body.contains("\"skills\""), "{body}");
        assert!(body.contains("\"double\""), "{body}");

        // 2. GET /agent.json
        let (code, body) = http_request(port, "GET", "/agent.json", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"name\": \"MathAgent\""), "{body}");

        // 3. GET /health
        let (code, body) = http_request(port, "GET", "/health", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"ok\""), "{body}");
        assert!(body.contains("\"agent\": \"MathAgent\""), "{body}");
        assert!(body.contains("\"version\": \"1.0.0\""), "{body}");

        // 4. POST /tasks: dispatch double
        let (code, body) = http_request(
            port,
            "POST",
            "/tasks",
            Some("{\"skill\": \"double\", \"input\": 21, \"id\": \"task-42\"}"),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"completed\""), "{body}");
        assert!(body.contains("\"output\": 42"), "{body}");
        assert!(body.contains("\"id\": \"task-42\""), "{body}");

        // 5. GET /tasks/task-42: task status endpoint
        let (code, body) = http_request(port, "GET", "/tasks/task-42", None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"completed\""), "{body}");
        assert!(body.contains("\"output\": 42"), "{body}");

        // 6. POST /skills/greet
        let (code, body) =
            http_request(port, "POST", "/skills/greet", Some("{\"name\": \"world\"}"));
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"completed\""), "{body}");
        assert!(body.contains("\"output\": \"hello world\""), "{body}");

        // 7. GET /tasks/unknown -> 404
        let (code, body) = http_request(port, "GET", "/tasks/unknown-id", None);
        assert_eq!(code, 404);
        assert!(body.contains("\"status\": \"failed\""), "{body}");

        // 8. POST /skills/unknown_skill -> 404
        let (code, body) = http_request(port, "POST", "/skills/unknown_skill", Some("{}"));
        assert_eq!(code, 404);
        assert!(body.contains("\"status\": \"failed\""), "{body}");

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_native_server_async_task_execution_and_cancellation() {
        let src = r#"agent MathWorker {
    fn compute(x: int) -> int {
        x * 10
    }
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // 1. Async task creation: status "running" returned immediately
        let (code, body) = http_request(
            port,
            "POST",
            "/tasks",
            Some(r#"{"skill": "compute", "input": 4, "async": true}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"running\""), "{body}");
        let val: serde_json::Value = serde_json::from_str(&body).unwrap();
        let task_id = val["id"].as_str().unwrap().to_string();

        // Poll for task completion
        let mut completed = false;
        for _ in 0..50 {
            let (code, body) = http_request(port, "GET", &format!("/tasks/{task_id}"), None);
            assert_eq!(code, 200);
            if body.contains("\"status\":\"completed\"") {
                assert!(body.contains("\"output\":40"), "{body}");
                completed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(completed, "async task should complete");

        // 2. Cancellation of a task
        let (code, body) = http_request(
            port,
            "POST",
            "/tasks",
            Some(r#"{"skill": "compute", "input": 5, "async": true}"#),
        );
        assert_eq!(code, 200);
        let val: serde_json::Value = serde_json::from_str(&body).unwrap();
        let cancel_id = val["id"].as_str().unwrap().to_string();

        let (code, body) = http_request(port, "POST", &format!("/tasks/{cancel_id}/cancel"), None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"cancelled\""), "{body}");

        let (code, body) = http_request(port, "GET", &format!("/tasks/{cancel_id}"), None);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"cancelled\""), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_jsonrpc_aliases() {
        let src = r#"agent MathWorker {
    fn compute(x: int) -> int {
        x * 10
    }
}"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let server = NativeServer::bind(&items, 0, None).expect("bind server");
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // 1. agent.getCard alias
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(r#"{"jsonrpc": "2.0", "id": 1, "method": "agent.getCard"}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"name\":\"MathWorker\""), "{body}");

        // 2. task.create alias
        let (code, body) = http_request(
            port,
            "POST",
            "/",
            Some(
                r#"{"jsonrpc": "2.0", "id": 2, "method": "task.create", "params": {"skill": "compute", "input": [7]}}"#,
            ),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"output\":70"), "{body}");
        let val: serde_json::Value = serde_json::from_str(&body).unwrap();
        let tid = val["result"]["id"].as_str().unwrap().to_string();

        // 3. task.get alias
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(&format!(
                r#"{{"jsonrpc": "2.0", "id": 3, "method": "task.get", "params": {{"id": "{tid}"}}}}"#
            )),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"output\":70"), "{body}");

        // 4. task.cancel alias
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(&format!(
                r#"{{"jsonrpc": "2.0", "id": 4, "method": "task.cancel", "params": {{"id": "{tid}"}}}}"#
            )),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"status\":\"cancelled\""), "{body}");

        // 5. task.list alias
        let (code, body) = http_request(
            port,
            "POST",
            "/rpc",
            Some(r#"{"jsonrpc": "2.0", "id": 5, "method": "task.list"}"#),
        );
        assert_eq!(code, 200);
        assert!(body.contains("\"tasks\""), "{body}");

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_max_connections_503() {
        let src = r#"fn ping() -> string { "pong" }"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let mut server = NativeServer::bind(&items, 0, None).expect("bind server");
        server.set_max_connections(1);
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // Occupy the only allowed connection
        let mut conn1 = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect conn1");
        conn1
            .set_read_timeout(Some(std::time::Duration::from_millis(100)))
            .unwrap();
        conn1.write_all(b"POST /health HTTP/1.1\r\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second connection exceeds limit (max 1) -> must get 503 Service Unavailable
        let (code, body) = http_request(port, "GET", "/health", None);
        assert_eq!(code, 503);
        assert!(body.contains("Server Busy"), "{body}");

        drop(conn1);
        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }

    #[test]
    fn test_native_server_task_history_pruning() {
        let src = r#"fn ping() -> string { "pong" }"#;
        let items = parse(lex(src).unwrap()).unwrap();
        let mut server = NativeServer::bind(&items, 0, None).expect("bind server");
        server.set_max_tasks(3);
        let port = server.port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            server.serve_with_shutdown(rx).unwrap();
        });

        // Run 4 tasks: tasks 1, 2, 3, 4
        for i in 1..=4 {
            let (code, body) = http_request(
                port,
                "POST",
                "/tasks",
                Some(&format!(r#"{{"skill": "ping", "id": "task-prune-{i}"}}"#)),
            );
            assert_eq!(code, 200);
            assert!(body.contains("\"status\":\"completed\""), "{body}");
        }

        // Oldest task (task-prune-1) should be pruned FIFO
        let (code1, _) = http_request(port, "GET", "/tasks/task-prune-1", None);
        assert_eq!(code1, 404);

        // Newer tasks should remain
        let (code2, _) = http_request(port, "GET", "/tasks/task-prune-2", None);
        assert_eq!(code2, 200);
        let (code4, _) = http_request(port, "GET", "/tasks/task-prune-4", None);
        assert_eq!(code4, 200);

        tx.send(()).unwrap();
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        handle.join().unwrap();
    }
}
