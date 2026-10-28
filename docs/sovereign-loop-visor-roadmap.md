# RFC & Architectural Roadmap: Sovereign Loop & Watch-Only Visor HUD
# 独立自主闭环（Sovereign Loop）与无干扰全景监控架构路线图

> **Status:** Draft / Specification Phase (RFC-2026-SOVEREIGN-HUD)  
> **Authors:** NekomyaDev  
> **Scope:** `crates/nudge-runtime`, `crates/nudge-agent`, `crates/nudgec`, `prelude/`  
> **Target Version:** v1.5 - v2.0  
> **Design Principles:** Zero-Context Accumulation, Fire-and-Forget Autonomy, Deterministic Self-Healing, Non-Intrusive Watch-Only Interface  

---

## 1. Executive Summary & Problem Statement

Nudge provides a deterministic, effect-typed programming model designed for reliable agentic systems. However, current agent loops remain largely **session-bound and conversational**:
1. **Context Window Degradation:** Conventional agent frameworks accumulate chat history. After dozens of tool calls and hours of execution, LLM context windows saturate, costs surge exponentially, and instruction adherence degrades.
2. **Cooperative / Ephemeral Execution:** The existing runtime executes top-down script runs. Once an execution terminates, background scheduling, unhandled web drifts, and state transitions are halted without an active OS-level supervisor.
3. **In-Memory Volatility:** `MemStore` and `NVFS` currently operate with pure RAM backing ($O(1)$ in-memory lookups), leaving state, active schedules, and acquired tokens vulnerable to process restarts.
4. **Interactive Friction in UIs:** Traditional AI interfaces demand constant prompt-response turn-taking, confirmation dialogs, and manual window switching. A truly sovereign autonomous assistant must run autonomously for days while projecting a non-blocking, visual **Watch-Only Observer Interface (Visor HUD)**.

### The Objective
Transform Nudge into a **Sovereign Autonomous Runtime**:
- **Fire-and-Forget Input:** Users inject an intent once; the runtime breaks it down into a dynamic **Goal Directed Acyclic Graph (DAG)** and executes continuously without conversational bloat.
- **Zero-Context Accumulation:** Ephemeral context is purged at each execution tick. State is maintained strictly via verified goal states and persistent world trees (`NVFS`).
- **Self-Healing Replay (`par race`):** When DOM, API, or system layouts drift, the runtime automatically synthesizes and validates repair branches against deterministic trace contracts (`NTF`).
- **Watch-Only Ambient HUD:** A modern, menuless visual feedback environment projecting real-time orbital goal nodes, ephemeral viewports, and reactive telemetry without modal prompts.

---

## 2. System Reality & Baseline Audit

| Subsystem / Module | Repository Location | Current Baseline | Target Production Milestone |
| :--- | :--- | :--- | :--- |
| **Sovereign Kernel & Daemon** | `crates/nudge-runtime/src/task.rs`, `proc.rs`, `crates/nudge-agent` | **Non-Existent (Episodic)** | `sovereign.rs`: Continuous state-machine supervisor with zero-context tick evaluation and Goal DAG scheduling. |
| **Autonomous Timer Wheel** | `crates/nudge-runtime/src/task.rs` | **Limited to In-Task Wait** | `scheduler.rs`: Wall-clock cron, periodic intervals (`schedule.every`), and reactive event watchers (`schedule.on_change`). |
| **Persistent World State (NVFS)** | `mem_store.rs`, `stdlib/store.rs` | **Ephemeral RAM (0-Disk)** | Synchronous/Asynchronous WAL (`nvfs.wal`) + periodic bincode snapshotting (`nvfs.snap`) with zero read-path degradation. |
| **Self-Healing Engine** | `eval/interpreter.rs`, `vm.rs` | **Fails on Drift (`NUDGE_REPLAY`)** | `healer.rs`: Parallel candidate repair (`par race`) generating verified replay updates without human escalation. |
| **Hybrid Web Engine** | `stdlib/browser.rs`, `prelude/web` | **Pure Nudge Parser & DOM** | Tiered router: Fast native engine (Katman 1) + headless OS bridge / CDP fallback (Katman 2) for heavy dynamic SPAs. |
| **Native OS Integration** | `stdlib/computer.rs`, `stdlib/os.rs` | **Simulated Mock & Metadata** | Direct cross-platform input/capture primitives (`computer.*`) and guarded process lifecycle (`os.spawn`, `os.watch`). |
| **Visual Observer HUD** | `ui.rs`, `ui_window.rs`, `font_engine.rs` | **Software Framebuffer** | High-performance 60 FPS ambient window with live telemetry stream, orbital goal graphs, and auto-docking PiP viewports. |

