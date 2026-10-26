# RFC & Architectural Roadmap: Pure-Nudge Self-Hosted In-Memory Indexing Infrastructure

> **Document ID:** RFC-NDG-2026-04  
> **Status:** Official Architectural Specification & Implementation Roadmap  
> **Authors:** Nudge Core Architecture Team  
> **Default Language:** English (with formal technical terminology)  
> **Target Version:** Nudge v2.1 – v3.0  
> **Primary Objective:** Eliminate disk I/O bottlenecks, achieve $\ge 20\times$ faster execution (targeting $100\times - 300\times$), eliminate all external language/syscall dependencies ($0$ foreign dependencies), and establish the self-hosted Nudge Virtual File System (NVFS).

---

## 1. Executive Summary & Problem Formulation

In modern AI agent runtimes, code intelligence, symbol resolution, and workspace navigation represent the primary performance bottlenecks. Current implementations in `nudgec` and `nudge-runtime` suffer from three structural constraints:

1. **Repetitive Disk Syscall Overhead:** Every invocation of `workspace.read(path)` or `io.read(path)` executes OS kernel transitions (`openat`, `fstat`, `read`, `close`). For projects containing hundreds of files or during iterative agent feedback loops, disk I/O latency accounts for over 85% of total task wall-clock time.
2. **Synchronous Disk Fsync Penalties:** The standard persistence effect (`Store`, via `store.set`) serializes state to `.nudge/store.json` and invokes synchronous `fsync` operations on every write, producing artificial 1.5ms – 15ms latency spikes.
3. **Foreign Substrate Dependency:** The compiler and runtime rely on Rust and host OS abstractions. The long-term vision requires Nudge to be fully self-hosting, where code analysis, state management, and file systems are executed in pure Nudge (`.ndg`) without external language boundaries.

### The Pure-Nudge Paradigm Shift
This specification establishes a **pure-Nudge in-memory indexing infrastructure and virtual file system (NVFS)**. By elevating file structures, AST tokenization, semantic diffing, and dependency graphs into durable in-memory Nudge representations, disk I/O is reduced to zero during continuous operation.

---

## 2. Quantitative Performance Thesis: Memory vs. Disk I/O

The requirement mandates at least a $20\times$ speedup over disk I/O. The mathematical latency verification is derived below:

### Latency Profile Comparison

| Operation Phase | Operating System Disk / VFS Path | Pure-Nudge In-Memory (NBC VM) | Theoretical Speedup |
| :--- | :--- | :--- | :--- |
| **System Call Context Switch** | $1.5\,\mu\text{s} - 3.0\,\mu\text{s}$ (`openat`) | $0\,\mu\text{s}$ (Zero syscalls) | $\infty$ |
| **Inode & Path Resolution** | $1.2\,\mu\text{s} - 2.5\,\mu\text{s}$ (Directory traversal) | $0.04\,\mu\text{s}$ ($40\,\text{ns}$, VM Slot lookup) | $\mathbf{\sim 40\times}$ |
| **Data Fetch (Page Cache / NVMe)**| $8.0\,\mu\text{s} - 45.0\,\mu\text{s}$ (`read()` buffer copy) | $0.08\,\mu\text{s}$ ($80\,\text{ns}$, In-memory pointer) | $\mathbf{\sim 100\times - 500\times}$ |
| **Parsing & UTF-8 Validation** | $5.0\,\mu\text{s} - 20.0\,\mu\text{s}$ (Per-read decode) | $0\,\mu\text{s}$ (Pre-parsed AST in memory) | $\mathbf{\sim 200\times}$ |
| **Symbol Table Resolution** | $15.0\,\mu\text{s} - 120.0\,\mu\text{s}$ (Linear file grep) | $0.15\,\mu\text{s}$ ($150\,\text{ns}$, Hash bucket index) | $\mathbf{\sim 100\times - 800\times}$ |
| **Total Query Latency** | **$30.7\,\mu\text{s} - 190.5\,\mu\text{s}$** | **$0.27\,\mu\text{s}$ ($270\,\text{ns}$)** | $\mathbf{\ge 113.7\times}$ |

$$\text{Speedup Factor} = \frac{T_{\text{disk}}}{T_{\text{memory}}} \ge \frac{30.7\,\mu\text{s}}{0.27\,\mu\text{s}} \approx \mathbf{113.7\times}$$

Even under optimal OS page-cache conditions, memory-resident lookup in Nudge bytecode guarantees between **$50\times$ and $300\times$** acceleration, decisively surpassing the $20\times$ threshold.

