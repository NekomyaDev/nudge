# RFC & Architectural Roadmap: Pure Nudge Native Execution Engine
# 原生独立执行引擎与去 Python 依赖路线图

> **Status:** Draft / Active Planning  
> **Authors:** Nudge Core Architecture Team  
> **Language Support:** English & Chinese (zh-CN)  
> **Target Version:** v1.3 – v2.0  

---

## 1. Executive Summary / 概述

Nudge was born as a typed, effect-aware agentic programming language designed to make LLM applications deterministic, verifiable, and budget-controlled.

To rapidly validate language semantics and tap into the AI ecosystem, the compiler (`nudgec`, written in Rust) was initially built as a **transpiler targeting Python** (`nudge_runtime` on PyPI) and later TypeScript (`nudge_runtime.ts`).

While this strategy allowed rapid bootstrapping, **reliance on an external Python runtime imposes severe limitations**:
1. **Cold-Start Latency:** The Rust compiler checks and transpiles code in under 2ms, but the Python interpreter startup takes 150–300ms.
2. **Environment & Packaging Friction:** Users face `venv`, pip versions, system package conflicts, and Python 3.10 vs 3.12 discrepancies.
3. **Impaired Portability:** Pure computational algorithms, stateful agents, and games written in pure Nudge cannot run without an external Python or Node.js environment.
4. **Resource Overhead:** Python's Global Interpreter Lock (GIL) and runtime memory footprint contradict Nudge's lean, high-throughput philosophy.

**The Ultimate Goal:** Evolve Nudge into a **fully self-contained, standalone language** where `nudgec run <file.ndg>` executes natively in Rust with zero Python/pip dependencies, while retaining Python purely as an optional plugin for legacy package interop.

---

## 2. Architecture: Current vs. Target / 架构对比

### Current Transpiler Architecture (现阶段转译架构)
```
┌────────────────────────────────────────────────────────┐
│ nudgec (Rust)                                          │
│ [Lexer] ➔ [Parser] ➔ [Checker & Effect System]        │
│                        │                               │
│                        ▼                               │
│              [Codegen (Python / TS)]                   │
└────────────────────────┬───────────────────────────────┘
                         │ emits out/<name>.py
                         ▼
┌────────────────────────────────────────────────────────┐
│ External Python Runtime (nudge_runtime)                │
│ Requires: Python 3.10+, pip, venv, requests, pydantic │
│ [LLM Adapter] [Repair Loop] [Trace Store] [Par Worker]│
└────────────────────────────────────────────────────────┘
```

### Target Native Execution Architecture (目标原生独立架构)
```
┌─────────────────────────────────────────────────────────────────┐
│ nudgec (Unified Rust Binary / 纯 Rust 单一独立二进制)          │
│                                                                 │
│ [Lexer] ➔ [Parser] ➔ [Checker & Inference] ➔ [AST / HIR]        │
│                                                      │          │
│         ┌────────────────────────────────────────────┴─────┐    │
│         ▼                                                  ▼    │
│  [Native Interpreter & VM]                         [Codegen]    │
│  - Pure Tree-Walking / Bytecode Execution         - Standalone  │
│  - Built-in Agent State Store                      - WASM Target│
│  - Native Tokio + Reqwest (rustls) LLM Client     - Python (Opt)│
│  - Native JSON Schema Validation & Repair Loop                  │
│  - Native NTF v1 Deterministic Replay Engine                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                         │ Zero External Dependencies
                         ▼
               Direct OS Process / Terminal
```

---

## 3. Step-by-Step Implementation Roadmap / 逐步实施路线图

Transitioning from an external runtime to a pure native execution engine will be executed in four disciplined phases:

```
┌────────────────┐      ┌────────────────┐      ┌────────────────┐      ┌────────────────┐
│    Phase 1     │ ───► │    Phase 2     │ ───► │    Phase 3     │ ───► │    Phase 4     │
│   Pure Nudge   │      │ Native Rust RT │      │ Standalone AOT │      │ Python Plugin  │
│  Interpreter   │      │ (Tokio+Reqwest)│      │  & WASM Target │      │  Decoupling    │
└────────────────┘      └────────────────┘      └────────────────┘      └────────────────┘
```

---

### Phase 1: Pure Nudge In-Tree AST Interpreter (`nudgec run`)
### 第一阶段：纯 Nudge 语法树解释器（支持无外部依赖直接执行）

* **Goal:** Enable execution of pure Nudge programs (functions, arithmetic, branching, `route{}`, and `agent` state machines) directly inside `nudgec` with zero external dependencies.
* **Scope:**
  - Build `crates/nudgec/src/eval/`:
    - `val.rs`: Native Rust enum `Value` (`Int(i64)`, `Float(f64)`, `Bool(bool)`, `Str(String)`, `List(Vec<Value>)`, `Map(HashMap<String, Value>)`).
    - `env.rs`: Scoped lexical environment for variable bindings.
    - `interpreter.rs`: AST tree-walking evaluator implementing:
      - Expression evaluation: binary/unary ops, list indexing, string concatenation.
      - Function calls and returns.
      - Value routing (`route { label: val when cond, ... }`).
      - State mutation (`agent` block state fields, `=` and `+=`).
  - Add CLI command:
    ```sh
    nudgec run <file.ndg> [--fn <entry_point>]
    ```
