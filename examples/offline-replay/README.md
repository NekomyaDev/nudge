# Offline Replay Example

This example demonstrates Nudge's core deterministic replay capability: record a tool call and an LLM call once, then replay the entire run deterministically with zero token costs ($0) and no network calls.

## Program: `research.ndg`

The program defines a web search tool backed by an MCP server and runs a search followed by an LLM synthesis:

```nudge
tool web(q: string) -> string {
    impl: mcp("search").web(q)
    side_effects: none
}

fn main() -> string uses LLM, Tool {
    let result = web("nudge lang")
    llm"""Summarize this search result: {result}"""
    with { model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}
```

## Record -> Replay Workflow

### 1. Record a Trace

During execution, Nudge records all tool calls, LLM interactions, and function returns into an NTF v1 trace file specified by `NUDGE_TRACE` (defaults to `trace.jsonl`).

To record a trace using the mock MCP server and native runner:

```sh
# Ensure the mock MCP server and nudgec are built
cargo build -p nudgec --bin nudgec --bin mock_mcp_nudgec

# Point the search MCP server to the mock binary
MOCK_BIN="$(pwd)/target/debug/mock_mcp_nudgec"
export NUDGE_MCP_SERVERS="{\"search\": {\"command\": \"$MOCK_BIN\"}}"
export NUDGE_PROVIDER=native
export NUDGE_TRACE=examples/offline-replay/trace.jsonl

# Run and record the trace
nudgec run examples/offline-replay/research.ndg
```

The execution produces `trace.jsonl` containing:
1. A `tool.call` record for `web("nudge lang")` with its inputs, output, and canonical `request_hash`.
2. An `llm.call` record with prompt, model, tokens, and `request_hash`.
3. An `fn.return` record for `main`.

You can inspect and validate the recorded trace:

```sh
nudgec trace-check examples/offline-replay/trace.jsonl
nudgec explain examples/offline-replay/trace.jsonl
```

### 2. Replay Offline ($0 Cost, No Network)

To replay the recorded trace deterministically:

```sh
NUDGE_REPLAY=examples/offline-replay/trace.jsonl nudgec run examples/offline-replay/research.ndg
```

When replaying:
- The runtime serves recorded tool results for matching tool calls. No MCP server process is spawned and no network requests are made.
- The runtime serves recorded LLM completions matching prompt and model hashes. No LLM provider is contacted and zero tokens are billed.
- If program logic diverges (e.g. prompt or tool arguments change), the runtime raises a `ReplayMismatch` error to prevent stale answers.

*Note on current runtime status:* LLM call replay is fully supported in `nudge-runtime`. Native `tool.call` replay is actively being hooked into the interpreter. Once complete, full end-to-end replay runs completely offline without external processes or network connectivity.
