# Contributing to joerl

Thank you for your interest in contributing to joerl! This document provides guidelines and information for contributors.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/yourusername/joerl.git`
3. Create a feature branch: `git checkout -b feature/my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Format code: `cargo fmt`
7. Run linter: `cargo clippy`
8. Commit your changes: `git commit -am 'Add my feature'`
9. Push to the branch: `git push origin feature/my-feature`
10. Open a Pull Request

## Development Setup

### Prerequisites

- Rust 1.85 or later (for edition 2024 support)
- Cargo

### Building

```bash
cargo build
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test integration_test
```

### Code Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html

# Open coverage report
open tarpaulin-report.html
```

## Code Style

We follow the standard Rust style guidelines:

- Use `cargo fmt` to format code
- Use `cargo clippy` to check for common mistakes
- Write idiomatic Rust code
- Add documentation comments for public APIs
- Include examples in documentation when appropriate

## Testing

- All new features must include tests
- Aim for high test coverage (>80%)
- Write both unit tests and integration tests
- Test edge cases and error conditions
- Use descriptive test names that explain what is being tested

## Documentation

- Document all public APIs with doc comments (`///`)
- Include examples in documentation
- Keep README.md up to date
- Add entries to CHANGELOG.md for notable changes

## Pull Request Process

1. Update the README.md with details of changes if applicable
2. Update the CHANGELOG.md following [Keep a Changelog](https://keepachangelog.com/) format
3. Ensure all tests pass
4. Ensure code is properly formatted and linted
5. Request review from maintainers
6. Address any review comments

## Commit Messages

Follow conventional commit format:

- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `test:` for test additions/changes
- `refactor:` for code refactoring
- `perf:` for performance improvements
- `chore:` for maintenance tasks

Example: `feat: add support for remote actors`

## Code of Conduct

### Our Pledge

We pledge to make participation in our project a harassment-free experience for everyone.

### Our Standards

- Be respectful and inclusive
- Be patient and welcoming
- Accept constructive criticism gracefully
- Focus on what is best for the community
- Show empathy towards others

## Questions?

Feel free to open an issue for:
- Bug reports
- Feature requests
- Questions about the codebase
- Suggestions for improvements

## License

By contributing to joerl, you agree that your contributions will be licensed under its MIT OR Apache-2.0 license.
