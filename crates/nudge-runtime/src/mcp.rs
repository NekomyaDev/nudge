//! MCP (Model Context Protocol) stdio client
//!
//! Provides a synchronous client for spawning and communicating with MCP servers
//! over standard input/output using JSON-RPC 2.0.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, Command, Stdio};

/// Configuration for an individual MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// Command to spawn the server (e.g. "python3 /path/mock.py" or a binary path).
    pub command: Option<String>,
    /// Optional list of tool names declared for this server.
    pub tools: Option<Vec<String>>,
}

/// Registry of configured MCP servers loaded from environment or JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpRegistry {
    /// Map of server name to server configuration.
    pub servers: HashMap<String, ServerConfig>,
}

impl McpRegistry {
    /// Create an empty server registry.
    pub fn empty() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Load registry from the `NUDGE_MCP_SERVERS` environment variable.
    ///
    /// Returns an empty registry if the environment variable is unset or empty.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("NUDGE_MCP_SERVERS") {
            Ok(val) => {
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    Ok(Self::empty())
                } else {
                    Self::from_json_str(trimmed)
                }
            }
            Err(std::env::VarError::NotPresent) => Ok(Self::empty()),
            Err(e) => Err(format!("failed to read NUDGE_MCP_SERVERS: {e}")),
        }
    }

    /// Parse registry from a JSON string mapping server names to config objects.
    pub fn from_json_str(s: &str) -> Result<Self, String> {
        let root: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("invalid NUDGE_MCP_SERVERS JSON: {e}"))?;
        let obj = root.as_object().ok_or_else(|| {
            "NUDGE_MCP_SERVERS must be a JSON object mapping server names to config".to_string()
        })?;

        let mut servers = HashMap::new();
        for (name, val) in obj {
            let cfg_obj = val
                .as_object()
                .ok_or_else(|| format!("server config for '{name}' must be a JSON object"))?;

            let command = cfg_obj.get("command").and_then(|c| {
                if let Some(cmd_str) = c.as_str() {
                    Some(cmd_str.to_string())
                } else if let Some(arr) = c.as_array() {
                    let parts: Vec<&str> = arr.iter().filter_map(|x| x.as_str()).collect();
                    Some(parts.join(" "))
                } else {
                    None
                }
            });

            let tools = cfg_obj.get("tools").and_then(|t| {
                t.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
            });

            servers.insert(name.clone(), ServerConfig { command, tools });
        }

        Ok(Self { servers })
    }

    /// Retrieve configuration for a named server.
    pub fn get(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }
}

/// An active stdio session with a spawned MCP server process.
pub struct McpSession {
    server_name: String,
    child: Child,
    writer: BufWriter<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
    closed: bool,
}

impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("server_name", &self.server_name)
            .field("next_id", &self.next_id)
            .field("closed", &self.closed)
            .finish()
    }
}

