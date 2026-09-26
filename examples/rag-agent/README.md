# RAG Agent

A retrieval-augmented generation agent: it retrieves context from a knowledge
base over MCP, answers **only** from that context, and refuses when the
context is insufficient.

## Features

- MCP tool retrieval (`retrieve`) with a server registry
- Grounded answers with per-claim citations
- Explicit refusal: `grounded=false` + `missing` list when evidence is thin
- Budget-controlled synthesis with automatic repair
- Replay-tested against a recorded trace (fake provider, no API key)

## Code

```nudge
type Chunk = { doc: string, text: string, score: float @range(0, 1) }
type Answer = { answer: string, citations: [string], grounded: bool, missing: [string] }

tool retrieve(query: string, k: int) -> [Chunk] {
    impl: mcp("kb").retrieve(query)
    side_effects: none
}

fn answer_question(question: string, k: int) -> Answer uses LLM, Tool {
    let chunks = retrieve(question, k)
    llm"""Answer the question using ONLY the retrieved context.

    Question: {question}

    Retrieved context (doc, text, relevance):
    {chunks}

    Rules:
    - Cite the source doc for every claim
    - If the context is insufficient, set grounded=false and list
      what information is missing
    - Never invent facts that are not in the context"""
    with { schema: Answer, model: "anthropic:sonnet-4.6", budget: 0.02 USD, retry: 2 with repair }
}

fn main() -> Answer uses LLM, Tool {
    answer_question("How does Nudge prevent silent cost overruns?", 4)
}
```

## Run

```sh
pip install nudge-runtime
export NUDGE_MCP_SERVERS='{"kb": {"command": "your-kb-mcp-server"}}'
nudgec check rag-agent.ndg && nudgec build rag-agent.ndg
python3 out/rag-agent.py          # live: hits the real MCP server

# deterministic test against the recorded trace (no server, no key)
nudgec test rag-agent.ndg
```

## Notes

- Swap `mcp("kb")` for any MCP server that exposes a `retrieve`-style tool —
  arguments arrive as a named `arguments` object, so FastMCP servers work
  out of the box.
- The committed `traces/rag.jsonl` was recorded with the fake provider and
  an empty `kb` registry (retrieval returns `[]`), which is exactly the
  "context is insufficient" branch the schema models.
