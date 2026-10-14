//! Host-tool bridge for WebAssembly (wasm32-unknown-unknown).
//!
//! Provides a mechanism for WASM agents running in a browser or edge JS host
//! to execute MCP tool calls by delegating them to the host via an imported
//! C function `nudge_host_tool_call`.

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn nudge_host_tool_call(
        server_ptr: *const u8,
        server_len: usize,
        tool_ptr: *const u8,
        tool_len: usize,
        args_ptr: *const u8,
        args_len: usize,
        out_ptr: *mut u8,
        out_cap: usize,
    ) -> isize;
}

/// Default buffer capacity allocated for tool call results (256 KiB).
#[cfg(target_arch = "wasm32")]
const DEFAULT_BUFFER_CAPACITY: usize = 256 * 1024;

/// Maximum buffer capacity if host reports buffer is too small (4 MiB).
#[cfg(target_arch = "wasm32")]
const MAX_BUFFER_CAPACITY: usize = 4 * 1024 * 1024;

/// Format a negative host error code into an informative error message.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn format_host_error(server: &str, tool: &str, code: isize) -> String {
    match code {
        -1 => format!(
            "host tool call '{server}::{tool}' failed: server not found or connection failed (code -1)"
        ),
        -2 => format!("host tool call '{server}::{tool}' failed: tool not found (code -2)"),
        -3 => format!("host tool call '{server}::{tool}' failed: tool execution failed (code -3)"),
        -4 => format!("host tool call '{server}::{tool}' failed: output buffer too small (code -4)"),
        -5 => format!(
            "host tool call '{server}::{tool}' failed: invalid arguments or JSON error (code -5)"
        ),
        code => format!("host tool call '{server}::{tool}' failed with host error code {code}"),
    }
}

/// Call a tool via the host bridge on wasm32 targets.
///
/// Marshals `server`, `tool`, and `args_json` strings across the WebAssembly FFI boundary
/// to `nudge_host_tool_call`, and reads back the resulting JSON text into a `String`.
#[cfg(target_arch = "wasm32")]
pub fn host_tool_call(server: &str, tool: &str, args_json: &str) -> Result<String, String> {
    let mut cap = DEFAULT_BUFFER_CAPACITY;
    let mut out_buf = vec![0u8; cap];

    // SAFETY: We pass valid pointers and lengths for server, tool, and args_json,
    // and a valid mutable buffer with capacity `cap`.
    let mut ret = unsafe {
        nudge_host_tool_call(
            server.as_ptr(),
            server.len(),
            tool.as_ptr(),
            tool.len(),
            args_json.as_ptr(),
            args_json.len(),
            out_buf.as_mut_ptr(),
            cap,
        )
    };

    // If host reported that the output buffer was too small (code -4), retry once with MAX_BUFFER_CAPACITY.
    if ret == -4 && cap < MAX_BUFFER_CAPACITY {
        cap = MAX_BUFFER_CAPACITY;
        out_buf.resize(cap, 0);
        ret = unsafe {
            nudge_host_tool_call(
                server.as_ptr(),
                server.len(),
                tool.as_ptr(),
                tool.len(),
                args_json.as_ptr(),
                args_json.len(),
                out_buf.as_mut_ptr(),
                cap,
            )
        };
    }

    if ret < 0 {
        return Err(format_host_error(server, tool, ret));
    }

    let len = ret as usize;
    if len > out_buf.len() {
        return Err(format!(
            "host tool call '{server}::{tool}' reported length {len} exceeding buffer capacity {}",
            out_buf.len()
        ));
    }

    String::from_utf8(out_buf[..len].to_vec())
        .map_err(|e| format!("host tool call '{server}::{tool}' returned invalid UTF-8: {e}"))
}

/// Stub implementation for non-wasm32 targets.
#[cfg(not(target_arch = "wasm32"))]
pub fn host_tool_call(_server: &str, _tool: &str, _args_json: &str) -> Result<String, String> {
    Err("host_tool_call is only available on wasm32".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_host_tool_call_non_wasm_stub() {
        let err = host_tool_call("test_server", "test_tool", "{}").unwrap_err();
        assert_eq!(err, "host_tool_call is only available on wasm32");
    }

    #[test]
    fn test_format_host_error_messages() {
        let err_server_not_found = format_host_error("srv", "t", -1);
        assert!(err_server_not_found.contains("server not found or connection failed"));

        let err_tool_not_found = format_host_error("srv", "t", -2);
        assert!(err_tool_not_found.contains("tool not found"));

        let err_exec = format_host_error("srv", "t", -3);
        assert!(err_exec.contains("tool execution failed"));

        let err_buf = format_host_error("srv", "t", -4);
        assert!(err_buf.contains("output buffer too small"));

        let err_json = format_host_error("srv", "t", -5);
        assert!(err_json.contains("invalid arguments or JSON error"));

        let err_unknown = format_host_error("srv", "t", -99);
        assert!(err_unknown.contains("host error code -99"));
    }
}
