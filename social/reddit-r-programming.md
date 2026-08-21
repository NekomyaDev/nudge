# Reddit Post - r/programming

## Title
I built a typed, replayable, budget-aware programming language for LLM agents

## Body

Hey r/programming!

I've been working on Nudge, a new programming language designed specifically for LLM agents. Here's the problem it solves:

**The current state of LLM agents:**
- Untyped output (parse JSON at runtime, hope for the best)
- No regression testing (what worked yesterday might not work today)
- Cost surprises (no budget control)
- Hidden side effects (no idea what the agent actually did)

**What Nudge does differently:**

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25
}
```

**Key features:**
- Schema is a type — proven at compile time
- Every run emits a trace; every trace is a test
- Budget is a contract, enforced by compiler + runtime
- `par map/race/all` with compile-time race safety
- Compiles to Python & TypeScript

**Zero dependencies** — the compiler is a single Rust binary that builds in seconds.

GitHub: https://github.com/NekomyaDev/nudge

Would love your feedback!
