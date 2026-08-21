# Contributing to Nudge

Thank you for your interest in contributing to Nudge!

## Getting Started

Nudge is closed source. To contribute:

1. Contact [@NekomyaDev](https://github.com/NekomyaDev) to request access to the private source repository
2. If approved, you will be added as a collaborator
3. Clone the private repository
4. Create a feature branch
5. Make your changes
6. Submit a pull request

## Development Setup

```bash
# Clone the private repository (requires access)
git clone https://github.com/NekomyaDev/nudge-source.git
cd nudge-source

# Build the compiler
cargo build

# Run tests
cargo test

# Run linter
cargo clippy --workspace --all-targets -- -D warnings
```

## Code Style

- Follow Rust conventions
- Use `cargo fmt` for formatting
- Ensure `cargo clippy` passes without warnings
- Write meaningful commit messages following Conventional Commits

## Pull Request Process

1. Update documentation if needed
2. Add tests for new features
3. Ensure all tests pass
4. Request review from maintainers

## Reporting Issues

Use [GitHub Issues](https://github.com/NekomyaDev/nudge/issues) for bug reports:

- Include reproduction steps
- Provide error messages and logs
- Specify your environment (OS, Rust version, etc.)

## License

By contributing, you agree that your contributions will be licensed under the project's proprietary license.

## Contact

For questions or discussions, contact [@NekomyaDev](https://github.com/NekomyaDev).
