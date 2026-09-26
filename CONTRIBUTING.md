# Contributing to Nudge

Thank you for your interest in contributing to Nudge! Everything lives in this repository: the compiler (`crates/nudgec`), the bytecode VM experiment (`crates/nudge-runtime`), the Python runtime (`runtime/`), and the VS Code extension (`editors/vscode/`).

## Getting Started

1. Fork the repository and create a feature branch
2. Make your changes
3. Submit a pull request

Look for issues labeled `good first issue` or `help wanted` if you want somewhere to start. If an issue is already assigned or has an open PR, pick a different one — and commenting on an issue before starting work is always appreciated.

## Development Setup

Requirements: Rust 1.75+ (stable), Python 3.10+, Node 22 (for the TS backend tests).

```bash
git clone https://github.com/NekomyaDev/nudge.git
cd nudge

# Build the compiler
cargo build

# Run tests (compiler + bytecode VM)
cargo test --workspace

# Lint and format
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Python runtime (no dependencies, pure stdlib)
pip install -e ./runtime
python -c "import nudge_runtime"

# VS Code extension
cd editors/vscode && npm install
```

Working on examples? The [fake provider](README.md#environment-variables) runs everything deterministically — no API keys needed:

```bash
cargo run -p nudgec -- test examples/chatbot/chatbot.ndg
```

## Code Style

- Follow Rust conventions; `cargo fmt` and `cargo clippy -D warnings` must pass (CI enforces both)
- Python runtime stays dependency-free and stdlib-only
- Conventional Commits for commit messages
- Never commit `Co-Authored-By` trailers referencing AI tools

## Pull Request Process

1. Update documentation if your change affects behavior
2. Add tests for new features (compiler modules carry inline `#[cfg(test)]` suites; extend whichever fits your change)
3. Ensure all tests pass and CI is green
4. Keep PRs focused — one logical change per PR

## Reporting Issues

Use [GitHub Issues](https://github.com/NekomyaDev/nudge/issues):

- Bug reports: include reproduction steps, error messages, and your environment (OS, Rust version, nudgec version)
- Feature requests: describe the problem you are trying to solve, not just the solution
- Security issues: please follow [SECURITY.md](SECURITY.md) instead of opening a public issue

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