---

## 3. Language Substrate & Native Primitives (The Missing Foundation)

To allow pure Nudge code to implement high-performance indexing without falling back to Rust or C, the Nudge language core must be extended with four native primitives:

### 3.1 Generic `Map[K, V]` & In-Place Mutation (`IndexAssign`)
* **Current Limitation:** `ast.rs` and `check.rs` only admit fixed record types (`{ k: T }`) and specific generics (`Task[T]`, `Unverified[T]`, etc.). Furthermore, statements only support variable assignment (`name = val`), lacking index-level mutation (`arr[i] = val`).
* **Specification:**
  1. Add `TypeExpr::Generic("Map", [K, V])` to `check.rs` supporting arbitrary hashable keys ($O(1)$ amortized lookup).
  2. Add `StmtKind::IndexAssign { collection: Expr, index: Expr, value: Expr }` to the AST and `Op::StoreIndex` to the NBC bytecode engine.

### 3.2 Ephemeral In-Memory Effect (`uses MemStore`)
* **Current Limitation:** `Store` effect flushes atomically to `.nudge/store.json` via `fsync`.
* **Specification:**
  - Introduce `MemStore` as the 13th native Nudge effect.
  - Expose `mem.get(key, default)`, `mem.set(key, val)`, `mem.has(key)`, `mem.delete(key)`, and `mem.keys(prefix)`.
  - State lives strictly within the process heap, eliminating all disk interaction.

### 3.3 Hardware-Grade Byte & Hash Primitives
* **Current Limitation:** Computing hashes requires manual iteration over character subsets with `str.substring`.
* **Specification:**
  - `str.char_code(s: string, idx: int) -> int`: Returns numeric ASCII/Unicode code point in $O(1)$.
  - `str.hash(s: string) -> int`: Produces SIMD-accelerated 64-bit FNV-1a / xxHash values in a single VM cycle.

### 3.4 Inode Status Primitive (`workspace.stat`)
* **Current Limitation:** Determining whether a file has changed requires full content loading via `workspace.read`.
* **Specification:**
  - `workspace.stat(path: string) -> { size: int, mtime_ms: int, is_dir: bool }`: Returns OS/NVFS metadata directly.

---

## 4. Architecture Pillar I: Nudge Virtual File System (NVFS)

NVFS provides a pure-Nudge abstraction layer that supersedes raw disk access, serving as the foundation for autonomous execution.

```
┌─────────────────────────────────────────────────────────────────┐
│                   NUDGE VIRTUAL FILE SYSTEM (NVFS)              │
│                                                                 │
│  ┌──────────────────────┐              ┌─────────────────────┐  │
│  │   Inode Registry     │              │  Content-Addressable│  │
│  │  - inode_id          │              │  Storage (CAS)      │  │
│  │  - path / hierarchy  │              │  - SHA/FNV Hash Key │  │
│  │  - mtime / size      │              │  - Deduplicated     │  │
│  │  - block pointers    │              │    Memory Chunks    │  │
│  └──────────┬───────────┘              └──────────┬──────────┘  │
│             │                                     │             │
│             └──────────────────┬──────────────────┘             │
│                                │                                │
│                                ▼                                │
│                   ┌─────────────────────────┐                   │
│                   │   In-Memory WAL Journal │                   │
│                   │  - Atomic Transactions  │                   │
│                   │  - Zero Fsync Overhead  │                   │
│                   └─────────────────────────┘                   │
└────────────────────────────────┬────────────────────────────────┘
                                 │ Intercepts workspace.read & io.read
                                 ▼
                     [ Pure-Nudge Applications ]
```

### 4.1 Inode & Directory Hierarchy
Every file within the workspace is represented as a pure-Nudge record:
```nudge
type Inode = {
    id: int,
    path: string,
    is_dir: bool,
    size: int,
    mtime_ms: int,
    content_hash: int,
    block_ids: [int]
}
```

### 4.2 Content-Addressable Storage (CAS)
File bodies are stored in deduplicated memory chunks. If multiple files share common boilerplates, imports, or schemas, blocks are stored once in memory and referenced via immutable hashes, drastically reducing the workspace RAM footprint.

---

## 5. Architecture Pillar II: Full-Spectrum AST Semantic Model

A robust indexer must capture every construct supported by Nudge, encompassing all 12 native effects (`IO`, `FsWatch`, `Store`, `Workspace`, `WorkspaceDelete`, `Process`, `Tool`, `UI`, `Auth`, `Decision`, `Computer`, `LLM`).

