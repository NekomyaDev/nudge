# Nudge for VS Code

Language support for [Nudge](https://github.com/NekomyaDev/nudge) — the typed,
replayable, budget-aware language for LLM agents (`.ndg`).

## Features

- **Syntax highlighting** — keywords, agents, state blocks, record types, `llm"""` prompts with
  interpolation, USD money literals, `@format` / `@range` constraints
- **Diagnostics, Hover & Go-to-Definition** — powered by the compiler itself (`nudgec lsp` over stdio)
- **Document Formatting** — format on save or on demand via `nudgec fmt`
- **Commands & Tooling** — run `nudgec check`, `build`, `build-ts`, `test`, `cost`, and launch trace view directly from the command palette
- **Trace Debugging** — debug agent execution traces (`.jsonl`) via the Debug Adapter Protocol (DAP)
- **Snippets** — `llm`, `agent`, `tool`, `decide`, `route`, `forall`, `type`, `par`, `fn`, `test`

## Requirements

The language server and commands need the `nudgec` binary on your `PATH`. Get it from the
[GitHub Releases](https://github.com/NekomyaDev/nudge/releases) (prebuilt for
Linux / macOS / Windows) or build from source with `cargo build --release`.

If `nudgec` lives elsewhere, configure `nudge.serverPath` in your VS Code settings.

## Debugging Traces

To step through a recorded trace (`.jsonl`), add a launch configuration in `.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "nudge",
      "request": "launch",
      "name": "Debug Nudge Trace",
      "trace": "${file}"
    }
  ]
}
```

## Install from .vsix

```bash
code --install-extension nudge-lang-1.2.1.vsix
```

or: Extensions view → `...` → *Install from VSIX...*
