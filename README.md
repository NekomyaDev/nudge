<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Don't parse your agents. Nudge them.</strong><br>
  A typed, replayable, budget-aware programming language for AI agents.<br>
  Native bytecode runtime and standalone compiler. Compiles to Python and TypeScript.
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.5-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Native%20%7C%20Python%20%7C%20TypeScript%20%7C%20WASM-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
  <a href="https://snyk.io"><img alt="Snyk Verified" src="https://img.shields.io/badge/Snyk-Verified%20Secure-4C556A?logo=snyk"></a>
</p>

---

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.zh-CN.md">中文</a>
</p>

---

## Why Nudge?

Production agents are still held together with glue code: prompt chains parsed by hand, tool calls wrapped in try/except blocks, missing replayability, unpredictable costs, and no regression tests. Libraries patch symptoms after they happen. **Nudge fixes the layer where the problem actually lives: the language.**

| Pain Point | Libraries | Nudge |
|:---|:---|:---|
| Untyped LLM output | Validate at runtime | Schema is a first-class language type, verified at compile time |
| Hidden side effects | Invisible | 10 explicit typed effects declared in signatures (`uses LLM, Tool, IO, ...`) |
| No regression testing | Record and replay bolted on | Every run writes an NTF trace, and every trace is a deterministic $0 test |
| Cost surprises | Monitored after the bill arrives | Budget is a hard contract enforced by compiler and runtime |
| Concurrency spaghetti | Manual async loops | Native `par map / race / all` with compile-time race safety |

## A Taste of Nudge

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // zero tokens burned in CI
}
```

The compiler proves the schema matches, infers effects, and bounds worst-case cost. The runtime records every call to a content-addressed trace you can diff, commit, and replay for $0.

## Quick Start

### 1. Install Nudge

**One-line installer (Recommended):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell as Administrator)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Package managers and containers:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Pre-built binaries for Linux, macOS, and Windows are also available on the [Releases](https://github.com/NekomyaDev/nudge/releases) page.

### 2. 60-Second First Run (Native, $0)

No Python, no Node.js, and no API keys required. Nudge includes a native stack VM and an in-memory deterministic fake provider:

```sh
# Create a program
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Create a greeting for {name}. Return message and timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

fn main() -> Greeting uses LLM {
    greet("World")
}
EOF

# Type-check your program
nudgec check hello.ndg

# Run directly on the native Bytecode VM ($0, no keys needed)
nudgec run hello.ndg

# Run offline regression tests
nudgec test hello.ndg
```

### 3. Interactive Lessons and Project Scaffolding

```sh
# Six interactive lessons in your terminal (hello, types, decide, policy, test, trace)
nudgec learn

# Scaffold a project from ready-made templates (triage, chatbot, rag-agent, etc.)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Compile to Python or TypeScript (Optional)

If you are deploying into an existing Python or TypeScript service:

```sh
# Python target
pip install nudge-runtime          # standard library only, zero dependencies
nudgec build hello.ndg              # emits out/hello.py
python3 out/hello.py

# TypeScript target
nudgec build-ts hello.ndg             # emits out/hello.ts
```

## Core Features at a Glance