---

## 3. Architectural Blueprint

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                        USER INTENT (Fire-and-Forget)                            │
│                  "Monitor inventory and compile weekly report"                  │
└────────────────────────────────────────┬────────────────────────────────────────┘
                                         │
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                 SOVEREIGN RUNTIME DAEMON (crates/nudge-runtime)                 │
│                                                                                 │
│   ┌──────────────────────────┐             ┌────────────────────────────────┐   │
│   │ SovereignLoop (State)    │             │ AutonomousScheduler            │   │
│   │ - Zero-Context Ticks     │◄───────────►│ - Wall-Clock Cron Engine       │   │
│   │ - Goal DAG Orchestrator  │             │ - File / Web Watcher Triggers  │   │
│   └────────────┬─────────────┘             └────────────────────────────────┘   │
│                │                                                                │
│                ▼                                                                │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │ Execution Engine & Self-Healing Pipeline                                │   │
│   │                                                                         │   │
│   │   [Deterministic NTF Replay] ──(Drift Detected)──► [healer.rs]         │   │
│   │                                                          │              │   │
│   │                                                    par race {           │   │
│   │                                                      Branch A: ARIA     │   │
│   │                                                      Branch B: Path     │   │
│   │                                                      Branch C: DOM Walk │   │
│   │                                                    }                    │   │
│   │                                                          │              │   │
│   │   [Updated Trace Recorded] ◄─────────────────────────────┘              │   │
│   └────────────┬────────────────────────────────────────────────────────────┘   │
│                │                                                                │
│                ▼                                                                │
│   ┌──────────────────────────┐             ┌────────────────────────────────┐   │
│   │ Persistent Memory (NVFS) │             │ Native IO Subsystems           │   │
│   │ - WAL (`nvfs.wal`)       │             │ - Tiered Browser (ureq + CDP)  │   │
│   │ - Snapshot (`nvfs.snap`) │             │ - Real OS Desktop (`computer`) │   │
│   └────────────┬─────────────┘             └────────────────────────────────┘   │
└────────────────┼────────────────────────────────────────────────────────────────┘
                 │
                 │ IPC / Live Framebuffer Telemetry (60 FPS)
                 ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    WATCH-ONLY VISOR HUD (Zero-Menu Sci-Fi HUD)                 │
│                                                                                 │
│  [Top Status & Telemetry] Uptime | Active DAGs | Replay Ratio | Cost/Confidence │
│                                                                                 │
│         ┌───────────────┐     ┌──────────────────────┐     ┌───────────────┐    │
│         │ PiP Viewport  │     │    Orbital Core      │     │ PiP Viewport  │    │
│         │ (Live Web)    │     │  (Reactive Sovereign │     │ (Desktop OS)  │    │
│         │ Auto-Docking  │     │     Loop State)      │     │ Auto-Docking  │    │
│         └───────────────┘     └──────────────────────┘     └───────────────┘    │
│                                                                                 │
│  [Bottom Log Stream] Live Thought Subtitles & Continuous Verification Pipeline   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Phase Breakdown & Implementation Tracks

### Phase 1: Core Daemon & Self-Scheduling Architecture

#### Track 1.1: Sovereign Consciousness Loop (`crates/nudge-runtime/src/sovereign.rs`)
- **State Machine Topology:**
  ```rust
  pub enum SovereignState {
      Idle { next_wakeup: Instant },
      EvaluatingGoal { goal_id: GoalId },
      ExecutingPlan { task_id: TaskId, trace_ref: Option<TraceId> },
      SelfHealing { failure_reason: DriftError },
      Snapshotting,
  }
  ```
- **Zero-Context Memory Contract:**
  - Conversations are non-cumulative. Each execution step receives exclusively:
    1. Active `GoalNode` metadata and constraints.
    2. Input payload from previous topological dependencies.
    3. Read-only structured world-state slice from `NVFS`.
  - Upon step completion, the LLM prompt/response context is discarded; only typed outputs and state mutations are written to the persistent store.