* **Milestone Deliverable:**
  Running `nudgec run examples/dungeon-crawler/dungeon-crawler.ndg` executes the complete turn-based RPG in pure Rust in **under 3 milliseconds**, with no Python installation required.

---

### Phase 2: In-Tree Native AI & Tool Runtime (`crates/nudge-rt`)
### 第二阶段：Rust 原生 AI 与工具运行时（内嵌 Tokio + Reqwest）

* **Goal:** Bring LLM calling, schema repair, deterministic replay, and MCP tool execution directly into the Rust toolchain.
* **Components to Implement in Rust:**
  1. **HTTP/SSE Transport (`reqwest` + `rustls`):**
     - Direct asynchronous streaming connections to OpenAI, Anthropic, Gemini, Groq, Mistral, and local Ollama instances.
     - Zero OpenSSL dependency (`rustls` for static compilation).
  2. **Schema & Repair Engine:**
     - Using `serde_json` and schema contracts derived during type-checking.
     - In-memory automated repair loop: when model returns invalid JSON, formulate feedback prompt and retry within budget limits.
  3. **NTF v1 Replay Engine:**
     - Native reading/writing of `.jsonl` trace files.
     - Deterministic mode (`NUDGE_MODE=replay`): match prompts against trace records and return cached outputs without network requests.
  4. **Native MCP Stdio Client:**
     - Spawn external MCP servers as child processes and communicate over JSON-RPC 2.0 stdio using asynchronous Tokio I/O.
* **Milestone Deliverable:**
  `nudgec run examples/chatbot/chatbot.ndg` runs end-to-end against live providers and replays traces in CI with $0 token cost, completely independent of Python.

---

### Phase 3: Bytecode VM, Standalone Binary & WebAssembly
### 第三阶段：字节码虚拟机、独立二进制导出与 WebAssembly

* **Goal:** Maximize execution performance and enable single-binary deployment.
* **Features:**
  1. **Nudge Bytecode (NBC):**
     - Compile AST into compact bytecode instructions (`LOAD_CONST`, `STORE_STATE`, `CALL_ROUTE`, `INVOKE_LLM`).
     - Fast dispatch loop (register-based or stack-based).
  2. **Standalone Executable Compiler (`nudgec build --standalone`):**
     - Embed the runtime engine and compiled bytecode into a single self-contained ELF/Mach-O/PE binary.
     - Distribute an agent as a single executable (`./my-agent`) that runs on any server without installing runtimes.
  3. **WebAssembly Target (`wasm32-unknown-unknown` / WASI):**
     - Compile Nudge programs and interpreter into WebAssembly to run entirely client-side in browsers or edge workers (Cloudflare Workers, Fastly Compute).
* **Milestone Deliverable:**
  Exporting any Nudge agent as a single, portable binary with sub-millisecond cold start.

---

### Phase 4: Decoupling Python as an Optional Interop Plugin
### 第四阶段：将 Python 解耦为可选互操作插件

* **Goal:** Demote Python from a mandatory core runtime dependency to an optional bridge.
* **Mechanism:**
  - If a Nudge script uses `use python "pandas" as pd`:
    - `nudgec` dynamically locates Python via `libpython` / `pyo3` or IPC bridge.
    - If Python is absent, the compiler reports a clean, targeted diagnostic:
      ```
      error[E0901]: Program requires Python interop module 'pandas', but Python 3 is not available.
      hint: Standard pure Nudge features do not require Python.
      ```
  - All standard language features (agents, LLM calls, tools, replay, parallel routines) run without Python.

---

## 4. Technical Specification: Native Interpreter Core / 核心技术规范

### 4.1 Value Representation (Rust 数据结构设计)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum NudgeVal {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<NudgeVal>),
    Record(HashMap<String, NudgeVal>),
    AgentState {
        name: String,
        fields: HashMap<String, NudgeVal>,
    },
}
```

### 4.2 Effect-Enforced Execution Contract (副作用约束机制)

The interpreter mirrors the compiler's inferred effect system:
- **`pure` Context:** No network I/O, no filesystem modifications, deterministic execution guaranteed.
- **`llm` Context:** Handled by provider pool with budget ceiling checks.
- **`tool` Context:** Executed via registered MCP stdio channels or native tool registry.

---

## 5. Migration & Compatibility Guarantee / 迁移与兼容性保证

1. **Non-Breaking Transition:**
   - `nudgec build` will continue emitting Python and TypeScript for teams whose infrastructure relies on Python/Node deployment pipelines.
   - `nudgec run` introduces native execution alongside existing targets without breaking existing CI workflows.
2. **Trace Compatibility:**
   - Traces produced by the native runtime strictly adhere to the frozen **NTF v1 standard** (`docs/ntf-spec.md`). Traces recorded in Python replay identically in the native Rust engine.
3. **Language Consistency:**
   - Syntax and typing rules remain strictly unified across all backends.

---

## 6. Summary / 总结

By transitioning Nudge from a transpiler to a native Rust execution engine:
- **Zero Dependencies:** Developers can download one standalone binary (`nudgec`) and build production-grade agentic systems immediately.
- **Microsecond Latency:** Cold starts drop from ~300ms to <2ms.
- **True Language Independence:** Nudge becomes a first-class, sovereign programming language rather than a frontend wrapper around Python.
