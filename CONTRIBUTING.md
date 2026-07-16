# Contributing to Nudge

Nudge is in pre-alpha: the design is frozen at v1.1 of [docs/design.md](docs/design.md) and the compiler is being built against it.

## How to help right now

1. **Design review** — read docs/design.md and open an issue on anything that is ambiguous, missing, or wrong. Ambiguity in a language spec is a bug.
2. **Compiler** — see [docs/roadmap.md](docs/roadmap.md) for the current MVP phase. Pick an open item, comment on the tracking issue before starting.
3. **Examples** — small `.ndg` programs that exercise one feature each live in `examples/`.

## Ground rules

- The design doc is the source of truth. Code that diverges from it without a design amendment will not merge.
- Every compiler error must have a code (E0xx), a message, and a conformance test.
- Traces and error messages are English-first; diagnostics are designed to be localizable (zh-CN first target).
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `design:`, `test:` …).

## Setup

```bash
cargo build          # compiler workspace
cargo test           # unit + conformance tests
```
