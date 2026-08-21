# Twitter/X Thread

## Tweet 1
I built a typed, replayable programming language for LLM agents.

It's called Nudge.

Here's why it exists and what it does: 🧵

## Tweet 2
The problem: Production agents are held together with glue code.

- Untyped LLM output (parse JSON, hope for the best)
- No regression testing (what worked yesterday might not work today)
- Cost surprises (no budget control)
- Hidden side effects

## Tweet 3
Nudge fixes this at the language level:

```
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string) -> [Finding] uses LLM {
    llm"""Extract findings about {q}"""
    with { schema: [Finding], budget: 0.03 USD }
}
```

Schema is a type. Proven at compile time.

## Tweet 4
Every run emits a trace. Every trace is a test.

```nudge
test "stays within budget" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25  // zero tokens burned in CI
}
```

## Tweet 5
Budget is a contract, enforced by compiler + runtime.

No more surprise API bills.

## Tweet 6
Zero dependencies. Single Rust binary. Builds in seconds.

Compiles to Python & TypeScript.

GitHub: https://github.com/NekomyaDev/nudge

## Tweet 7
VS Code extension: https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang

Install: `curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash`

#rustlang #python #typescript #ai #llm #programming
