//! A2A agent-card export (design §9, v1.0) and A2A protocol serving (v2.0):
//! `nudgec a2a <file.ndg>` emits `out/<name>.agent.json`.
//! `nudgec a2a-serve <file.ndg> [--port N]` serves the agent as a network-addressable
//! A2A peer with agent card, health, and skill execution endpoints.

use crate::ast::*;
use crate::json::Json;

pub const DEFAULT_PORT: u16 = 9000;

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
        use std::io::{Read, Write};
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to server");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let body_bytes = body.unwrap_or("").as_bytes();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body_bytes.len(),
            body.unwrap_or("")
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();

        let mut lines = resp.lines();
        let status_line = lines.next().unwrap_or("");
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let resp_body = if let Some(idx) = resp.find("\r\n\r\n") {
            &resp[idx + 4..]
        } else if let Some(idx) = resp.find("\n\n") {
            &resp[idx + 2..]
        } else {
            ""
        };
        (status_code, resp_body.to_string())
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
}