### 5.1 Exhaustive Entity Representation
The indexer model encompasses:

1. **Functions (`fn`):** Parameters, return types, declared effects (`uses ...`), cost boundaries (`costs < X USD`), and repair retry configurations (`retry N with repair`).
2. **Agents (`agent`):** Durable state declarations (`state { ... }`), type invariants, default expressions, state mutators (`state.x += v`), and recovery boundaries.
3. **External Tools (`tool`):** MCP server implementations (`impl: mcp("name").method`), side effect declarations (`side_effects: none | write`), and execution timeouts.
4. **Type Definitions & Refinements (`type`):**
   - Structural Records (`{ k: T }`) and Type Aliases.
   - Refinement Constraints: `@range(min, max)`, `@format(regex|url|email)`.
   - Verification Stages: `Unverified[T]`, `Validated[T]`, `Verified[T]`.
   - Confidence Lattices: `Confidence[T]`.
5. **Local UI Architecture (`view` & `window` - Pack 3):**
   - Layout hierarchies (`ui.column`, `ui.tabs`, `ui.code_block`, `ui.status_line`).
   - Reactive event handlers enforcing `uses UI`.
6. **Decision Models (`decide` - JEV Family):**
   - Question blocks (`choice`, `noul`, `score`), confidence gates, and latency constraints.
7. **Computer Automation (`computer` - v1.5):**
   - Visual drift tracking, screen observation checkpoints, and OS input targets.
8. **Provider Authentication (`auth`):**
   - `auth provider` and `account` bindings with secret token containment.
9. **Property-Based Verification (`for_all`, `test`):**
   - Generative testing loops (`gen.int`), formal postconditions, and invariant assertions.

---

## 6. Architecture Pillar III: Reactive Semantic Diff & Blast Radius Engine

Detecting file modification must not trigger full workspace re-indexing. The diff engine categorizes changes semantically to calculate precise downstream blast radius.

```
                    [ File Ingestion / Modification ]
                                   │
                                   ▼
                   [ Lexer & AST Boundary Scanner ]
                   - Tokenizes source into blocks
                   - Protects llm""" multi-line prompts
                   - Extracts sig_hash and body_hash
                                   │
                                   ▼
                   [ 4-Tier Semantic Diff Engine ]
       ┌───────────────────────────┼───────────────────────────┐
       ▼                           ▼                           ▼
[SIGNATURE_MUTATED]         [BODY_MUTATED]           [EFFECT_ESCALATED]
  Interface Broken            Internal Logic           New Privilege
  - Params changed            - Body hash changed      - Added WorkspaceDelete
  - Return type altered       - Signature identical    - Added IO / Tool
       │                           │                           │
       ▼                           ▼                           ▼
 Trigger Blast Radius        Silent In-Memory         Trigger Security
 Downstream Re-check         Replacement Only         Audit Alert
```

### 6.1 Two-Tier Hash Slicing
* **Signature Hash (`sig_hash`):** Computed over the declaration line, parameters, return type, effects, and cost bounds.
* **Body Hash (`body_hash`):** Computed strictly over the implementation statements inside `{ ... }`.

### 6.2 Blast Radius Analysis & Call-Graph Invalidation
The indexer maintains a bidirectional dependency graph:
* **Forward Map:** $\text{Entity} \to [\text{Callees}, \text{TypeDependencies}, \text{InvokedTools}]$
* **Reverse Map:** $\text{Symbol} \to [\text{DependentCallers}, \text{DependentViews}, \text{ImplementingAgents}]$

When a `SIGNATURE_MUTATED` or `REMOVED` event occurs, the engine performs a reverse graph traversal in memory ($<500\,\text{ns}$) to pinpoint every impacted call-site across all files, preventing cascaded runtime faults.

---

## 7. Architecture Pillar IV: Event Streaming & Buffer Architecture

### 7.1 Event Coalescing & Sliding-Window Debounce
Modern code editors perform atomic saves by creating hidden temporary files, deleting existing targets, and renaming buffers. Raw `fs.watch` events produce multiple noisy signals per save.
* NVFS incorporates a 50ms sliding-window debouncing filter.
* Ephemeral atomic events (`.tmp`, `~`, swap files) are coalesced into a single canonical `FILE_MUTATED` action before reaching the parser.