- **Goal DAG Engine:**
  - Dynamic decomposition of high-level goals into directed dependency graphs:
    ```rust
    pub struct GoalNode {
        pub id: GoalId,
        pub parent_id: Option<GoalId>,
        pub intent: String,
        pub schedule: ScheduleSpec,
        pub dependencies: Vec<GoalId>,
        pub retry_budget: u32,
        pub status: GoalStatus,
    }
    ```

#### Track 1.2: Autonomous Scheduler (`crates/nudge-runtime/src/scheduler.rs`)
- **Timer Wheel & Reactive Triggers:**
  - High-resolution timing wheel supporting millisecond to multi-day intervals.
  - Integration with file-system notifications (`notify` / kernel kqueue/epoll) and HTTP `ETag` / `Last-Modified` pollers.
- **Language Primitives (`prelude/schedule.ndg`):**
  ```nudge
  schedule.every("4h", check_stock_levels, [sku_list])
  schedule.after("15m", verify_shipment_status, [order_id])
  schedule.on_change("/var/log/syslog", triage_system_event)
  schedule.on_dom_change("https://status.example.com", "#incident-banner", alert_ops)
  ```
- **Dynamic Hot-Reloading:**
  - Runtime code synthesis: the sovereign agent can emit standalone `.ndg` modules, validate them through `nudgec::check`, and dynamically register tasks into the running scheduler without restarting the process.

#### Track 1.3: Deterministic Self-Healing Replay (`crates/nudge-runtime/src/healer.rs`)
- **Continuous Replay Verification:**
  - Tasks execute in **0-Token Replay Mode** by default via recorded `NTF` traces.
  - When target web or system environments mutate, a `TraceDriftError` triggers the autonomous healer.
- **Multi-Branch Resolution (`par race`):**
  - Concurrently evaluates repair strategies:
    1. *Structural Recovery:* Sibling and parent ARIA tree exploration.
    2. *Semantic Recovery:* Re-running structured prompt with visual/DOM snippet.
    3. *API Bypass:* Querying alternative data sources or local state cache.
  - The winning branch updates the `NTF` baseline trace automatically; execution resumes uninterrupted.

---

### Phase 2: Persistence, Native IO & Cross-Platform Bridges

#### Track 2.1: Persistent NVFS & WAL Engine (`crates/nudge-runtime/src/mem_store.rs`)
- **Zero-Latency In-Memory Lookups with Crash Durability:**
  - Retain existing lock-free / low-overhead in-memory indexes for standard queries.
  - Append-only Write-Ahead Log (`~/.nudge/state/nvfs.wal`) written synchronously or with bounded micro-batching ($< 5\text{ms}$).
  - Compacted binary snapshots (`~/.nudge/state/nvfs.snap`) generated periodically or on graceful pause.
- **Automated Cold-Start Recovery:**
  - On restart, `nudge-agent` restores goal hierarchies, active timers, session cookies, and trace manifests directly into RAM in $< 100\text{ms}$.

#### Track 2.2: Tiered Browser Router (`crates/nudge-runtime/src/stdlib/browser.rs`)
- **Katman 1 (Native Engine - Default):**
  - Pure Nudge DOM parser, flexbox layout, and software rasterizer backed by lightweight HTTP/TLS (`ureq` + `rustls`).
  - Capable of processing dozens of static/SSR pages per second at zero external dependency cost.
- **Katman 2 (Headless CDP / WebView Fallback):**
  - Seamless escalation for heavy client-side SPAs (React, Angular, Cloudflare Turnstile).
  - Drives platform-native web rendering processes (WebView2 on Windows, WKWebView on macOS, WebKitGTK / Headless Chromium CDP on Linux).
  - Unifies DOM inspection and rasterization outputs into the standard Nudge `Element` and `RasterBuffer` structs.
- **RFC 6265 Cookie Continuity:**
  - Persistent cookie jar stored directly inside NVFS partition `/state/cookies.json` to maintain authenticated sessions across weeks of autonomous operation.

#### Track 2.3: Native OS & Desktop Automation (`crates/nudge-runtime/src/stdlib/computer.rs`, `os.rs`)
- **Direct OS Subsystem Drivers:**
  - Replaces synthetic mock execution with real OS API bindings:
    - Screen Capture: Platform-specific frame grabbers (Direct3D Desktop Duplication on Windows, CoreGraphics / ScreenCaptureKit on macOS, PipeWire / X11 XShm on Linux).
    - Input Injection: Native OS event generation for mouse and keyboard without external Python or helper tools.
