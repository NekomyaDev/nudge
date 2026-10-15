# WebAssembly Runtime Target
# WebAssembly 原生执行目标

> **Status:** Shipped in v1.5 (Native Runtime Phase 3)
> **Targets:** `wasm32-wasip1`, `wasm32-unknown-unknown`

---

## 1. Overview / 概述

Nudge provides native WebAssembly support, enabling agent evaluation and deterministic trace replay in sandboxed environments such as browsers, edge workers (Cloudflare Workers, Fastly Compute), and WASI runtimes (wasmtime, wasmer).

The WebAssembly target provides:
- **Offline NTF Trace Replay:** Run recorded agent traces inside WASI with zero network access and zero child processes.
- **Sandboxed In-Process Evaluator:** Full language semantics (pure computations, state machines, routes, parallel control flows) execute inside WebAssembly.
- **Host Bridge for Live Tools:** A standardized extern import (`nudge_host_tool_call`) allowing JavaScript host environments to fulfill live tool calls.

---

## 2. Sandboxed Architecture / 沙箱架构设计

In WebAssembly execution environments, native operating system sockets and process-spawning capabilities are either restricted or unavailable. The Nudge runtime cleanly partitions capabilities:

```
┌────────────────────────────────────────────────────────┐
│ WebAssembly Sandbox (wasm32-wasip1 / unknown-unknown) │
│                                                        │
│  - In-Tree AST Evaluator & Scoped Environment          │
│  - Pure Standard Library Functions                     │
│  - Deterministic NTF Replay Engine                     │
│  - In-Process Fake Desktop for Computer Use Tests      │
└──────────────────────────┬─────────────────────────────┘
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
   Offline Replay Path          Live Host Bridge
   (Serves from trace.jsonl)    (nudge_host_tool_call)
   Zero network / Zero OS I/O   Browser / Edge Host
```

### What is Included on wasm32

- **Pure language features:** arithmetic, string operations, records, lists, pattern matching, state machines, and routing.
- **Deterministic replay engine:** trace matching and cached response lookup for `llm.call` and `tool.call` events.
- **Fake providers:** virtual computer-use desktop and deterministic mock providers for $0 testing.

### What is Compiled Out on wasm32

- **Direct network stack:** `ureq`, `rustls`, and `ring` are excluded to eliminate unsupported system dependencies and keep binary size minimal.
- **Child process execution:** OS-level subprocess spawning for stdio MCP servers and native desktop tools is omitted.
- Attempting live network or child process execution without a replay trace or host bridge produces an explicit error: `"network not available on wasm32; use NUDGE_REPLAY"` or `"MCP process not available on wasm32; use NUDGE_REPLAY"`.

---

## 3. Offline Trace Replay: `nudge-replay-wasi` / WASI 离线重放

The `nudge-replay-wasi` binary is a dedicated WASI executable (`wasm32-wasip1`) designed for offline replay.

### How It Works

1. **Input Bundle and Trace:** Takes a serialized `StandaloneBundle` bincode file (pre-compiled AST items and entry point) and an NTF trace JSONL file.
2. **Environment Configuration:** Sets `NUDGE_REPLAY` internally pointing to the provided trace path.
3. **Trace-Driven Evaluation:** Executes `nudgec::eval::run_program`. When the program encounters `llm"""` blocks or `tool` calls, the replay engine intercepts the call and serves recorded outcomes directly from the trace.
4. **Byte-Identical Output:** Verified under `wasmtime` to produce output byte-identical to the native recording run.

### Running under wasmtime

```sh
# Pre-open directory containing bundle and trace
wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl [optional_args...]
```

### Binary Size

When compiled with size optimizations (`opt-level = "z"`, LTO, bincode bundle), the `nudge-replay-wasi.wasm` module measures approximately **526KB** (reduced from ~599KB), making it suitable for fast distribution and rapid instantiation.

---

## 4. Live Tool Calls via Host Bridge / 宿主桥接工具调用

While direct subprocess spawning is not possible inside a WebAssembly sandbox, a WASM agent can execute live tools through an external host bridge.

### The `nudge_host_tool_call` Import Contract

The runtime defines an external function import implemented by the hosting environment (browser JavaScript or edge runtime):

```rust
extern "C" {
    fn nudge_host_tool_call(
        tool_ptr: *const u8,
        tool_len: usize,
        params_ptr: *const u8,
        params_len: usize,
        out_ptr: *mut u8,
        out_cap: usize,
    ) -> i32;
}
```

### Execution Flow

1. **Invocation:** When a Nudge program executes a tool call in live mode, the runtime serializes the tool name and argument record to JSON.
2. **Host Delegation:** The runtime invokes `nudge_host_tool_call` with pointers to the serialized parameters.
3. **Host Fulfillment:** The JavaScript host reads the arguments from WASM memory, invokes the corresponding tool (via browser API, HTTP `fetch`, or local service), and serializes the result.
4. **Result Return:** The host writes the response JSON into the output buffer in WASM memory.
5. **Deserialization:** The Nudge runtime deserializes the output into typed values and continues evaluation.

See `examples/wasm-host/` for a reference implementation of the host harness and browser integration code.

The live-tool host bridge is verified end-to-end via `scripts/wasm_host_e2e.sh` and `examples/wasm-host/e2e.mjs`. Under Node WASI, the harness instantiates the WASM module, provides the `nudge_host_tool_call` import, and executes a live `calculator::add({a: 17, b: 25})` call over WASM linear memory, round-tripping to 42 with zero network access.

---

## 5. Building the WASM Artifacts / 构建指南

Prerequisites: Rust toolchain with WebAssembly targets installed.

```sh
# Add WebAssembly targets
rustup target add wasm32-wasip1
rustup target add wasm32-unknown-unknown
```

### Build the WASI Replay Binary

```sh
cargo build --release -p nudge-agent --bin nudge-replay-wasi --target wasm32-wasip1
```

The resulting `.wasm` binary is located at `target/wasm32-wasip1/release/nudge-replay-wasi.wasm`.

### Build the Runtime Library for Web / Edge

```sh
cargo build --release -p nudge-runtime --target wasm32-unknown-unknown
```

---

## 6. Capability Matrix / 特性支持矩阵

| Capability | Native Agent | WASM (nudge-replay-wasi) | WASM + Host Bridge |
|:---|:---:|:---:|:---:|
| Bytecode VM (Default) | Supported | Supported | Supported |
| Pure AST Evaluation (Fallback) | Supported | Supported | Supported |
| State Machines & Routes | Supported | Supported | Supported |
| Deterministic NTF Replay | Supported | Supported | Supported |
| Offline $0 Testing | Supported | Supported | Supported |
| Direct HTTP/TLS Network | Supported | Compiled out | Delegated to host |
| Direct OS Subprocess Spawn | Supported | Compiled out | Delegated to host |
| Live Tool Execution | Native stdio / HTTP | Trace replay only | Supported via host bridge |
