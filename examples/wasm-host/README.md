# Nudge WASM Host Tool Bridge

This example demonstrates how a JavaScript or WebAssembly host implements the host-tool bridge for Nudge agents compiled to `wasm32-unknown-unknown` (browser and edge runtimes).

## Why Host-Tool Bridging is Needed

When running in sandboxed WebAssembly (such as inside a web browser or edge worker), the runtime cannot spawn child processes or establish arbitrary network connections on its own. While offline execution can replay recorded traces via `NUDGE_REPLAY`, live tool execution requires delegating tool invocations to the hosting JavaScript environment.

Nudge achieves this by importing a C function `nudge_host_tool_call` that the host environment provides upon instantiation.

## Import Signature

The host must provide the following extern "C" function in the WebAssembly import object under the `"env"` namespace:

```c
int32_t nudge_host_tool_call(
    const uint8_t *server_ptr,
    size_t server_len,
    const uint8_t *tool_ptr,
    size_t tool_len,
    const uint8_t *args_ptr,
    size_t args_len,
    uint8_t *out_ptr,
    size_t out_cap
);
```

In Rust (`crates/nudge-runtime/src/host_tool.rs`), this is declared as:

```rust
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
```

## Memory Marshalling Contract

The host and the WebAssembly module share the linear memory buffer (`WebAssembly.Memory`).

### 1. Inputs (WASM to Host)
- `server_ptr` and `server_len`: Pointer and byte length to the UTF-8 server identifier string.
- `tool_ptr` and `tool_len`: Pointer and byte length to the UTF-8 tool name string.
- `args_ptr` and `args_len`: Pointer and byte length to the UTF-8 JSON arguments string (e.g., `{"city":"Tokyo"}`).

The host reads the UTF-8 bytes directly from `instance.exports.memory.buffer`:
```javascript
const server = new TextDecoder().decode(new Uint8Array(memory.buffer, serverPtr, serverLen));
const tool = new TextDecoder().decode(new Uint8Array(memory.buffer, toolPtr, toolLen));
const argsJson = new TextDecoder().decode(new Uint8Array(memory.buffer, argsPtr, argsLen));
```

### 2. Output Buffer (Host to WASM)
- `out_ptr`: Pointer to a pre-allocated buffer in WASM linear memory where the host must write the UTF-8 JSON result string.
- `out_cap`: Maximum capacity in bytes available at `out_ptr`.
- The Rust runtime allocates an initial capacity of 256 KiB. If the result exceeds this, the host returns `-4` (Buffer Too Small), and the runtime retries once with up to 4 MiB.

The host encodes the result as UTF-8 bytes and writes to `out_ptr`:
```javascript
const resultBytes = new TextEncoder().encode(resultString);
if (resultBytes.length > outCap) {
  return -4; // Buffer too small
}
new Uint8Array(memory.buffer, outPtr, outCap).set(resultBytes);
```

### 3. Return Value (Length or Error Code)
The function returns an `isize` (signed 32-bit integer in WASM):
- **`>= 0` (Success)**: The number of bytes written into the output buffer at `out_ptr`.
- **`< 0` (Error Code)**: A negative status code indicating why the tool call failed:
  - `-1`: Server not found or connection failed
  - `-2`: Tool not found on server
  - `-3`: Tool execution failed
  - `-4`: Output buffer too small (`out_cap` exceeded)
  - `-5`: Invalid arguments or JSON parsing error
  - Other negative values: Generic host error

## Host Implementation Options

The JS host can dispatch tool calls in multiple ways:

1. **Local In-Memory Tools**: Run directly in JavaScript (e.g., interacting with browser APIs, DOM, or localStorage).
2. **Remote MCP Servers**: Forward the request to an MCP server running over HTTP SSE or WebSocket.

## Running the Example

### In Browser
Open `index.html` in a web browser using any static file server:
```bash
python3 -m http.server 8080 --directory examples/wasm-host
```
Then navigate to `http://localhost:8080`.

### In Node.js
Import `createHostToolImports` from `./host.js` when instantiating your WebAssembly module:
```javascript
import { readFile } from "node:fs/promises";
import { createHostToolImports } from "./host.js";

let memory;
const bridge = createHostToolImports(() => memory);

const wasmBytes = await readFile("./path_to_agent.wasm");
const { instance } = await WebAssembly.instantiate(wasmBytes, {
  env: bridge.env,
});
memory = instance.exports.memory;
```
