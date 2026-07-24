# Nudge for VS Code

Language support for [Nudge](https://github.com/NekomyaDev/nudge) — the typed,
replayable, budget-aware language for LLM agents (`.ndg`).

## Features

- **Syntax highlighting** — keywords, record types, `llm"""` prompts with
  interpolation, USD money literals, `@format` / `@range` constraints
- **Diagnostics** — real parse + type errors as you type, powered by the
  compiler itself (`nudgec lsp` over stdio — zero re-implementation)
- **Snippets** — `llm`, `type`, `par`, `fn`, `test`

## Requirements

The diagnostics need the `nudgec` binary on your `PATH`. Get it from the
[GitHub Releases](https://github.com/NekomyaDev/nudge/releases) (prebuilt for
Linux / macOS / Windows) or `cargo build --release` from source.

If `nudgec` lives elsewhere, set `nudge.serverPath` in your settings.

## Install from .vsix

```
code --install-extension nudge-1.0.0.vsix
```

or: Extensions view → `...` → *Install from VSIX...*
