use nudge_runtime::mcp::{
    call_tool, connect_server, list_tools, McpClient, McpRegistry, ServerConfig,
};
use std::sync::Mutex;

// Mutex to serialize tests that mutate the NUDGE_MCP_SERVERS environment variable
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_mcp_tools_call_round_trip() {
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
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
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: Some(vec!["web".to_string()]),
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
    let _guard = ENV_LOCK.lock().unwrap();
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
    let mock_bin = env!("CARGO_BIN_EXE_mock_mcp");
    let mut reg = McpRegistry::empty();
    reg.servers.insert(
        "search".to_string(),
        ServerConfig {
            command: Some(mock_bin.to_string()),
            tools: None,
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
