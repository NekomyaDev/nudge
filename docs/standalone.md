# Standalone Agent Executables (Cargo-Free)
# 独立 Agent 原生二进制构建（无需 Cargo）

> **Status:** Shipped in v1.5 (Native Runtime Phase 3)
> **Applies to:** `nudgec build --standalone`

---

## 1. Overview / 概述

Nudge can compile and package an agent into a single, self-contained native executable binary:

```sh
nudgec build --standalone <prog.ndg> [-o <out>] [--fn <entry>]
```

The resulting binary runs directly on the target operating system with zero external dependencies: no Python runtime, no Node.js, and **no Rust toolchain (`cargo` / `rustc`) required at the build site**.

---

## 2. How It Works: AST Stamping / 语法树盖印机制

Previous standalone packaging approaches required invoking Cargo in a temporary directory to compile a custom crate, requiring a full Rust toolchain and taking several seconds per build.

Nudge now uses **cargo-free AST stamping** (append-payload packaging):

```
┌─────────────────────────────────────────────────────────────────┐
│ Pre-built nudge-agent Runtime Binary                            │
│ (Evaluator + Runtime Engine + Built-in Stdlib)                  │
├─────────────────────────────────────────────────────────────────┤
│ Appended Payload:                                               │
│ 1. Serialized StandaloneBundle bincode (entry point + AST)      │
│ 2. 64-bit Little-Endian Payload Length (8 bytes)                │
│ 3. Magic Tag: b"NDGAGNT1" (8 bytes)                             │
└─────────────────────────────────────────────────────────────────┘
```

### Build Steps

1. **Static Validation:** `nudgec` compiles and type-checks the `.ndg` source. If errors are detected, build terminates immediately with clear diagnostics and no binary is produced.
2. **AST Serialization:** The verified AST items and selected entry function are serialized into a bincode-encoded `StandaloneBundle`.
3. **Runtime Discovery:** `nudgec` locates the pre-compiled `nudge-agent` runtime binary.
4. **Copy and Append:** `nudgec` copies the runtime binary to the output destination and appends the payload with the `NDGAGNT1` footer.
5. **Executable Permissions:** On Unix systems, executable permissions (`0o755`) are automatically set.

Because the program is pre-compiled into an AST at build time, the grammar parser is completely omitted from the agent binary. The resulting binary contains the bytecode VM (the default execution engine for pure compute and routes), effect delegation to the evaluator, and runtime engine.

---

## 3. Full Language Capabilities / 完整语言特性支持

Standalone agent binaries support the full Nudge language surface:

- **Typed LLM Calls:** `llm"""` blocks with schema enforcement, automatic retry, and schema repair.
- **Tool Invocations:** Declarative tool bindings and MCP tools over stdio channels.
- **Computer Use:** Full `computer.observe`, `computer.click`, `computer.type`, and action primitives.
- **Native Parallelism:** `par map`, `par race`, and `par all` with race-safety guarantees.
- **Typed Decisions:** `decide { ... }` blocks with calibrated confidence and deadlines.
- **Deterministic Replay:** Trace replay via `NUDGE_REPLAY` for offline validation at $0 cost.
- **Security Grants:** Execution-layer tool access policies via `NUDGE_TOOL_GRANTS`.
- **Streaming:** SSE provider streaming with `stream let` constructs.

---

## 4. Runtime Flavors: Full vs. Replay-Only / 运行时规格

To balance capability and binary size, the standalone runtime is provided in two distinct flavors:

| Flavor | Size | Cargo Features | Description |
|:---|:---:|:---|:---|
| **Full (Default)** | ~2.09MB | `default` (with `live-provider`) | Full live-capable runtime with embedded HTTP/TLS stack (`ureq` + `rustls`). Connects to real LLM providers and live MCP services (reduced from ~2.18MB via bincode payload and tuned release profile). |
| **Replay-Only** | ~832KB | `--no-default-features` | Ultra-compact runtime with network and TLS dependencies compiled out. Evaluates logic and serves all model/tool calls offline from recorded NTF traces (reduced from ~909KB via bincode payload). |

Both flavors are stripped and compiled with size optimization (`opt-level = "z"`, LTO, codegen-units=1, panic=abort). Standalone payloads use compact bincode encoding instead of JSON while preserving the `[binary][bundle][u64 LE len][NDGAGNT1]` binary framing.

---

## 5. Locating the Runtime Binary / 运行时定位机制

When `nudgec build --standalone` is executed, `nudgec` locates the pre-built `nudge-agent` binary using the following priority order:

1. **Environment Variable:** The path specified in `NUDGE_AGENT_RUNTIME`.
2. **Sibling Binary:** A `nudge-agent` binary (or `nudge-agent.exe` on Windows) located in the same directory as the executing `nudgec` binary.

If neither location contains a valid runtime binary, `nudgec` exits with an informative error instructing the user to install `nudge-agent` alongside `nudgec` or set `NUDGE_AGENT_RUNTIME`.

---

## 6. One-Command Packaging via Makefile / Makefile 辅助命令

A Makefile helper provides convenient one-command packaging shortcuts:

```sh
# Build a full live-capable standalone agent (~2.09MB)
make agent PROG=examples/chatbot/chatbot.ndg OUT=bin/chatbot

# Build a compact replay-only standalone agent (~832KB)
make agent-small PROG=examples/chatbot/chatbot.ndg OUT=bin/chatbot-replay
```

These targets manage runtime prerequisites and invoke `nudgec build --standalone` with the corresponding runtime configuration.

---

## 7. CLI Usage Examples / 命令行示例

### Basic Build

```sh
# Compiles agent.ndg and writes native binary ./agent
nudgec build --standalone agent.ndg
./agent
```

### Custom Output Path and Entry Function

```sh
# Specify explicit output binary name and non-main entry function
nudgec build --standalone agent.ndg -o release/worker --fn process_batch
./release/worker input.json
```

### Deterministic Trace Replay

A standalone binary honors standard Nudge environment variables, including offline trace replay:

```sh
# Run entirely offline from a recorded trace with zero network calls
NUDGE_REPLAY=traces/recorded_run.jsonl ./agent
```
