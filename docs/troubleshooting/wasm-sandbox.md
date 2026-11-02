# Troubleshooting WASM Tool Sandboxing in Nudge

Nudge executes external agent tools in a sandboxed WebAssembly (WASI Preview 1) environment. This guide addresses common traps, capability limits, and resolution steps.

## Common Issues and Solutions

### 1. Missing Memory Allocation Exports (`malloc` / `free`)

**Symptom**: `HostToolError: module does not export malloc or free memory functions`.

**Root Cause**: When Nudge marshals strings or JSON structures across the host-guest boundary, it requires the WASM guest to allocate memory for incoming byte buffers.

**Fix**: Ensure your WASM module exports canonical memory allocation symbols:
```rust
// In Rust WASI tool crate
#[no_mangle]
pub extern "C" fn malloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() {
        unsafe { drop(Vec::from_raw_parts(ptr, 0, size)) };
    }
}
```

### 2. WASI Host Import Version Mismatches (`preview1` vs Component Model)

**Symptom**: `Trap: unknown import 'wasi_snapshot_preview1::fd_write'` or component model format error.

**Root Cause**: Nudge currently targets the standardized `wasi_snapshot_preview1` ABI. Modules compiled with the experimental WASI Preview 2 / component model (`wasm32-wasip2`) cannot link directly against Preview 1 hosts without an adapter.

**Fix**: Compile your tool targeting `wasm32-wasip1`:
```bash
cargo build --target wasm32-wasip1 --release
```

### 3. Memory Limit Exceeded Traps (`MAX_MEMORY` Bounds)

**Symptom**: `HostToolError: memory allocation exceeds sandbox ceiling (limit: 64MB)`.

**Root Cause**: The WASM runtime enforces strict memory caps to prevent denial-of-service from runaway tool processes.

**Fix**: Configure memory limits via `nudge.toml` or the environment variable:
```bash
export NUDGE_WASM_MAX_MEMORY_MB=128
```

## Verification Commands

Inspect and validate WASM binaries before attaching them to agents:
```bash
# Verify exported functions and WASI imports
nudgec tool verify ./target/wasm32-wasip1/release/my_tool.wasm

# Test run tool directly with arguments
nudgec tool run ./target/wasm32-wasip1/release/my_tool.wasm -- '{"query": "ping"}'
```
