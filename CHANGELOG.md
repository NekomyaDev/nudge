# Changelog

All notable changes to Nudge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- **NTF open standard**: `docs/ntf-spec.md` (frozen v1 spec page), a conformance corpus (`conformance/` — 17 cases with expected verdicts, wired into `cargo test` and a vendor-neutral `conformance/run.py` runner), and a LangChain → NTF bridge (`bridges/langchain_ntf.py`)
- **`nudge-ci` GitHub Action** (`action.yml`): agent regression CI in any repo — `nudgec check` + `nudgec test` (replay, $0) over a glob of `.ndg` programs, with optional `trace-diff` regression gate and `version`/`check-only` inputs; the repository now dogfoods it via `.github/workflows/agent-ci.yml`
- `nudgec trace-diff a b --fail-on-regression`: exits 1 when the candidate trace costs more, uses more tokens, performs more repair rounds, or turns a passing llm call into a failure — turning trace diffing into a CI gate
- `install.sh`: `NUDGE_VERSION` environment variable pins an exact release (or `main`) instead of the per-platform default

## [1.2.1] - 2026-09-26

### Open source
- **Nudge is now open source.** The compiler (`crates/nudgec`), the bytecode VM experiment (`crates/nudge-runtime`), the Python runtime (`runtime/nudge_runtime`), its TypeScript port (`runtime/nudge_runtime.ts`), and the VS Code extension source (`editors/vscode/`) now live in this repository
- Language design documentation: `docs/design.md` (language spec) and `docs/roadmap.md`
- Additional example agents: `hello_llm.ndg`, `checkpoint_agent.ndg`, `smoke_provider.ndg`, `research_agent.ndg` (with replay fixtures under `examples/traces/`)
- Rust CI workflow (`cargo test` + `rustfmt` + `clippy -D warnings`) and the tag-driven release workflow moved into this repository
- License changed from proprietary (closed source, binaries under a separate distribution license) to **Apache-2.0** — the repository and the distributed binaries now share the same license

### Added
- `nudgec --version` / `-V` prints the version and exits 0 (previously printed the usage banner and exited 64)
- Named-argument MCP calls: tool stubs now send MCP `tools/call` arguments as an object keyed by parameter name, so FastMCP-style servers work out of the box (previously a positional `{"args": [...]}` list that most servers reject)
- Live tool results support `.field` access (AttrDict-wrapped), so branching on a tool result works
- Map types: `{string: T}` lowers to `{type: object, additionalProperties: T}` and accepts any field access in the checker
- README now documents the MCP transport contract and the full `NUDGE_*` environment variable reference

### Fixed
- Replay record index is reserved atomically under the replay lock — `par` fan-out no longer double-consumes records
- Docker image builds `nudgec` from source and vendors the Python runtime from `runtime/` (the broken `COPY runtime/` step is gone)

### Notes
- Linux release assets and the Docker image (`nekomyadev/nudge:1.2.1`, `:latest`) ship the new compiler; the macOS/Windows tarballs on this release are unchanged from v1.2.0 and will be refreshed in a later release. macOS/Windows users can use Docker or `pip install nudge-runtime` meanwhile.
- PyPI: `nudge-runtime` 1.1.0

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
