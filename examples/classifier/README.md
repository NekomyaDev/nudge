# Support Ticket Classifier

Classifies support tickets with a **cost-aware model route**: short tickets
go to a cheap model, everything else goes to a strong one — chosen per call
with `route{}`, evaluated at compile time for cost ranges.

## Features

- Typed classification (category / priority / summary) with repair
- `route{}` model routing: `simple` vs `strong` per ticket length
- `par map` over a batch of tickets with per-lane branch labels in the trace
- Replay-tested trace (fake provider, no API key)

## Code

```nudge
type Ticket = { category: string, priority: string, summary: string }

fn triage(body: string) -> Ticket uses LLM {
    llm"""Classify this support ticket.
    ..."""
    with {
        schema: Ticket,
        model: route{
            simple: "openai:gpt-4.5-mini" when len(body) < 200,
            strong: "anthropic:sonnet-4.6" otherwise,
        },
        budget: 0.01 USD,
        retry: 2 with repair,
    }
}

fn main() -> [Ticket] uses LLM {
    par map(tickets) |(t)| -> triage(t)
}
```

## Run

```sh
pip install nudge-runtime
export NUDGE_API_KEY=...   # or NUDGE_PROVIDER=fake for the deterministic run
nudgec check classifier.ndg && nudgec build classifier.ndg
python3 out/classifier.py
nudgec test classifier.ndg
```

## Notes

- `route{}` arms evaluate top-down; the winning label is recorded as the
  additive `route` field on the llm.call record — run `nudgec trace-view`
  on `traces/classifier.jsonl` to see it.
- Batch size and ticket bodies are fixed in `main` for v1.2.x (programs
  take no argv; external data enters through MCP tools).
