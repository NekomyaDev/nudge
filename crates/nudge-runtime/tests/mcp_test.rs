use nudge_runtime::mcp::{
    call_tool, call_tool_with_timeout, connect_server, list_tools, reap_all_mcp_sessions,
    McpClient, McpRegistry, ServerConfig,
};
use std::sync::Mutex;

// Mutex to serialize tests in this module so environment mutations and
// global process reaping do not interfere across parallel test runner threads.
static MCP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_mcp_tools_call_round_trip() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
            timeout_ms: None,
        },
    );

    let mut client = McpClient::with_registry(reg);
    let result = client
        .call_tool("search", "web", serde_json::json!({ "q": "nudge lang" }))
        .expect("tools/call should succeed");

    assert_eq!(result, "searched: nudge lang");
}

#[test]
fn test_mcp_unknown_server_exact_error() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mut client = McpClient::with_registry(McpRegistry::empty());
    let err = client
        .call_tool("nope", "web", serde_json::json!({ "q": "test" }))
        .unwrap_err();

    assert!(
        err.contains("unknown MCP server 'nope'"),
        "error must contain exact `unknown MCP server 'nope'`, got: {err}"
    );
}

#[test]
fn test_mcp_tools_list() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
            timeout_ms: None,
        },
    );

    let mut client = McpClient::with_registry(reg);
    let tools_val = client
        .list_tools("search")
        .expect("tools/list should succeed");

    let tools_arr = tools_val
        .get("tools")
        .and_then(|t| t.as_array())
        .expect("result should have 'tools' array");

    let has_web = tools_arr
        .iter()
        .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("web"));
    assert!(has_web, "mock server should expose 'web' tool");
}

#[test]
fn test_mcp_env_var_integration() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let registry_json = format!(r#"{{"search": {{"command": "{mock_bin}"}}}}"#);

    unsafe {
        std::env::set_var("NUDGE_MCP_SERVERS", &registry_json);
    }

    // Test free functions with NUDGE_MCP_SERVERS
    let res = call_tool("search", "web", serde_json::json!({ "q": "hello world" }))
        .expect("call_tool via env should succeed");
    assert_eq!(res, "searched: hello world");

    let tools = list_tools("search").expect("list_tools via env should succeed");
    let tools_arr = tools.get("tools").and_then(|t| t.as_array()).unwrap();
    assert!(tools_arr
        .iter()
        .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("web")));

    // Test unknown server with env
    let err = call_tool("unknown_svc", "web", serde_json::json!({})).unwrap_err();
    assert!(
        err.contains("unknown MCP server 'unknown_svc'"),
        "error must contain exact `unknown MCP server 'unknown_svc'`, got: {err}"
    );

    let err2 = connect_server("unknown_svc").unwrap_err();
    assert!(
        err2.contains("unknown MCP server 'unknown_svc'"),
        "error must contain exact `unknown MCP server 'unknown_svc'`, got: {err2}"
    );

    unsafe {
        std::env::remove_var("NUDGE_MCP_SERVERS");
    }
}

#[test]
fn test_mcp_session_drop_reaps_cleanly() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: None,
            timeout_ms: None,
        },
    );

    let mut client = McpClient::with_registry(reg);
    // Connect to spawn process
    let session = client.connect("search").expect("connect should succeed");
    let pid = session
        .call_tool("web", serde_json::json!({ "q": "ping" }))
        .unwrap();
    assert_eq!(pid, "searched: ping");

    // Close client and ensure it reaps
    client.close().expect("close should succeed");
}

