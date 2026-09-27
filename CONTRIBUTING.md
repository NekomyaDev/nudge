# Contributing to Nudge

Thanks for considering a contribution. Nudge is Apache-2.0; everything here (compiler, runtime, examples, docs) is built in the open.

## The lay of the land

| Directory | What it is |
|:---|:---|
| `crates/nudgec/` | the Rust compiler (lexer → parser → checker → Python/TS codegen, traces, CLI) |
| `runtime/` | the Python runtime (`nudge_runtime/`) and TypeScript runtime, test suites included |
| `examples/` | runnable example agents (also the source of `nudgec init` templates) |
| `docs/` | design notes, the decision contract (`decision.md`), the NTF spec (`ntf-spec.md`), recipes |
| `conformance/` | the vendor-neutral frozen-v1 trace conformance suite |
| `bridges/` | framework → NTF bridges (LangChain done; LangGraph/CrewAI wanted) |

## Ground rules

- **Additive on main.** Frozen-v1 NTF fields and the core grammar never break; new capability ships additively.
- **Every feature is tested.** New compiler diagnostics get a test showing the error *and* the message; runtime features get parity tests on both backends when applicable.
- **No dependencies** in the Python runtime and the emitted code; compiler deps stay minimal.
- **Docs move with code** — README(s), `CHANGELOG.md`, and the relevant `docs/` page.

## Good first contributions

- The open bridges ([#69](https://github.com/NekomyaDev/nudge/issues/69)): LangGraph callback tracer, CrewAI hooks
- Vendor list additions in [docs/jev-conformance.md](docs/jev-conformance.md) (if your decision server passes the conformance suite)
- More recipes for `docs/recipes/` (each recipe must type-check — CI enforces it)
- More lessons for `nudgec learn` (lesson programs are compiler-verified, same rule)

## How to submit

1. Fork, branch (`feat/…` or `fix/…`).
2. `cargo test --workspace && cargo fmt && cargo clippy --workspace --all-targets` must be clean; `node --experimental-strip-types --test runtime/` for runtime changes.
3. Open a PR describing the *problem* first, then the change. Keep PRs single-topic.

## Report a problem

Security-sensitive? See the audit notes in the release notes; please use GitHub private vulnerability reporting rather than a public issue. Everything else: a GitHub issue with a minimal reproducer (an `.ndg` snippet, a trace, or a command) is perfect.
