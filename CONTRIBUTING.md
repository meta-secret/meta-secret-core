# Contributing to Meta Secret

Thank you for your interest in contributing to Meta Secret! This document provides guidelines and information for contributors.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Code Structure](#code-structure)
- [Contributing Process](#contributing-process)
- [Code Standards](#code-standards)
- [Testing](#testing)
- [Documentation](#documentation)
- [Community Guidelines](#community-guidelines)

## Getting Started

Meta Secret is a decentralized password manager built with Rust that uses advanced cryptography including Shamir Secret Sharing. Before contributing, please:

1. Read the [README.md](README.md) to understand the project
2. Check out the [Development Guide](docs/DEVELOPMENT.md) for setup instructions
3. Review the [Architecture Documentation](docs/ARCHITECTURE.md) to understand the system design
4. Look at existing issues and discussions to see what needs work

## Development Setup

### Prerequisites

- **Rust**: Install via [rustup](https://rustup.rs/)
- **Node.js**: Required for web components (version 16+)
- **Docker**: For containerized builds and testing
- **Earthly**: For reproducible builds (optional but recommended)

### Quick Setup

```bash
# Clone the repository
git clone https://github.com/meta-secret/meta-secret-core.git
cd meta-secret-core

# Build the core library
cd meta-secret/core
cargo build

# Run tests
cargo test

# Build CLI tool
cd ../cli
cargo build --release
```

For detailed setup instructions, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Code Structure

The project is organized as a Rust workspace with multiple crates:

- `meta-secret/core/` - Core cryptographic functionality
- `meta-secret/cli/` - Command-line interface
- `meta-secret/meta-cli/` - Enhanced CLI with interactive features
- `meta-secret/wasm/` - WebAssembly bindings
- `meta-secret/mobile/` - Mobile platform bindings
- `meta-secret/meta-server/` - Server components
- `meta-secret/web-cli/ui/` - Web interface (Vue.js)

## Contributing Process

### 1. Find or Create an Issue

- Check existing [issues](https://github.com/meta-secret/meta-secret-core/issues)
- For new features, create an issue to discuss before implementing
- For bugs, provide detailed reproduction steps

### 2. Fork and Branch

```bash
# Fork the repository on GitHub, then:
git clone https://github.com/your-username/meta-secret-core.git
cd meta-secret-core
git checkout -b feature/your-feature-name
```

### 3. Make Changes

- Follow the [Code Standards](#code-standards)
- Add tests for new functionality
- Update documentation as needed
- Keep commits focused and write clear commit messages

### 4. Test Your Changes

```bash
# Run all tests
cargo test

# Test specific components
cd meta-secret/core && cargo test
cd meta-secret/cli && cargo test

# Test WebAssembly (requires Firefox)
make wasm_test_headless
```

### 5. Submit a Pull Request

- Push your branch to your fork
- Create a pull request with a clear title and description
- Reference any related issues
- Ensure CI tests pass

## Code Standards

### Rust Code

- Follow standard Rust formatting: `cargo fmt`
- Ensure code passes linting: `cargo clippy`
- Use meaningful variable and function names
- Add documentation comments for public APIs
- Handle errors appropriately using `Result` types

### Documentation

- Document all public functions with rustdoc comments
- Include usage examples in documentation
- Update README.md for user-facing changes
- Keep documentation current with code changes

### Commit Messages

Use conventional commit format:

```
type(scope): description

Examples:
feat(core): add new secret splitting algorithm
fix(cli): resolve QR code parsing error
docs(readme): update installation instructions
test(core): add unit tests for encryption
```

## Testing

### Unit Tests

Each component should have comprehensive unit tests:

```bash
# Run tests for all components
cargo test

# Run tests with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html
```

### Integration Tests

Integration tests are located in the `tests/` directory and test interactions between components.

### Security Testing

For cryptographic changes:
- Ensure compatibility with existing encrypted data
- Test edge cases and error conditions
- Consider timing attack implications
- Review cryptographic implementations carefully

## Documentation

### API Documentation

Generate API documentation with:

```bash
cargo doc --no-deps --open
```

### User Documentation

- Update README.md for user-facing changes
- Add examples to demonstrate new features
- Keep installation and usage instructions current

## Community Guidelines

### Code of Conduct

We follow the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). Please be respectful and inclusive in all interactions.

### Security

**Do not report security vulnerabilities in public issues.** Instead, please email the maintainers directly or use GitHub's private vulnerability reporting feature.

### Questions and Support

- For usage questions, check existing documentation first
- For development questions, create a discussion or issue
- For real-time chat, join our community channels (if available)

### Recognition

Contributors will be recognized in release notes and project documentation. Thank you for helping make Meta Secret better!

## License

By contributing to Meta Secret, you agree that your contributions will be licensed under the same license as the project.