use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("mock_mcp: invalid json: {e}");
                continue;
            }
        };

        let method = msg.get("method").and_then(|m| m.as_str());
        let id = msg.get("id");

        match method {
            Some("initialize") => {
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": {
                                "tools": {}
                            },
                            "serverInfo": {
                                "name": "mock",
                                "version": "0"
                            }
                        }
                    });
                    if writeln!(out, "{}", resp).is_err() || out.flush().is_err() {
                        break;
                    }
                }
            }
            Some("notifications/initialized") => {
                // Notifications do not have an id and expect no response
            }
            Some("tools/list") => {
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [
                                {
                                    "name": "web",
                                    "description": "Search the web",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "q": { "type": "string" }
                                        },
                                        "required": ["q"]
                                    }
                                }
                            ]
                        }
                    });
                    if writeln!(out, "{}", resp).is_err() || out.flush().is_err() {
                        break;
                    }
                }
            }
            Some("tools/call") => {
                if let Some(id) = id {
                    let params = msg.get("params");
                    let tool_name = params
                        .and_then(|p| p.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let q = params
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("q"))
                        .and_then(|q| q.as_str())
                        .unwrap_or("");

                    if tool_name == "hang" || q == "hang" {
                        loop {
                            std::thread::sleep(std::time::Duration::from_secs(3600));
                        }
                    }

                    if tool_name == "crash" || q == "crash" {
                        std::process::exit(1);
                    }

                    let resp = if tool_name == "web" {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    {
                                        "type": "text",
                                        "text": format!("searched: {q}")
                                    }
                                ]
                            }
                        })
                    } else {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("unknown tool: {tool_name}")
                            }
                        })
                    };

                    if writeln!(out, "{}", resp).is_err() || out.flush().is_err() {
                        break;
                    }
                }
            }
            _ => {
                // If id is present, return method not found error
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": "method not found"
                        }
                    });
                    if writeln!(out, "{}", resp).is_err() || out.flush().is_err() {
                        break;
                    }
                }
            }
        }
    }
}
