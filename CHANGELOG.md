# Changelog

All notable changes to Nudge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [1.2.0] - 2026-07-29

### Added
- Trace viewer: `nudgec trace-view <trace.jsonl>` opens a local web UI
- Trace diff: `nudgec trace-diff a.jsonl b.jsonl` compares two traces
- DAP support: `nudgec debug <trace.jsonl>` for trace debugging
- VS Code extension on Marketplace
- Real providers: OpenAI, Gemini, Groq, MiMo, Mistral, Anthropic, Ollama
- Streaming with early-abort schema validation
- MCP stdio transport
- LSP hover, definition, completion
- Prompt Clippy linter (W0001-W0004)
- Tag-driven release workflow with prebuilt binaries

### Changed
- Design frozen at v1.24
- Trace format frozen at v1

## [1.1.0] - 2026-07-20

### Added
- Real provider support (OpenAI-compatible)
- Binary distribution (Linux, macOS, Windows)
- VS Code extension (syntax highlighting, snippets)
- LSP server (`nudgec lsp`)
- A2A agent-card export

## [1.0.0] - 2026-07-15

### Added
- Initial release
- Lexer, parser, type checker, codegen
- Python and TypeScript backends
- Trace and replay system
- Budget enforcement
- Parallel execution (par map/race/all)
- Agent state and checkpoint/resume
- Effect system
- MCP interop