#[test]
fn test_mcp_timeout_eviction_and_clean_restart() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "flaky".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
            timeout_ms: Some(150),
        },
    );

    let mut client = McpClient::with_registry(reg);

    // 1. Initial successful call: verifies session is established and healthy
    let res1 = client
        .call_tool("flaky", "web", serde_json::json!({ "q": "first" }))
        .expect("initial call should succeed");
    assert_eq!(res1, "searched: first");

    let initial_pid = client
        .get_session("flaky")
        .expect("session should be cached")
        .pid();
    assert!(initial_pid > 0);

    // 2. Call that hangs: must time out
    let err = client
        .call_tool("flaky", "web", serde_json::json!({ "q": "hang" }))
        .unwrap_err();

    assert!(
        err.contains("error[E-native-0203]: MCP read timeout"),
        "expected timeout error E-native-0203, got: {err}"
    );

    // Verify session was evicted from client
    assert!(
        !client.has_session("flaky"),
        "timed-out session must be evicted from McpClient"
    );

    // Verify the hung child process was terminated and reaped from OS
    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(initial_pid as libc::pid_t, 0) };
        assert_eq!(
            ret, -1,
            "hung child process {initial_pid} should be terminated and reaped"
        );
    }

    // 3. Subsequent call on the same client instance: must cleanly restart a fresh session
    let res2 = client
        .call_tool("flaky", "web", serde_json::json!({ "q": "second" }))
        .expect("subsequent call should succeed with a fresh restarted session");
    assert_eq!(res2, "searched: second");

    let new_pid = client
        .get_session("flaky")
        .expect("restarted session should be cached")
        .pid();
    assert_ne!(
        new_pid, initial_pid,
        "new session must have a fresh child PID"
    );

    client.close().expect("close should succeed");
}

#[test]
fn test_mcp_crash_eviction_and_clean_restart() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "crashy".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
            timeout_ms: None,
        },
    );

    let mut client = McpClient::with_registry(reg);

    // 1. Initial call
    let res1 = client
        .call_tool("crashy", "web", serde_json::json!({ "q": "ping" }))
        .expect("initial call should succeed");
    assert_eq!(res1, "searched: ping");

    // 2. Trigger crash in server
    let err = client
        .call_tool("crashy", "web", serde_json::json!({ "q": "crash" }))
        .unwrap_err();
    assert!(
        err.contains("closed the pipe") || err.contains("failed to write"),
        "expected closed pipe or write failure, got: {err}"
    );

    // Verify session was evicted
    assert!(!client.has_session("crashy"));

    // 3. Subsequent call should cleanly restart
    let res2 = client
        .call_tool("crashy", "web", serde_json::json!({ "q": "recovered" }))
        .expect("should restart fresh session after crash");
    assert_eq!(res2, "searched: recovered");

    client.close().unwrap();
}

#[test]
fn test_mcp_per_rpc_timeout_override() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
            timeout_ms: Some(60_000), // Default server timeout is 60s
        },
    );

    let mut client = McpClient::with_registry(reg);

    // Call with per-RPC timeout override of 100ms on a hung request
    let start = std::time::Instant::now();
    let err = client
        .call_tool_with_timeout(
            "search",
            "web",
            serde_json::json!({ "q": "hang" }),
            Some(100),
        )
        .unwrap_err();
    let elapsed = start.elapsed();

    assert!(err.contains("error[E-native-0203]: MCP read timeout"));
    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "per-RPC timeout must abort in ~100ms, not 60s (elapsed: {elapsed:?})"
    );
}

#[test]
fn test_call_tool_with_timeout_free_fn() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let registry_json = format!(r#"{{"search": {{"command": "{mock_bin}"}}}}"#);

    unsafe {
        std::env::set_var("NUDGE_MCP_SERVERS", &registry_json);
    }

    let start = std::time::Instant::now();
    let err = call_tool_with_timeout(
        "search",
        "web",
        serde_json::json!({ "q": "hang" }),
        Some(100),
    )
    .unwrap_err();
    let elapsed = start.elapsed();

    assert!(err.contains("error[E-native-0203]: MCP read timeout"));
    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "free fn timeout must abort in ~100ms (elapsed: {elapsed:?})"
    );

    unsafe {
        std::env::remove_var("NUDGE_MCP_SERVERS");
    }
}

#[test]
fn test_mcp_reap_all_sessions() {
    let _guard = MCP_TEST_LOCK.lock().unwrap();
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: None,
            timeout_ms: None,
        },
    );

    let mut client = McpClient::with_registry(reg);
    let session = client.connect("search").unwrap();
    let pid = session.pid();
    assert!(pid > 0);

    // Verify process is running
    #[cfg(unix)]
    {
        let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
        assert_eq!(ret, 0, "child process should be running");
    }

    // Call reap_all_mcp_sessions()
    reap_all_mcp_sessions();

    // Verify process was killed and reaped
    #[cfg(unix)]
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
        assert_eq!(
            ret, -1,
            "process {pid} should be reaped after reap_all_mcp_sessions"
        );
    }
}
