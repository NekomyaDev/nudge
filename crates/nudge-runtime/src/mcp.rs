//! MCP (Model Context Protocol) stdio client
//!
//! Provides a synchronous client for spawning and communicating with MCP servers
//! over standard input/output using JSON-RPC 2.0.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::hash_map::Entry;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, Command, Stdio};

/// Default MCP read timeout in milliseconds (a hanging MCP server must not block
/// a request forever). Overridable via `NUDGE_MCP_TIMEOUT_MS`.
#[cfg(not(target_arch = "wasm32"))]
fn mcp_read_timeout_ms() -> u64 {
    std::env::var("NUDGE_MCP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60_000)
}

/// Wait until `fd` is readable, has hung up, or `timeout_ms` elapses.
/// Used to make pipe reads interruptible so deadline checks and task cancellation
/// are reachable while the server is idle.
#[cfg(unix)]
fn wait_readable(fd: std::os::unix::io::RawFd, timeout_ms: i32) -> bool {
    let mut pollfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let n = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    n > 0 && (pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0
}

#[cfg(not(target_arch = "wasm32"))]
static ACTIVE_MCP_CHILDREN: std::sync::Mutex<Option<std::collections::HashSet<u32>>> =
    std::sync::Mutex::new(None);

#[cfg(not(target_arch = "wasm32"))]
fn register_mcp_child(pid: u32) {
    let mut guard = ACTIVE_MCP_CHILDREN
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(std::collections::HashSet::new)
        .insert(pid);
}

#[cfg(not(target_arch = "wasm32"))]
fn unregister_mcp_child(pid: u32) {
    let mut guard = ACTIVE_MCP_CHILDREN
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(set) = guard.as_mut() {
        set.remove(&pid);
    }
}

/// Cleanly terminate and reap all active MCP server child processes across all sessions.
#[cfg(not(target_arch = "wasm32"))]
pub fn reap_all_mcp_sessions() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut guard = ACTIVE_MCP_CHILDREN
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(mut set) = guard.take() {
            for pid in set.drain() {
                #[cfg(unix)]
                unsafe {
                    let ret = libc::kill(pid as libc::pid_t, 0);
                    if ret == 0 {
                        libc::kill(pid as libc::pid_t, libc::SIGKILL);
                        let mut status = 0;
                        libc::waitpid(pid as libc::pid_t, &mut status, 0);
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn reap_all_mcp_sessions() {}

#[cfg(not(target_arch = "wasm32"))]
fn is_fatal_mcp_error(err: &str) -> bool {
    err.contains("E-native-0203")
        || err.contains("closed the pipe")
        || err.contains("failed to write")
        || err.contains("failed to flush")
        || err.contains("failed to read")
        || err.contains("TaskCancelled")
        || err.contains("broken pipe")
        || err.contains("session is closed")
}

/// Configuration for an individual MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// Command to spawn the server (e.g. "python3 /path/mock.py" or a binary path).
    pub command: Option<String>,
    /// Optional list of tool names declared for this server.
    pub tools: Option<Vec<String>>,
    /// Optional default read timeout in milliseconds for this server.
    pub timeout_ms: Option<u64>,
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

            let timeout_ms = cfg_obj
                .get("timeout_ms")
                .or_else(|| cfg_obj.get("timeout"))
                .and_then(|v| {
                    if let Some(n) = v.as_u64() {
                        Some(n)
                    } else if let Some(n) = v.as_i64() {
                        if n > 0 {
                            Some(n as u64)
                        } else {
                            None
                        }
                    } else {
                        v.as_str().and_then(|s| s.parse::<u64>().ok())
                    }
                });

            servers.insert(
                name.clone(),
                ServerConfig {
                    command,
                    tools,
                    timeout_ms,
                },
            );
        }

        Ok(Self { servers })
    }

    /// Retrieve configuration for a named server.
    pub fn get(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }
}

/// An active stdio session with a spawned MCP server process.
#[cfg(not(target_arch = "wasm32"))]
pub struct McpSession {
    server_name: String,
    child: Child,
    writer: BufWriter<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
    closed: bool,
    poisoned: bool,
    default_timeout_ms: Option<u64>,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for McpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpSession")
            .field("server_name", &self.server_name)
            .field("next_id", &self.next_id)
            .field("closed", &self.closed)
            .field("poisoned", &self.poisoned)
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

        let pid = child.id();
        register_mcp_child(pid);

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
            poisoned: false,
            default_timeout_ms: config.timeout_ms,
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
        if let Err(e) = session.send_request("initialize", init_params) {
            let _ = session.close();
            return Err(e);
        }

        // (2) send `notifications/initialized` notification (no id, server sends no response)
        if let Err(e) =
            session.send_notification("notifications/initialized", serde_json::json!({}))
        {
            let _ = session.close();
            return Err(e);
        }

        Ok(session)
    }

    /// Process ID of the spawned server child process.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether this session has been closed or terminated.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Whether this session is active and healthy (not closed and not poisoned).
    pub fn is_healthy(&self) -> bool {
        !self.closed && !self.poisoned
    }

    /// Name of the server this session is connected to.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Send a JSON-RPC notification (no id, expects no response).
    pub fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        if self.closed || self.poisoned {
            return Err(format!(
                "error[E-native-0201]: MCP server '{}' session is closed",
                self.server_name
            ));
        }

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let payload = serde_json::to_string(&msg)
            .map_err(|e| format!("failed to serialize JSON-RPC notification: {e}"))?;

        if let Err(e) = self.writer.write_all(payload.as_bytes()) {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to write notification to MCP server '{}': {e}",
                self.server_name
            ));
        }
        if let Err(e) = self.writer.write_all(b"\n") {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to write newline to MCP server '{}': {e}",
                self.server_name
            ));
        }
        if let Err(e) = self.writer.flush() {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to flush notification to MCP server '{}': {e}",
                self.server_name
            ));
        }
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response matching the request id.
    pub fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.send_request_with_timeout(method, params, None)
    }

    /// Send a JSON-RPC request with an optional per-RPC timeout in milliseconds.
    pub fn send_request_with_timeout(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        let effective_ms = timeout_ms
            .or(self.default_timeout_ms)
            .unwrap_or_else(mcp_read_timeout_ms);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(effective_ms);
        self.send_request_with_deadline(method, params, deadline)
    }

    /// Send a JSON-RPC request with an absolute deadline.
    pub fn send_request_with_deadline(
        &mut self,
        method: &str,
        params: serde_json::Value,
        deadline: std::time::Instant,
    ) -> Result<serde_json::Value, String> {
        if self.closed || self.poisoned {
            return Err(format!(
                "error[E-native-0201]: MCP server '{}' session is closed",
                self.server_name
            ));
        }

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

        if let Err(e) = self.writer.write_all(payload.as_bytes()) {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to write request to MCP server '{}': {e}",
                self.server_name
            ));
        }
        if let Err(e) = self.writer.write_all(b"\n") {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to write newline to MCP server '{}': {e}",
                self.server_name
            ));
        }
        if let Err(e) = self.writer.flush() {
            self.poisoned = true;
            let _ = self.close();
            return Err(format!(
                "failed to flush request to MCP server '{}': {e}",
                self.server_name
            ));
        }

        if crate::task::is_current_cancelled() {
            self.poisoned = true;
            let _ = self.close();
            return Err("error[E-task-0001]: TaskCancelled".into());
        }

        // Read responses until matching req_id is found.
        loop {
            if crate::task::is_current_cancelled() {
                self.poisoned = true;
                let _ = self.close();
                return Err("error[E-task-0001]: TaskCancelled".into());
            }

            let now = std::time::Instant::now();
            if now >= deadline {
                self.poisoned = true;
                let _ = self.close();
                return Err(format!(
                    "error[E-native-0203]: MCP read timeout: server '{}' did not respond to '{method}' in time",
                    self.server_name
                ));
            }

            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                let fd = self.reader.get_ref().as_raw_fd();
                let remaining_ms = deadline.saturating_duration_since(now).as_millis();
                let slice = (remaining_ms.min(100) as i32).max(1);
                if !wait_readable(fd, slice) {
                    continue; // re-check cancellation + deadline
                }
            }

            let mut line = String::new();
            let max_bytes: u64 = 16 * 1024 * 1024;
            let read_res = (&mut self.reader).take(max_bytes).read_line(&mut line);
            let bytes_read = match read_res {
                Ok(b) => b,
                Err(e) => {
                    self.poisoned = true;
                    let _ = self.close();
                    return Err(format!(
                        "failed to read response from MCP server '{}': {e}",
                        self.server_name
                    ));
                }
            };

            if bytes_read == 0 {
                self.poisoned = true;
                let _ = self.close();
                return Err(format!(
                    "MCP server '{}' closed the pipe during '{method}'",
                    self.server_name
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let resp: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    self.poisoned = true;
                    let _ = self.close();
                    return Err(format!(
                        "invalid JSON-RPC response from MCP server '{}': {e}",
                        self.server_name
                    ));
                }
            };

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
        self.list_tools_with_timeout(None)
    }

    /// List tools supported by the server (`tools/list`) with a specific timeout.
    pub fn list_tools_with_timeout(
        &mut self,
        timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        self.send_request_with_timeout("tools/list", serde_json::json!({}), timeout_ms)
    }

    /// Call a tool (`tools/call`) and return the concatenated text from all text content items.
    pub fn call_tool(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        self.call_tool_with_timeout(tool, arguments, None)
    }

    /// Call a tool (`tools/call`) with a specific per-RPC timeout.
    pub fn call_tool_with_timeout(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> Result<String, String> {
        let mut explicit_timeout = timeout_ms;
        let mut args = if arguments.is_object() {
            arguments
        } else if arguments.is_null() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "args": arguments })
        };

        if explicit_timeout.is_none() {
            if let Some(obj) = args.as_object_mut() {
                if let Some(t) = obj
                    .remove("_timeout_ms")
                    .or_else(|| obj.remove("__timeout_ms"))
                    .or_else(|| obj.remove("timeout_ms"))
                {
                    explicit_timeout = t.as_u64();
                }
            }
        }

        let effective_ms = explicit_timeout
            .or(self.default_timeout_ms)
            .unwrap_or_else(mcp_read_timeout_ms);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(effective_ms);
        self.call_tool_with_deadline(tool, args, deadline)
    }

    /// Call a tool (`tools/call`) with an absolute deadline.
    pub fn call_tool_with_deadline(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
        deadline: std::time::Instant,
    ) -> Result<String, String> {
        let result = self.send_request_with_deadline(
            "tools/call",
            serde_json::json!({
                "name": tool,
                "arguments": arguments,
            }),
            deadline,
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
            let pid = self.child.id();
            let _ = self.child.kill();
            let _ = self.child.wait();
            unregister_mcp_child(pid);
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// A stub MCP server session on wasm32.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct McpSession {
    server_name: String,
}

#[cfg(target_arch = "wasm32")]
impl McpSession {
    pub fn connect(server_name: &str, _config: &ServerConfig) -> Result<Self, String> {
        let _ = server_name;
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn pid(&self) -> u32 {
        0
    }

    pub fn is_closed(&self) -> bool {
        false
    }

    pub fn is_healthy(&self) -> bool {
        true
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn send_notification(
        &mut self,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<(), String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn send_request(
        &mut self,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn send_request_with_timeout(
        &mut self,
        _method: &str,
        _params: serde_json::Value,
        _timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn send_request_with_deadline(
        &mut self,
        _method: &str,
        _params: serde_json::Value,
        _deadline: std::time::Instant,
    ) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn list_tools(&mut self) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn list_tools_with_timeout(
        &mut self,
        _timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    pub fn call_tool(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        call_tool(&self.server_name, tool, arguments)
    }

    pub fn call_tool_with_timeout(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
        _timeout_ms: Option<u64>,
    ) -> Result<String, String> {
        call_tool(&self.server_name, tool, arguments)
    }

    pub fn call_tool_with_deadline(
        &mut self,
        tool: &str,
        arguments: serde_json::Value,
        _deadline: std::time::Instant,
    ) -> Result<String, String> {
        call_tool(&self.server_name, tool, arguments)
    }

    pub fn close(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// High-level client managing MCP server registry and sessions.
pub struct McpClient {
    registry: McpRegistry,
    pub sessions: HashMap<String, McpSession>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("registry", &self.registry)
            .field("sessions", &self.sessions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self {
            registry: McpRegistry::from_env().unwrap_or_default(),
            sessions: HashMap::new(),
        }
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

    /// Replace the registry.
    pub fn set_registry(&mut self, registry: McpRegistry) {
        self.registry = registry;
    }

    /// Check if a session for `server` is currently cached.
    pub fn has_session(&self, server: &str) -> bool {
        self.sessions.contains_key(server)
    }

    /// Borrow the session for `server` if active.
    pub fn get_session(&self, server: &str) -> Option<&McpSession> {
        self.sessions.get(server)
    }

    /// Evict and terminate a server session.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn evict(&mut self, server: &str) -> Option<McpSession> {
        if let Some(mut session) = self.sessions.remove(server) {
            let _ = session.close();
            Some(session)
        } else {
            None
        }
    }

    /// Evict a server session on wasm32.
    #[cfg(target_arch = "wasm32")]
    pub fn evict(&mut self, server: &str) -> Option<McpSession> {
        self.sessions.remove(server)
    }

    /// Evict an existing session and start a fresh connection.
    pub fn restart(&mut self, server: &str) -> Result<&mut McpSession, String> {
        self.evict(server);
        self.connect(server)
    }

    /// Connect to a named server or return the cached session.
    /// If an existing cached session is closed or unhealthy, it is evicted and reconnected.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn connect(&mut self, server: &str) -> Result<&mut McpSession, String> {
        if let Some(session) = self.sessions.get(server) {
            if !session.is_healthy() {
                self.evict(server);
            }
        }

        if self.registry.get(server).is_none() {
            if let Ok(fresh_reg) = McpRegistry::from_env() {
                if fresh_reg.get(server).is_some() {
                    self.registry = fresh_reg;
                }
            }
        }

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

    /// Connect to a named server (stubbed on wasm32).
    #[cfg(target_arch = "wasm32")]
    pub fn connect(&mut self, _server: &str) -> Result<&mut McpSession, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    /// Call a tool on a named server.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn call_tool(
        &mut self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        self.call_tool_with_timeout(server, tool, arguments, None)
    }

    /// Call a tool on a named server with an optional per-RPC timeout.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn call_tool_with_timeout(
        &mut self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> Result<String, String> {
        let session = self.connect(server)?;
        let result = session.call_tool_with_timeout(tool, arguments, timeout_ms);
        if let Err(ref e) = result {
            let is_unhealthy = self.sessions.get(server).is_some_and(|s| !s.is_healthy());
            if is_unhealthy || is_fatal_mcp_error(e) {
                self.evict(server);
            }
        }
        result
    }

    /// Call a tool on a named server (routes to host bridge on wasm32).
    #[cfg(target_arch = "wasm32")]
    pub fn call_tool(
        &mut self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        self.call_tool_with_timeout(server, tool, arguments, None)
    }

    /// Call a tool on a named server with timeout on wasm32.
    #[cfg(target_arch = "wasm32")]
    pub fn call_tool_with_timeout(
        &mut self,
        server: &str,
        tool: &str,
        arguments: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> Result<String, String> {
        call_tool_with_timeout(server, tool, arguments, timeout_ms)
    }

    /// List tools on a named server.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_tools(&mut self, server: &str) -> Result<serde_json::Value, String> {
        self.list_tools_with_timeout(server, None)
    }

    /// List tools on a named server with an optional per-RPC timeout.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_tools_with_timeout(
        &mut self,
        server: &str,
        timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        let session = self.connect(server)?;
        let result = session.list_tools_with_timeout(timeout_ms);
        if let Err(ref e) = result {
            let is_unhealthy = self.sessions.get(server).is_some_and(|s| !s.is_healthy());
            if is_unhealthy || is_fatal_mcp_error(e) {
                self.evict(server);
            }
        }
        result
    }

    /// List tools on a named server (stubbed on wasm32).
    #[cfg(target_arch = "wasm32")]
    pub fn list_tools(&mut self, _server: &str) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    /// List tools on a named server with timeout (stubbed on wasm32).
    #[cfg(target_arch = "wasm32")]
    pub fn list_tools_with_timeout(
        &mut self,
        _server: &str,
        _timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
    }

    /// Cleanly close all active server sessions.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn close(&mut self) -> Result<(), String> {
        for (_, mut session) in self.sessions.drain() {
            let _ = session.close();
        }
        Ok(())
    }

    /// Cleanly close all active server sessions.
    #[cfg(target_arch = "wasm32")]
    pub fn close(&mut self) -> Result<(), String> {
        self.sessions.clear();
        Ok(())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Load the MCP registry from the `NUDGE_MCP_SERVERS` environment variable.
pub fn load_registry() -> Result<McpRegistry, String> {
    McpRegistry::from_env()
}

/// Connect to a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_server(server: &str) -> Result<McpSession, String> {
    let registry = McpRegistry::from_env()?;
    let config = registry
        .get(server)
        .ok_or_else(|| format!("unknown MCP server '{server}'"))?;
    McpSession::connect(server, config)
}

/// Connect to a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
#[cfg(target_arch = "wasm32")]
pub fn connect_server(_server: &str) -> Result<McpSession, String> {
    Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
}

/// Call a tool on a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
#[cfg(not(target_arch = "wasm32"))]
pub fn call_tool(server: &str, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
    call_tool_with_timeout(server, tool, arguments, None)
}

/// Call a tool on a named MCP server with an optional timeout in milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub fn call_tool_with_timeout(
    server: &str,
    tool: &str,
    arguments: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<String, String> {
    let mut session = connect_server(server)?;
    session.call_tool_with_timeout(tool, arguments, timeout_ms)
}

/// Call a tool on a named MCP server.
///
/// On wasm32 targets, this routes to the host bridge via `host_tool::host_tool_call`.
#[cfg(target_arch = "wasm32")]
pub fn call_tool(server: &str, tool: &str, arguments: serde_json::Value) -> Result<String, String> {
    call_tool_with_timeout(server, tool, arguments, None)
}

/// Call a tool on a named MCP server with an optional timeout.
#[cfg(target_arch = "wasm32")]
pub fn call_tool_with_timeout(
    server: &str,
    tool: &str,
    arguments: serde_json::Value,
    _timeout_ms: Option<u64>,
) -> Result<String, String> {
    let args = if arguments.is_object() {
        arguments
    } else if arguments.is_null() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "args": arguments })
    };
    let args_json = serde_json::to_string(&args)
        .map_err(|e| format!("failed to serialize MCP arguments: {e}"))?;
    crate::host_tool::host_tool_call(server, tool, &args_json)
}

/// List tools available on a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
#[cfg(not(target_arch = "wasm32"))]
pub fn list_tools(server: &str) -> Result<serde_json::Value, String> {
    list_tools_with_timeout(server, None)
}

/// List tools available on a named MCP server with an optional timeout.
#[cfg(not(target_arch = "wasm32"))]
pub fn list_tools_with_timeout(
    server: &str,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, String> {
    let mut session = connect_server(server)?;
    session.list_tools_with_timeout(timeout_ms)
}

/// List tools available on a named MCP server using configuration from `NUDGE_MCP_SERVERS`.
#[cfg(target_arch = "wasm32")]
pub fn list_tools(_server: &str) -> Result<serde_json::Value, String> {
    Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
}

/// List tools available on a named MCP server with an optional timeout.
#[cfg(target_arch = "wasm32")]
pub fn list_tools_with_timeout(
    _server: &str,
    _timeout_ms: Option<u64>,
) -> Result<serde_json::Value, String> {
    Err("MCP process not available on wasm32; use NUDGE_REPLAY".to_string())
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
        assert_eq!(cfg.timeout_ms, None);
    }

    #[test]
    fn test_registry_timeout_parsing() {
        let json = r#"{"search": {"command": "mock_mcp", "timeout_ms": 2500}}"#;
        let reg = McpRegistry::from_json_str(json).unwrap();
        let cfg = reg.get("search").expect("server search should exist");
        assert_eq!(cfg.timeout_ms, Some(2500));

        let json2 = r#"{"search": {"command": "mock_mcp", "timeout": 1200}}"#;
        let reg2 = McpRegistry::from_json_str(json2).unwrap();
        let cfg2 = reg2.get("search").expect("server search should exist");
        assert_eq!(cfg2.timeout_ms, Some(1200));
    }

    #[test]
    fn test_registry_empty_env() {
        let reg = McpRegistry::empty();
        assert!(reg.get("search").is_none());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(not(target_arch = "wasm32"))]
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
