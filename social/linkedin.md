# LinkedIn Post

## Title
I built a typed, replayable programming language for LLM agents

## Body

Excited to share a project I've been working on: **Nudge** — a programming language designed specifically for LLM agents.

**The Problem:**
Production AI agents are still held together with glue code — prompt chains parsed by hand, tool calls wrapped in try/except, no replay, no cost control, no regression tests.

**The Solution:**
Nudge fixes this at the language level:

✅ **Typed LLM calls** — output schema is a language type, proven at compile time
✅ **Deterministic replay** — every run emits a trace; every trace is a test
✅ **Budget contracts** — per-call, per-run USD ceilings enforced by compiler
✅ **Effect system** — pure / LLM / Tool / IO effects inferred and shown
✅ **Native parallelism** — `par map/race/all` with compile-time race safety

**Example:**
```
fn analyze(q: string) -> [Finding] uses LLM {
    llm"""Extract findings about {q}"""
    with { schema: [Finding], budget: 0.03 USD }
}
```

**Zero dependencies** — single Rust binary, compiles to Python & TypeScript.

GitHub: https://github.com/NekomyaDev/nudge

Looking for feedback from the AI/ML and developer tools community!

#AI #LLM #ProgrammingLanguage #Rust #Python #TypeScript #DeveloperTools #OpenSource