| Feature | What It Delivers | Section |
|:---|:---|:---|
| **Typed LLM Calls** | Output schema is a type, and violations trigger automatic repair | [A Taste of Nudge](#a-taste-of-nudge) |
| **Control Flow & Data** | `if/else`, `for` loops, record literals `{k: v}`, indexing, mutable locals | [Control Flow & Data](#control-flow--data) |
| **Typed Effects** | 10 known effects tracked and checked at compile time | [Typed Effects](#typed-effects) |
| **Workspace & Store** | Sandboxed scoped filesystem + typed persistent key-value store | [Workspace & Store](#scoped-workspace--persistent-store) |
| **Local UI** | Typed views, windows, headless tests, and native OS desktop window | [Local UI](#local-ui) |
| **Tasks & Concurrency** | Task spawn/join, cooperative cancellation, safe `par race`, budget release | [Task Lifecycle & Concurrency](#task-lifecycle--concurrency) |
| **Provider Auth** | Fail-closed RFC 8628 OAuth device flow, no foreign token harvesting | [Provider Authentication](#provider-authentication) |
| **Typed Decisions** | Batched classification, yes/no, rubric scoring over states (`/v1/systemone`) | [Typed Decisions](#typed-decisions) |
| **Computer Use** | Screen observation, typed actions, app allowlisting, kill switch | [Computer Use](#computer-use) |
| **Bytecode VM** | Default stack VM engine (~70x faster pure compute, deep recursion safety) | [Bytecode VM Engine](#bytecode-vm-engine) |
| **Standalone Agents** | Cargo-free single binary packaging (~2.09MB live, ~832KB replay) | [Standalone Native Agents](#standalone-native-agents-cargo-free) |
| **WebAssembly** | Offline replay in WASI (~526KB), live tool host bridge | [WebAssembly Target](#webassembly-target) |
| **Deterministic Traces** | Open NTF v1 traces, $0 replay, CI regression gates, trace diffing | [Deterministic Traces & Agent CI](#deterministic-traces--agent-ci) |
| **Privacy Redaction** | Default auto-redaction of secrets/PII at a single choke point | [Privacy & Trace Redaction](#privacy--trace-redaction) |

## Language & Capabilities

### Control Flow & Data

Nudge provides familiar structured programming constructs with deterministic safety bounds:

```nudge
fn process_scores(raw: [int]) -> { total: int, count: int } {
    let mut total = 0
    let mut count = 0

    for (score, idx) in raw {
        if score >= 50 {
            total = total + score
            count = count + 1
        } else {
            // skip failing scores
        }
    }

    { total: total, count: count }
}
```

- **Branching:** `if/else` and `else if` expressions supporting multi-statement branch blocks.
- **Iteration:** sequential `for item in coll` loops and indexed `for (item, idx) in coll` loops.
- **Structured Data:** record literals `{ field1: expr1, field2: expr2 }` with static typing and field access (`obj.field`).
- **Indexing:** indexed access on collections and records (`items[0]`, `map["key"]`). Bounds errors fail cleanly with `E0705`.
- **Mutable Locals:** explicit `let mut name = init` variables with reassignment (`name = next`).
- **Loop Bounds:** loops are bound by `NUDGE_MAX_LOOP_ITERATIONS` (default 10,000) to prevent runaway execution, raising `E0703` on overrun.

### Typed Effects

Every side effect is declared in the function signature and validated by the compiler:

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Calling unlisted effects triggers compiler error E0301
}
```

Nudge defines 10 known effects (`crates/nudgec/src/check.rs`):
- `LLM`: Invokes language model completions or streaming prompts.
- `Tool`: Invokes external MCP or registered tools.
- `IO`: Host input/output operations (`print`, `println`).
- `Decision`: Queries decision models via `decide{}`.
- `Computer`: Interacts with desktop environments and accessibility trees via `computer.*`.
- `Workspace`: Scoped read and write operations inside the sandboxed workspace.
- `WorkspaceDelete`: Destructive file or directory deletion inside the workspace.
- `Store`: Persistent key-value storage operations via `store.*`.
- `UI`: Creates, mounts, or dispatches events to graphical user interface elements.
- `Auth`: Accesses authenticated provider resources (supports scoped `Auth(account)`).

Functions without a `uses` clause are proven pure at compile time. Unknown effects are rejected with `E0302`.

### Scoped Workspace & Persistent Store

Local agents must work with files and maintain state across runs without compromising system security:

```nudge
record Config {
    theme: string,
    max_entries: int
}

fn update_workspace_file(path: string, new_content: string) -> bool uses Workspace, Store {
    // 1. Scoped filesystem access (path traversal and symlink escapes denied)
    if workspace.exists(path) {
        workspace.write(path, new_content)   // Atomic write: staging, fsync, rename
    }

    // 2. Typed persistent store (survives process restarts and reboots)
    store.set("app_config", Config { theme: "dark", max_entries: 100 })
    true
}
```

- **Sandboxed Workspace (`workspace.*`):** all operations are confined to an explicit workspace root (configured via `--workspace` or `NUDGE_WORKSPACE_DIR`, defaulting to the current directory). Path traversals (`../`) and symlink escapes outside the root are blocked. Writes are crash-consistent (atomic write-to-temp and rename).
- **Separation of Deletion Authority:** deleting files or directories requires the dedicated `WorkspaceDelete` effect (`uses WorkspaceDelete`), preventing general write routines from deleting directories.
- **Persistent Key-Value Store (`store.*`):** strongly typed storage saved to `<workspace>/.nudge/store.json` (or `NUDGE_STORE_PATH`). Unlike ephemeral run checkpoints, this data store persists across distinct executions and reboots.

### Process & File Watching

Build agents that drive build/test runners, language servers, and live-reload tooling — with the same effect + grants discipline as everything else:

```nudge
fn watch_and_rebuild() -> bool uses FsWatch, Process {
    // 1. Bounded, workspace-confined file watching (no indefinite hang)
    let changes = fs.watch("src", 5000)          // dir/path + timeout (ms)

    // 2. Controlled child process: program and args SEPARATELY (never a shell)
    let handle = proc.spawn("cargo", ["build", "--quiet"], 60000)
    let out = proc.await(handle)
    proc.alive(handle) == false
}
```

- **File Watching (`fs.watch`, `FsWatch` effect):** watch a workspace path for change events, bounded by a timeout (it never hangs indefinitely) and confined to the workspace root (escapes denied). Undeclared use is typed away (`E0301`).
- **Controlled Processes (`proc.*`, `Process` effect):** `proc.spawn(cmd, argv, timeout_ms)` / `proc.run(...)` (spawn + await) / `proc.await` / `proc.kill` / `proc.alive`. `cmd` and its `argv` are passed **separately** and executed without a shell (no shell-injection surface). Lifecycle is bounded (`E0921`) with **no orphan processes** (children are reaped on task exit or cancel) and kill-on-cancel. Executing anything is **fail-closed**: denied by default unless authorized by `NUDGE_PROCESS_GRANTS` (`E0920`, workspace confinement `E0922`).

### Local UI

Build native human-in-the-loop interfaces, approval stations, and interactive inspection panels directly in Nudge:

```nudge
type ReviewState = { prompt: string, approved: bool }

view ReviewCard(state: ReviewState) -> View {
    column(padding = 16, gap = 12) [
        text("Agent Review Station", size = 18, bold = true),
        text("Request: " + state.prompt),
        row(gap = 8) [
            button("Approve", on_click = approve_action),
            button("Reject", on_click = reject_action)
        ]
    ]
}

agent ReviewStation {
    state {
        prompt: string = "Execute deploy script",
        approved: bool = false
    }

    fn approve_action() uses UI, IO {
        state.approved = true
        io.println("Action approved")
    }

    fn reject_action() uses UI {
        state.approved = false
    }

    window MainWindow {
        title: "Nudge Review Station",
        width: 500,
        height: 300,
        content: ReviewCard(state)
    }
}
```

- **Dual-Mode Execution:**
  - **Headless Testing:** `mount_headless()` and `dispatch_click()` evaluate UI trees and state transitions entirely in memory without opening a physical window. Runs in CI for $0.
  - **Native Desktop GUI:** when compiled with the `native-gui` Cargo feature, `window.open()` creates a native OS window powered by `winit` and `softbuffer`.
- **Headless Fallback:** setting `NUDGE_UI_HEADLESS=1` or running during trace replay (`NUDGE_REPLAY`) routes window calls to the headless engine automatically.

### Task Lifecycle & Concurrency

Coordinate concurrent agent arms with formal task state management and cooperative cancellation:

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race automatically cancels losing branches
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **Cooperative Cancellation:** when a race resolves, losing branches are halted at loop, stream, and effect boundaries instead of continuing to spend budget in the background.
- **Budget Protection:** unspent budget reservations under `NUDGE_BUDGET` are immediately released upon task cancellation.
- **Fail-Closed Grants:** cancelled tasks immediately lose permission to dispatch further tool calls (`Tool`) or computer actions (`Computer`).
- **Task Primitives:** explicit task control via `task.spawn`, `task.join`, `task.status`, and `task.cancel`.

### Provider Authentication

Secure credential management built directly into the language and runtime:

- **Fail-Closed Authentication:** functions declare explicit authentication requirements with `uses Auth` or `uses Auth(account)`. Unauthenticated calls fail closed.
- **No Foreign Token Harvesting:** Nudge never reads private token caches or credential stores belonging to third-party tools.
- **Native RFC 8628 Device Flow:** interactive terminal authorization via OAuth 2.0 Device Authorization Grant with secure credential storage in `~/.nudge/credentials.json` (file mode 0600).
- **Provider Gates:** providers must satisfy verifiable security contracts (`E-auth-0401` for missing credentials, `E-auth-0403` for scope deficits, `E-auth-0404` for disabled providers).
- **Unproven Providers Stay Closed (Permission Gate):** a provider whose official third-party usage permission has not been proven is **fail-closed by default**. This currently applies to `antigravity`: its OAuth login flow and provider code exist, but because no official proof was found that its OAuth client and model endpoint may be used from an independent third-party client, the gate stays **disabled** (`E-auth-0404`) until permission is verified. **A mock OAuth test or a green CI run does NOT prove live-account access or usage permission.** Consumers (including any app built on Nudge) must not rely on this connection until the permission is documented. An explicit, user-asserted opt-in (`NUDGE_ENABLE_ANTIGRAVITY=1`) enables it at the user's own risk once permission is verified.

### Typed Decisions

Compile uncertainty from decision models (such as Laya and Jev) via the `/v1/systemone` wire protocol:

```nudge
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept:    "which team handles this?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "laya:multilingual", deadline: 50 }   // ms latency budget
    route {
        auto: resolve(d.dept.winner) when d.dept.confidence > 0.8,
        review: escalate(d) otherwise
    }
}
```

- **Batched Execution:** one call evaluates all questions simultaneously, returning winner, probability, distribution, and confidence.
- **Zero-Dollar Testing:** the default fake decision provider synthesizes deterministic seeded distributions for $0 test runs.
- **Live Providers:** connect local or remote models via `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'`.
- **Decision Cache:** `NUDGE_DECISION_CACHE=<path>` caches validated answers across runs so you never pay twice for the same state.
- **Policy Sweeps:** run `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` to evaluate threshold adjustments over recorded distributions with zero model invocations. Read the [Decision Documentation](docs/decision.md).

### Computer Use

Control desktop environments with typed observations and actions where the language owns control and safety while the model owns perception:

```nudge
fn automate_app(app: string) -> bool uses Computer, Decision {
    let obs = computer.observe(app, allow = [app], deadline = 30000)
    let check = decide {
        ready: "is the target button visible?" yes/no
    } on obs.tree
    with { deadline: 100 }

    let r = route {
        click: computer.click(1, allow = [app]) when check.ready.p > 0.5,
        close: computer.key("Escape", allow = [app]) otherwise
    }
    r.ok
}
```

- **Typed Primitives:** `computer.observe` returns an `Observation` record (accessibility tree, elements, optional screenshot). Actions (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`) return `ActionResult` with dispatch receipts (`action_sent`).
- **Replay Safety:** trace replay never re-fires physical actions. With `NUDGE_COMPUTER_DRIFT=1`, the runtime re-observes live and calculates a mechanical `Drift` diff without executing actions.
- **Hardening:** app allowlisting (`allow: [app]`), controller lease locks, rejection of stale element indices (`stale_state`), and a hard kill switch (`NUDGE_COMPUTER_KILL=1`).
- **Deterministic Desktop:** default fake provider runs in-memory UI scenarios (`NUDGE_COMPUTER_SCENARIO`), allowing $0 end-to-end tests. Read the [Computer Use Documentation](docs/computer-use.md).

## Native Infrastructure

### Bytecode VM Engine

Pure functions, data transformations, and route branching execute on a dedicated stack-based bytecode VM (`crates/nudgec/src/bytecode.rs` and `vm.rs`):

- **High-Speed Pure Compute:** executes pure code ~70x faster than the tree-walking interpreter.
- **Deep Recursion Safety:** managed frame stack eliminates OS stack overflows on recursive algorithms.
- **Transparent Effect Delegation:** effectful expressions (`llm`, `tool`, `computer`, `workspace`) delegate seamlessly from the VM to the runtime evaluator via `DelegateCall` and `DelegateExpr`.
- **Default Engine:** the VM runs by default for `nudgec run` and standalone binaries. Set `NUDGE_VM=0` to fall back to the AST interpreter.

### Standalone Native Agents (Cargo-Free)

Package your agent into a single, self-contained native executable binary with zero build-site toolchain dependencies:

```sh
# Package an agent into a standalone binary
nudgec build --standalone my-agent.ndg -o bin/my-agent

# Run directly on the host (zero Python/Node/Rust dependencies)
./bin/my-agent
```

- **AST Stamping:** `nudgec` type-checks the source, serializes the AST bundle into bincode, and appends it to a pre-compiled `nudge-agent` runtime binary (`NDGAGNT1` footer) in milliseconds.
- **Two Runtime Flavors:**
  - **Full (~2.09MB, default):** live-capable runtime with embedded `ureq` + `rustls` HTTP/TLS stack for live LLMs and MCP servers.
  - **Replay-Only (~832KB):** compact runtime with network dependencies omitted, serving all operations from recorded NTF traces for $0 offline execution.
- **Runtime Discovery:** located via the `NUDGE_AGENT_RUNTIME` environment variable or as a sibling binary next to `nudgec`. Read the [Standalone Documentation](docs/standalone.md).

### WebAssembly Target

Deploy agents into browser sandboxes, edge runtimes (Cloudflare Workers, Fastly), and WASI environments:

- **Offline Replay (`nudge-replay-wasi`):** replay recorded NTF traces offline inside a ~526KB `.wasm` module under `wasmtime` with byte-identical output to native runs:
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **Live Tool Host Bridge:** agents in browser or edge JavaScript runtimes make live tool calls through the standardized `nudge_host_tool_call` import contract (verified under Node WASI, see `examples/wasm-host/`).
- **Sandboxed Security:** network sockets and OS process spawning are gated out on `wasm32`, ensuring sandboxed execution. Read the [WebAssembly Documentation](docs/wasm.md).

## Tooling & Integrations

### MCP Integration

Declare tools backed by Model Context Protocol (MCP) servers communicating over stdio:

```nudge
type SearchResult = { url: string, title: string }

tool web_search(query: string) -> [SearchResult] {
    impl: mcp("search").web_search(query)
    side_effects: none
}

fn main() -> [SearchResult] uses Tool {
    web_search("nudge language")
}
```

Configure live servers via `NUDGE_MCP_SERVERS`:

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

Parameters are passed as named JSON argument objects for compatibility with FastMCP and standard servers. Missing servers fail fast with clear diagnostics.

### Deterministic Traces & Agent CI

Every run emits an open NTF v1 JSONL trace capturing all effect interactions:

```sh
# Verify trace integrity against the open specification
nudgec trace-check trace.jsonl

# Compare two traces to identify cost or behavior changes
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression
```

**GitHub Action (`action.yml`):** test agents in CI on every push at $0 cost with zero API keys:

```yaml
name: agents
on: [push, pull_request]
jobs:
  test-agents:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: NekomyaDev/nudge@v1
        with:
          files: "agents/**/*.ndg"
          trace-diff: "baseline.jsonl candidate.jsonl"
```

The frozen NTF specification lives at [docs/ntf-spec.md](docs/ntf-spec.md), with a full test suite in [`conformance/`](conformance/) and bridges for LangChain, CrewAI, and LangGraph.

### Developer Tools

- **Local Trace Viewer:** `nudgec trace-view <trace.jsonl>` starts a local web UI with visual execution timelines, token expenditure, and repair highlights.
- **Run Explainer:** `nudgec explain <trace.jsonl>` produces a human-readable CLI summary highlighting failures, token consumption, and low-confidence decisions.
- **Formatter & Watcher:** `nudgec fmt <file.ndg>` safely formats code, and `nudgec check <file.ndg> --watch` type-checks on every save.
- **Static HTML Export:** `nudgec trace-html trace.jsonl -o report.html` exports self-contained interactive audit reports.

### VS Code Extension

Install the [Nudge Language Extension](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) for syntax highlighting, code snippets, hover documentation, and real-time type diagnostics powered by `nudgec lsp`.

## Privacy & Trace Redaction

AI agents frequently handle sensitive tokens, authorization headers, private keys, and user PII. Nudge protects your data with a fail-closed trace redaction pipeline:

- **Single Choke Point:** redaction occurs inside `TraceWriter::emit` before records are serialized to disk or memory buffers. Every record type (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) is masked at this single bottleneck.
- **Fail-Closed by Default:** trace redaction is active by default (`NUDGE_TRACE_REDACT=1`). Set `NUDGE_TRACE_REDACT=0` only for explicit local debugging.
- **`NUDGE_TRACE_REDACT=0` is a security boundary, not a convenience:** disabling redaction records secrets, API keys, authorization headers, and personal data in **PLAINTEXT** in the trace file. This is the documented, intended consequence of the off switch (asserted by tests) — never use it in CI, with shared/committed artifacts, or when an app built on Nudge consumes traces. Downstream consumers must treat the presence of `NUDGE_TRACE_REDACT=0` as a security-relevant flag and must NOT assume traces are safe to store or share in that mode.
- **Automatic Secret Detection:** automatically identifies and masks environment variables ending in `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT`, or containing `AUTH` (with values of 6 or more characters).
- **Layered Pattern Masking:** scrubs bearer tokens, authorization headers, key-value credential pairs, private key blocks, and vendor token prefixes (OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`).
- **Cryptographic Replay Invariance:** the content-addressed `request_hash` is computed on **pre-redaction** inputs. This guarantees that masked traces remain byte-identical during replay lookup with O(1) matching without persisting credentials to disk.

## Environment Variables

| Variable | Scope | Purpose | Default |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | both | Active LLM provider (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | native | Bytecode VM execution engine toggle (`1` = VM default, `0` = AST interpreter fallback) | `1` |
| `NUDGE_DEFAULT_MODEL` | native | Default model string when not explicitly specified in `with` block | Provider default |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | both | Credentials and base endpoint for OpenAI-compatible LLM providers | None |
| `NUDGE_PRICING` | both | Custom JSON pricing table: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | Built-in table |
| `NUDGE_GUARD` | python/TS | Output guards (`pii` masks secrets, emails, IP addresses in LLM outputs) | None |
| `NUDGE_BUDGET` | both | Run-wide USD spending ceiling enforced across parallel fan-outs | None |
| `NUDGE_REPAIR_BUDGET` | python | Cumulative USD ceiling for repair rounds | None |
| `NUDGE_MAX_LOOP_ITERATIONS` | native | Maximum sequential loop iterations before raising runtime error `E0703` | `10000` |
| `NUDGE_TRACE` | both | Destination path for writing the NTF JSONL execution trace | `trace.jsonl` |
| `NUDGE_REPLAY` | both | Trace path for deterministic offline replay (`NUDGE_REPLAY_MODE=all` or `llm`) | None |
| `NUDGE_RESUME` | both | Resume crashed execution from checkpoint using recorded trace prefix | None |
| `NUDGE_TRACE_REDACT` | native | Enable single choke point secret and PII masking in traces (`1` or `0`) | `1` (Active) |
| `NUDGE_TRACE_REDACT_ENV` | native | Automatically mask values of secret environment variables in traces | `1` (Active) |
| `NUDGE_TRACE_REDACT_KEYS` | native | Comma-separated list of additional JSON object keys to mask | None |
| `NUDGE_TRACE_REDACT_PATTERNS` | native | Comma-separated regex list or path to a regex file for custom trace masking | None |
| `NUDGE_WORKSPACE_DIR` | native | Root directory authority for sandboxed `workspace.*` operations | Current directory |
| `NUDGE_STORE_PATH` | native | Storage file location for persistent typed key-value data | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | native | Force UI window operations to run headlessly in memory (`1` or `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | both | Execution-layer JSON tool policy (e.g. `{"web_search": ["*"]}`). Fails closed | None |
| `NUDGE_PROCESS_GRANTS` | native | Fail-closed JSON execution policy for `proc.*` commands and arguments | None (denied) |
| `NUDGE_PROCESS_TIMEOUT` | native | Default child-process execution timeout in milliseconds for `proc.*` | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | native | Default polling and wait timeout in milliseconds for `fs.watch` | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | native | Opt-in toggle to activate the Antigravity provider (`1` or `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | native | Direct bearer token for Antigravity provider authentication | None |
| `NUDGE_AUTH_GITHUB_COPILOT` | native | Direct token override for GitHub Copilot provider authentication | None |
| `NUDGE_MCP_SERVERS` | both | JSON registry mapping MCP server names to stdio commands | None |
| `NUDGE_MCP_TIMEOUT_MS` | native | Per-request read deadline and hung-session timeout for MCP servers | `30000` |
| `NUDGE_DECISION_SERVERS` | both | JSON registry for decision model endpoints (e.g. `{"laya": {"base_url": "..."}}`) | None |
| `NUDGE_DECISION_CACHE` | both | Cache file path for persisted decision model distributions | None |
| `NUDGE_COMPUTER_SERVERS` | both | JSON registry for computer use bridges (subprocess JSONL or HTTP) | None |
| `NUDGE_COMPUTER_PROVIDER` | both | Selected bridge for computer actions (`fake`, or registered server name) | `fake` |
| `NUDGE_COMPUTER_KILL` | both | Hard kill switch: `1` immediately refuses all computer operations | `0` |
| `NUDGE_COMPUTER_DRIFT` | both | When `1`, re-observes live state during replay and attaches mechanical drift diffs | `0` |
| `NUDGE_AGENT_RUNTIME` | native | Path to pre-built `nudge-agent` binary for `nudgec build --standalone` | Sibling binary |
| `NUDGE_CREDENTIALS_PATH` | native | Custom path to Nudge authentication credentials store | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | python | OpenTelemetry span export file | None |
| `NUDGE_OTEL_ENDPOINT` | python | OpenTelemetry HTTP export endpoint | None |

`native` applies to the Rust runtime (`nudgec`/`nudge-runtime`), `python/TS` to the legacy generated runtimes, and `both` to both runtime families.

## Real-World Examples

Explore complete agent implementations in [`examples/`](examples/):

| Example | Domain | Highlights |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | Conversational Assistant | Typed memory schemas, deterministic replay, budget ceilings |
| [Triage Agent](examples/triage-agent/) | Support Classification | Batched `decide{}` blocks, confidence thresholds, routing policies |
| [Code Reviewer](examples/code-reviewer/) | Software Engineering | Structured syntax diagnostics, automated rubric scoring |
| [Research Agent](examples/research_agent.ndg) | Multi-Source Synthesis | Parallel retrieval with `par map`, confidence scoring |
| [Computer Automation](examples/computer-automation/) | Desktop Agent | Screen observation, accessibility trees, drift validation |
| [RAG Agent](examples/rag-agent/) | Retrieval-Augmented Generation | MCP stdio retrieval tool bindings, citation extraction |
| [Property Fuzzer](examples/property-fuzz/) | Agent Testing | `for_all` invariant testing, prompt injection fuzzing |
| [Dungeon Crawler](examples/dungeon-crawler/) | State Machine Simulation | Pure Nudge, agent state transitions, turn-based combat |

## License & Building from Source

Nudge is open source under the **Apache-2.0** license. See [LICENSE](LICENSE) for details.

### Building from Source

Building the compiler requires Rust 1.75+ (stable):

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # compiler binary at target/release/nudgec
cargo test --workspace            # compiler, VM, and runtime test suite
```

The Python runtime is located in `runtime/` (standard library only, zero external dependencies):

```sh
pip install ./runtime
```

---

<p align="center">
  Made with love by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
