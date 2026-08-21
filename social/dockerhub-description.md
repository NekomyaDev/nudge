# Docker Hub Description

## Short Description
Typed, replayable, budget-aware programming language for LLM agents

## Full Description

# Nudge

A typed, replayable, budget-aware programming language for LLM agents.

## Features

- **Typed LLM Calls** - Output schema is a language type; violations trigger automatic repair
- **Deterministic Replay** - Full, hybrid, and live modes; traces are git-friendly JSONL
- **Budget Contracts** - Per-call, per-run, and per-repair USD ceilings
- **Effect System** - Pure / LLM / Tool / IO effects inferred
- **Native Parallelism** - `par map`, `par race`, `par all`
- **Compiles to Python & TypeScript**

## Quick Start

```bash
# Pull the image
docker pull nekomyadev/nudge:latest

# Run nudgec
docker run -it --rm nekomyadev/nudge nudgec --help

# Mount your project
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec check hello.ndg
```

## Example

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string) -> [Finding] uses LLM {
    llm"""Extract findings about {q}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6", budget: 0.03 USD }
}
```

## Links

- [GitHub](https://github.com/NekomyaDev/nudge)
- [VS Code Extension](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang)
- [Documentation](https://github.com/NekomyaDev/nudge#readme)

## Tags

- `latest` - Latest stable release
- `1.2.0` - Version 1.2.0
