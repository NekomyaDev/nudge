# Changelog

## 1.2.1

- Added document formatting provider powered by `nudgec fmt`.
- Added Debug Adapter Protocol (DAP) integration for stepping through Nudge `.jsonl` trace files via `nudgec debug`.
- Added Nudge command palette actions:
  - `Nudge: Type-Check File (nudgec check)`
  - `Nudge: Format Document (nudgec fmt)`
  - `Nudge: Build / Compile (nudgec build)`
  - `Nudge: Build TypeScript (nudgec build-ts)`
  - `Nudge: Run Tests (nudgec test)`
  - `Nudge: Static Cost Analysis (nudgec cost)`
  - `Nudge: View Trace (nudgec trace-view)`
  - `Nudge: Restart Language Server`
- Updated syntax highlighting with `forall`, `merge`, `route`, `decide`, `repair`, `computer`, and `stream`.
- Expanded snippet library for `agent`, `tool`, `decide`, `route`, and `forall`.
- Added license file and packaging scripts.

## 1.2.0

- Diagnostics, hover, go-to-definition, and symbol completion via `nudgec lsp`.

## 1.0.0

- Initial release: TextMate grammar for `.ndg`, language configuration,
  snippets, and diagnostics wired to `nudgec lsp` over stdio.