- **Confinement & Governance:**
  - Execution strictly bounded by `NUDGE_TOOL_GRANTS` and capability envelopes to prevent unconstrained system tampering.

---

### Phase 3: Watch-Only Visor HUD (Non-Intrusive Visual Interface)

#### Track 3.1: Central Consciousness Core & Orbital Goal Graph
- **Orbital State Dynamics:**
  - Central pulsating node visually communicating current loop status:
    - *Azure Steady:* Deterministic Replay (0-Token execution).
    - *Amber Pulse:* Active Reasoning & Goal Planning (`llm"""` synthesis).
    - *Violet Double-Helix:* Self-Healing in progress (`par race` reconciliation).
  - Surrounding orbital rings represent active and scheduled goals. Worker tasks expand outward during execution and collapse back into the core upon completion.

#### Track 3.2: Autonomous Docking Viewports (Picture-in-Picture)
- **Zero-Click Observability:**
  - As the agent triggers `browser.observe` or `computer.observe`, corner viewports smoothly slide into view.
  - Displays live screen / web canvas rasterizations overlaid with targeting reticles showing ARIA nodes, focused buttons, and text fields.
  - Viewports auto-retract once the step completes; no window management or closing clicks required from the user.

#### Track 3.3: Ambient Telemetry & Live Consciousness Feed
- **Streaming Subtitle Stream:**
  - Continuous single-line telemetry ticker at the screen base translating internal states into human-readable operations:
    - *"Re-evaluating Goal #12... Target site updated; synthesized new selector; trace #94 verified."*
- **Key Real-Time Indicators:**
  - Active autonomous tasks, system uptime, replay economy ratio (e.g. `96.4% $0-cost deterministic`), and confidence gauge.

#### Track 3.4: Fire-and-Forget Intent Overlay
- **Modal-Free Command Injection:**
  - No permanent chat history box. Pressing a global activation key (e.g. `Space` or `Super+N`) opens a translucent, minimalist input line.
  - Submitting an intent immediately converts it into a top-level DAG node and dismisses the input. The HUD seamlessly returns to passive observation mode.

---

## 5. Engineering Roadmap & Milestone Schedule

```
Week 1: Persistence & Daemon Foundations
├── Day 1-2: NVFS Disk Durability (WAL + Bincode Snapshotting)
├── Day 3-4: SovereignLoop State Machine & Zero-Context Tick Cycle
└── Day 5:   AutonomousScheduler (Timer Wheel & Wall-Clock Cron)

Week 2: Self-Healing & Hybrid IO Integration
├── Day 6-7: Deterministic Healer Engine (`healer.rs` + `par race`)
├── Day 8-9: Tiered Browser Router (Native Katman 1 + Headless CDP Katman 2)
└── Day 10:  Real OS Computer & Process Primitives (`computer.*`, `os.spawn`)

Week 3: Visor HUD & Autonomous Release
├── Day 11-12: High-Performance HUD Window & WebSocket/WASM Telemetry Bus
├── Day 13:   Orbital Visualizer, Auto-Docking PiP Viewports & Ambient Subtitles
└── Day 14:   Standalone Binary Unification (`nudgec build --standalone`) & 48h Stress Test
```

### Verification Criteria & Quality Gates

1. **Gate 1 (Persistence Durability):**
   - Inject 1,000 goals and schedules into `mem_store`.
   - Issue `SIGKILL` to `nudge-agent`. Restart process.
   - 100% of goal hierarchy, schedules, and active traces must restore in $< 100\text{ms}$ with zero corrupt records.
2. **Gate 2 (Zero-Context Stability):**
   - Execute a continuous synthetic loop for 24 wall-clock hours consisting of 2,500 sequential goal evaluations.
   - Peak RSS memory footprint must remain flat ($< 65\text{MB}$ delta).
   - Context window per LLM call must remain strictly bounded ($O(1)$ relative to total execution duration).
3. **Gate 3 (Self-Healing Autonomous Drift):**
   - Alter DOM tree IDs and CSS classes on test web targets.
   - System must reconcile and update trace contracts within $\le 3$ repair attempts without human input.
4. **Gate 4 (HUD Frame Budget):**
   - HUD rendering must maintain a consistent 60 FPS update cycle under active dual PiP rasterization stream without blocking sovereign scheduling threads.
