# Authoring WASI Tools in Rust for Nudge

Nudge allows compiling native Rust crates into WebAssembly modules using the standardized `wasm32-wasip1` target, providing sandboxed execution with strict resource boundaries.

## 1. Project Setup

Create a new Rust library crate:
```bash
cargo new --lib my_tool
cd my_tool
```

Configure `Cargo.toml` to emit a C dynamic library (`cdylib`):
```toml
[package]
name = "my_tool"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

## 2. Implement Memory Marshalling & Exported Functions

In `src/lib.rs`, export `malloc`, `free`, and your entry point:
```rust
use std::slice;

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

#[no_mangle]
pub extern "C" fn transform(ptr: *const u8, len: usize) -> u64 {
    let input = unsafe { slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(input);
    let upper = text.to_uppercase();
    let bytes = upper.into_bytes();
    let out_len = bytes.len();
    let out_ptr = bytes.as_ptr() as u32;
    std::mem::forget(bytes);
    ((out_ptr as u64) << 32) | (out_len as u64)
}
```

## 3. Compile to `wasm32-wasip1`

Ensure you have the Rust target installed and compile in release mode:
```bash
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1 --release
```

## 4. Consuming the Tool in Nudge (`.ndg`)

Declare the WASM tool in your Nudge code:
```nudge
tool to_upper(s: string) -> string {
    impl: wasi("target/wasm32-wasip1/release/my_tool.wasm").transform(s = s)
    side_effects: none
}

fn main() -> string uses Tool {
    to_upper("hello from nudge wasm host")
}
```
