# Contributing to Nudge

Thank you for your interest in contributing to Nudge! This document provides guidelines and information for contributors.

## Getting Started

1. Contact [@NekomyaDev](https://github.com/NekomyaDev) to access the private source repository
2. Fork the repository
3. Create a feature branch
4. Make your changes
5. Submit a pull request

## Development Setup

```bash
# Clone the repository
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
- Write meaningful commit messages

## Pull Request Process

1. Update documentation if needed
2. Add tests for new features
3. Ensure all tests pass
4. Request review from maintainers

## Reporting Issues

- Use GitHub Issues for bug reports
- Include reproduction steps
- Provide error messages and logs
- Specify your environment (OS, Rust version, etc.)

## License

By contributing, you agree that your contributions will be licensed under the project's proprietary license.

## Contact

For questions or discussions, contact [@NekomyaDev](https://github.com/NekomyaDev).