impl McpSession {
    /// Spawn and initialize a new MCP server session.
    pub fn connect(server_name: &str, config: &ServerConfig) -> Result<Self, String> {
        let command = config.command.as_deref().ok_or_else(|| {
            format!("MCP server '{server_name}' has no 'command' in NUDGE_MCP_SERVERS")
        })?;

        // Simple split on whitespace for program and arguments
        let mut parts = command.split_whitespace();
        let prog = parts.next().ok_or_else(|| {
            format!("MCP server '{server_name}' has empty command in NUDGE_MCP_SERVERS")
        })?;
        let args: Vec<&str> = parts.collect();

        let mut child = Command::new(prog)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to spawn MCP server '{server_name}' ({prog}): {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("failed to capture stdin for MCP server '{server_name}'"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("failed to capture stdout for MCP server '{server_name}'"))?;

        let mut session = Self {
            server_name: server_name.to_string(),
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
            next_id: 1,
            closed: false,
        };

        // Handshake:
        // (1) send `initialize` request and read response
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "nudge",
                "version": "1.1"
            }
        });
        session.send_request("initialize", init_params)?;

        // (2) send `notifications/initialized` notification (no id, server sends no response)
        session.send_notification("notifications/initialized", serde_json::json!({}))?;

        Ok(session)
    }

    /// Send a JSON-RPC notification (no id, expects no response).
    pub fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let payload = serde_json::to_string(&msg)
            .map_err(|e| format!("failed to serialize JSON-RPC notification: {e}"))?;
        self.writer.write_all(payload.as_bytes()).map_err(|e| {
            format!(
                "failed to write notification to MCP server '{}': {e}",
                self.server_name
            )
        })?;
        self.writer.write_all(b"\n").map_err(|e| {
            format!(
                "failed to write newline to MCP server '{}': {e}",
                self.server_name
            )
        })?;
        self.writer.flush().map_err(|e| {
            format!(
                "failed to flush notification to MCP server '{}': {e}",
                self.server_name
            )
        })?;
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response matching the request id.
    pub fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let req_id = self.next_id;
        self.next_id += 1;

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params,
        });

        let payload = serde_json::to_string(&msg)
            .map_err(|e| format!("failed to serialize JSON-RPC request: {e}"))?;
        self.writer.write_all(payload.as_bytes()).map_err(|e| {
            format!(
                "failed to write request to MCP server '{}': {e}",
                self.server_name
            )
        })?;
        self.writer.write_all(b"\n").map_err(|e| {
            format!(
                "failed to write newline to MCP server '{}': {e}",
                self.server_name
            )
        })?;
        self.writer.flush().map_err(|e| {
            format!(
                "failed to flush request to MCP server '{}': {e}",
                self.server_name
            )
        })?;

        // Read responses until matching req_id is found
        loop {
            let mut line = String::new();
            let bytes_read = self.reader.read_line(&mut line).map_err(|e| {
                format!(
                    "failed to read response from MCP server '{}': {e}",
                    self.server_name
                )
            })?;
            if bytes_read == 0 {
                return Err(format!(
                    "MCP server '{}' closed the pipe during '{method}'",
                    self.server_name
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
                format!(
                    "invalid JSON-RPC response from MCP server '{}': {e}",
                    self.server_name
                )
            })?;

            let Some(id) = resp.get("id") else {
                continue;
            };

            let matches_id = match id {
                serde_json::Value::Number(n) => n.as_u64() == Some(req_id),
                _ => false,
            };

            if matches_id {
                if let Some(err) = resp.get("error") {
                    return Err(format!("MCP '{method}' on '{}': {err}", self.server_name));
                }
                return Ok(resp
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
        }
    }

    /// List tools supported by the server (`tools/list`).
    pub fn list_tools(&mut self) -> Result<serde_json::Value, String> {
        self.send_request("tools/list", serde_json::json!({}))
    }

    /// Call a tool (`tools/call`) and return the concatenated text from all text content items.
    pub fn call_tool(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let args = if arguments.is_object() {
            arguments
        } else if arguments.is_null() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "args": arguments })
        };

        let result = self.send_request(
            "tools/call",
            serde_json::json!({
                "name": tool,
                "arguments": args,
            }),
        )?;

        if result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let content = result.get("content").unwrap_or(&serde_json::Value::Null);
            return Err(format!(
                "MCP tool '{tool}' on '{}' reported an error: {content}",
                self.server_name
            ));
        }

        let mut output = String::new();
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            for entry in content {
                if entry.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = entry.get("text").and_then(|t| t.as_str()) {
                        output.push_str(text);
                    }
                }
            }
        } else if let Some(content_str) = result.get("content").and_then(|c| c.as_str()) {
            output.push_str(content_str);
        }

        Ok(output)
    }

    /// Cleanly close the session by killing and waiting for the child process.
    pub fn close(&mut self) -> Result<(), String> {
        if !self.closed {
            self.closed = true;
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        Ok(())
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// High-level client managing MCP server registry and sessions.
pub struct McpClient {
    registry: McpRegistry,
    sessions: HashMap<String, McpSession>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("registry", &self.registry)
            .field("sessions", &self.sessions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl McpClient {
    /// Create a new MCP client reading the registry from `NUDGE_MCP_SERVERS`.
    pub fn new() -> Result<Self, String> {
        let registry = McpRegistry::from_env()?;
        Ok(Self {
            registry,
            sessions: HashMap::new(),
        })
    }

    /// Create a new MCP client with an explicit registry.
    pub fn with_registry(registry: McpRegistry) -> Self {
        Self {
            registry,
            sessions: HashMap::new(),
        }
    }

    /// Access the registry.
    pub fn registry(&self) -> &McpRegistry {
        &self.registry
    }

    /// Connect to a named server or return the cached session.
    pub fn connect(&mut self, server: &str) -> Result<&mut McpSession, String> {
        match self.sessions.entry(server.to_string()) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let config = self
                    .registry
                    .get(server)
                    .ok_or_else(|| format!("unknown MCP server '{server}'"))?;
                let session = McpSession::connect(server, config)?;
                Ok(entry.insert(session))
            }
        }
    }

    /// Call a tool on a named server.
    pub fn call_tool(
        &mut self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let session = self.connect(server)?;
        session.call_tool(tool, arguments)
    }

    /// List tools on a named server.
    pub fn list_tools(&mut self, server: &str) -> Result<serde_json::Value, String> {
        let session = self.connect(server)?;
        session.list_tools()
    }

    /// Cleanly close all active server sessions.
    pub fn close(&mut self) -> Result<(), String> {
        for session in self.sessions.values_mut() {
            let _ = session.close();
        }
        self.sessions.clear();
        Ok(())
    }
}

/// Load the MCP registry from the `NUDGE_MCP_SERVERS` environment variable.
pub fn load_registry() -> Result<McpRegistry, String> {
    McpRegistry::from_env()
}

/// Connect to a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
pub fn connect_server(server: &str) -> Result<McpSession, String> {
    let registry = McpRegistry::from_env()?;
    let config = registry
        .get(server)
        .ok_or_else(|| format!("unknown MCP server '{server}'"))?;
    McpSession::connect(server, config)
}

/// Call a tool on a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
pub fn call_tool(server: &str, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
    let mut session = connect_server(server)?;
    session.call_tool(tool, arguments)
}

/// List tools available on a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
pub fn list_tools(server: &str) -> Result<serde_json::Value, String> {
    let mut session = connect_server(server)?;
    session.list_tools()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_parsing() {
        let json = r#"{"search": {"command": "mock_mcp --verbose", "tools": ["web"]}}"#;
        let reg = McpRegistry::from_json_str(json).unwrap();
        let cfg = reg.get("search").expect("server search should exist");
        assert_eq!(cfg.command.as_deref(), Some("mock_mcp --verbose"));
        assert_eq!(cfg.tools, Some(vec!["web".to_string()]));
    }

    #[test]
    fn test_registry_empty_env() {
        let reg = McpRegistry::empty();
        assert!(reg.get("search").is_none());
    }

    #[test]
    fn test_unknown_server_error_message() {
        let mut client = McpClient::with_registry(McpRegistry::empty());
        let err = client
            .call_tool("nope", "web", serde_json::json!({}))
            .unwrap_err();
        assert!(
            err.contains("unknown MCP server 'nope'"),
            "expected error containing unknown MCP server 'nope', got: {err}"
        );
    }

    #[test]
    fn test_server_without_command_error() {
        let json = r#"{"search": {"tools": ["web"]}}"#;
        let reg = McpRegistry::from_json_str(json).unwrap();
        let mut client = McpClient::with_registry(reg);
        let err = client
            .call_tool("search", "web", serde_json::json!({}))
            .unwrap_err();
        assert!(
            err.contains("has no 'command'"),
            "expected error about missing command, got: {err}"
        );
    }
}