### 7.2 Multi-Core Cold-Start Ingestion (`par map`)
Upon initialization, workspace discovery executes concurrently using Nudge's native parallel evaluation:
```nudge
let payloads = par map(workspace_files, concurrency = 8) |file_path| -> {
    let raw_text = workspace.read(file_path)
    parse_ast_blocks(file_path, raw_text)
}
```
All parsed records are subsequently merged into the central state using CRDT-style join semantics (`| merge`).

---

## 8. Modular Implementation Structure

The entire system is partitioned into 10 decoupled pure-Nudge modules (`.ndg`):

```
crates/nudge-index/
├── nvfs/
│   ├── inode.ndg              # Inode records, virtual file descriptors
│   ├── cas_store.ndg          # Content-addressable memory storage & deduplication
│   └── journal.ndg            # In-memory transaction journal & rollback
├── lexer/
│   ├── tokens.ndg             # Keyword tokens, operators, delimiters
│   └── scanner.ndg            # State machine with string & prompt shielding
├── parser/
│   ├── ast_nodes.ndg          # Full AST representation covering all 12 effects
│   └── structural_parser.ndg  # Scope-aware recursive-descent block analyzer
├── indexer/
│   ├── symbol_table.ndg       # Partitioned hash buckets for O(1) symbol lookup
│   ├── call_graph.ndg         # Bidirectional caller/callee matrix & blast radius
│   ├── inverted_index.ndg     # Substring, token, and reference posting lists
│   └── diff_engine.ndg        # 4-tier semantic diff and structural comparator
└── api/
    └── query.ndg              # Zero-disk developer and agent query API
```

---

## 9. Formal Acceptance Gates

The implementation must strictly satisfy five quality gates before production release:

| Gate Identifier | Validation Criterion | Verification Methodology | Target Metric |
| :--- | :--- | :--- | :--- |
| **GATE-01** | **Foreign Language Independence** | Source code audit (`find . -not -name "*.ndg"`) | **$100\%$ Pure Nudge ($0$ foreign code)** |
| **GATE-02** | **Zero Continuous Disk I/O** | Kernel syscall profiling (`strace -e openat,read`) | **$0$ disk syscalls after cold start** |
| **GATE-03** | **Performance Acceleration** | Microbenchmarks comparing disk vs NVFS lookup | **$\ge 20\times$ (Target: $50\times - 300\times$)** |
| **GATE-04** | **Semantic Diff Fidelity** | Synthetic AST mutation test suite (1,000 cases) | **$100\%$ precision on signature vs body diff** |
| **GATE-05** | **Memory Consumption Ceiling** | Memory heap profiling across 50,000 LOC codebase | **$\le 25\,\text{MB}$ peak resident memory** |

---

## 10. Phased Implementation Roadmap

### Phase 1: Core Substrate Extensions (v2.1) [COMPLETED]
- [x] Add `Map[K, V]` generic resolution to `check.rs` and AST.
- [x] Implement `Op::StoreIndex` in `vm.rs` and `IndexAssign` syntax in `parser.rs`.
- [x] Introduce `uses MemStore` effect and ephemeral runtime backing.
- [x] Register `str.char_code` and `str.hash` standard library builtins.

### Phase 2: NVFS Foundation (v2.2) [COMPLETED]
- [x] Implement pure-Nudge `inode.ndg` and virtual file handle table.
- [x] Build content-addressable storage (`cas_store.ndg`) with memory deduplication.
- [x] Intercept `workspace.read` calls to route through NVFS buffers (`mod.ndg`).
- [x] Complete end-to-end acceptance test suite and microbenchmarks (`test_nvfs.ndg`).

### Phase 3: Lexer & Full-Spectrum AST Parser (v2.3)
- [ ] Develop pure-Nudge state-machine tokenizer (`scanner.ndg`) with prompt protection.
- [ ] Implement structural block parser supporting all 12 effects, UI views, and contract bounds.
- [ ] Validate AST generation against full language test suite (`docs/learn/*.ndg`).

### Phase 4: Reactive Diff & Blast Radius Engine (v2.4)
- [ ] Deploy two-tier hashing (`sig_hash` vs `body_hash`).
- [ ] Construct bidirectional call-graph matrix (`call_graph.ndg`).
- [ ] Implement 4-tier semantic diff categorization and automated caller invalidation.

### Phase 5: Query Engine & Production Benchmarking (v2.5)
- [ ] Finalize high-level query API (`api/query.ndg`).
- [ ] Conduct end-to-end benchmark comparisons proving $\ge 20\times$ speedup over disk I/O.
- [ ] Publish comprehensive test reports and formal certification.
