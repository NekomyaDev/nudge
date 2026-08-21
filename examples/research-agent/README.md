# Research Agent

An AI agent that researches topics and produces structured findings.

## Features

- Multi-source research
- Structured findings with confidence scores
- Source verification
- Budget-controlled research
- Parallel research (par map)

## Code

```nudge
type Finding = { claim: string, source: string, confidence: float @range(0, 1) }
type Research = { topic: string, findings: [Finding], summary: string, gaps: [string] }

fn research_topic(topic: string, sources: [string]) -> Research uses LLM {
    llm"""Research the topic: {topic}

    Available sources:
    {sources}

    Extract verifiable findings with:
    - Clear claims
    - Source references
    - Confidence scores (0-1)

    Identify research gaps and summarize findings."""
    with { schema: Research, model: "anthropic:sonnet-4.6", budget: 0.05 USD, retry: 2 with repair }
}

fn main() -> Research uses LLM {
    let sources = [
        "Nudge is a typed programming language for LLM agents",
        "It compiles to Python and TypeScript",
        "Features deterministic replay and budget contracts"
    ]
    research_topic("Nudge programming language", sources)
}

test "research produces structured findings" {
    let t = replay("traces/research.jsonl")
    assert len(t.output.findings) >= 2
    assert t.output.summary != ""
    assert t.cost_usd < 0.10
}
```

## Run

```sh
nudgec check research-agent.ndg
nudgec build research-agent.ndg
python3 out/research-agent.py
```

## How it works

1. Takes topic and sources
2. Extracts findings with confidence scores
3. Identifies research gaps
4. Returns structured research report
5. Budget enforced per-call ($0.05 USD)
