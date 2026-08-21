# Hacker News Post

## Title
Show HN: Nudge – A typed programming language for LLM agents

## URL
https://github.com/NekomyaDev/nudge

## Comment (first comment)

Hi HN! I built Nudge, a programming language for LLM agents.

**The problem:** Production agents are held together with glue code — prompt chains parsed by hand, tool calls wrapped in try/except, no replay, no cost control.

**What Nudge does:**
- Schema is a type (proven at compile time, not runtime)
- Every run emits a trace (every trace is a test)
- Budget is a contract (enforced by compiler + runtime)
- Compiles to Python & TypeScript

**Example:**
```nudge
fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract findings about {q} from: {hits}"""
    with { schema: [Finding], budget: 0.03 USD, retry: 2 with repair }
}
```

**Zero dependencies** — single Rust binary, builds in seconds.

Looking for feedback on the language design and compiler architecture!
