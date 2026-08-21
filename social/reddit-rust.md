# Reddit Post - r/rust

## Title
Built a compiler in Rust for LLM agents — zero dependencies, single binary

## Body

Hey r/rust!

Just open-sourced Nudge, a programming language for LLM agents, written entirely in Rust.

**Why Rust?**
- Single binary distribution (no runtime dependencies)
- Fast compilation (lexer → parser → checker → codegen in milliseconds)
- Memory safety for a compiler that handles untrusted input
- Cross-platform (Linux, macOS, Windows)

**The compiler architecture:**
```
nudgec lex <file.ndg>    # dump token stream
nudgec parse <file.ndg>  # dump AST
nudgec check <file.ndg>  # type + effect verification
nudgec build <file.ndg>  # emit Python to out/
nudgec build-ts <file.ndg> # emit TypeScript
nudgec cost <file.ndg>   # static cost report
nudgec test <file.ndg>   # replay traces — zero tokens
```

**Zero dependencies** — uses only stdlib. The entire compiler is ~2000 lines of Rust.

**What it does:**
- Typed LLM calls with schema validation
- Deterministic replay (traces are git-friendly JSONL)
- Budget enforcement (per-call, per-run USD ceilings)
- Effect system (pure / LLM / Tool / IO)
- Prompt Clippy (lints your llm""" blocks)

GitHub: https://github.com/NekomyaDev/nudge

Built with Rust 2021 edition, compiles in seconds. Would love feedback from the Rust community!
