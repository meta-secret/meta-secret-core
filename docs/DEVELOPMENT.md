# Development Guide

This guide provides detailed instructions for setting up a development environment for Meta Secret.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Environment Setup](#environment-setup)
- [Building Components](#building-components)
- [Development Workflow](#development-workflow)
- [Testing](#testing)
- [Debugging](#debugging)
- [Platform-Specific Setup](#platform-specific-setup)
- [Troubleshooting](#troubleshooting)

## Prerequisites

### Required Tools

1. **Rust Toolchain**
   ```bash
   # Install rustup (Rust installer)
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   
   # Install required targets
   rustup target add wasm32-unknown-unknown
   ```

2. **Node.js and npm** (for web components)
   ```bash
   # Install Node.js 16+ (via nvm recommended)
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
   nvm install 16
   nvm use 16
   ```

3. **Docker** (for containerized builds)
   - Install from [docker.com](https://docs.docker.com/get-docker/)

4. **Additional Tools**
   ```bash
   # WebAssembly tools
   cargo install wasm-pack
   
   # Development utilities
   cargo install cargo-watch
   cargo install cargo-tarpaulin  # For test coverage
   
   # Optional: Earthly for reproducible builds
   # Install from https://earthly.dev/get-earthly
   ```

### System Dependencies

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install build-essential pkg-config libssl-dev
```

**macOS:**
```bash
# Install Xcode command line tools
xcode-select --install

# Install homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

**Windows:**
- Install Visual Studio Build Tools or Visual Studio Community
- Install Git for Windows

## Environment Setup

### 1. Clone the Repository

```bash
git clone https://github.com/meta-secret/meta-secret-core.git
cd meta-secret-core
```

### 2. Verify Installation

```bash
# Check Rust installation
rustc --version
cargo --version

# Check Node.js installation
node --version
npm --version

# Check Docker installation
docker --version
```

### 3. Build Verification

```bash
# Build the core library
cd meta-secret/core
cargo build
cargo test

# If successful, you're ready to develop!
```

## Building Components

### Core Library

The foundation of Meta Secret, containing cryptographic primitives and secret sharing logic.

```bash
cd meta-secret/core

# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

### Command Line Interface (CLI)

```bash
cd meta-secret/cli

# Build CLI
cargo build --release

# Test CLI functionality
./target/release/meta-secret-cli --help

# Or run directly with cargo
cargo run -- --help
```

### Enhanced CLI (meta-cli)

Interactive CLI with additional features:

```bash
cd meta-secret/meta-cli

cargo build --release
cargo run -- --help
```

### WebAssembly (WASM)

For browser integration:

```bash
cd meta-secret/wasm

# Build WASM package
wasm-pack build --target web

# Run browser tests (requires Firefox)
wasm-pack test --firefox

# Headless testing
wasm-pack test --headless --firefox
```

### Web Interface

Vue.js-based web application:

```bash
cd meta-secret/web-cli/ui

# Install dependencies
npm install

# Development server
npm run dev

# Build for production
npm run build

# Run tests
npm run test
```

### Mobile Components

For iOS and Android integration:

```bash
# iOS
cd meta-secret/mobile/ios
cargo build

# Android
cd meta-secret/mobile/android
cargo build
```

### Server Components

```bash
# Server node
cd meta-secret/meta-server/server-node
cargo build

# Web server
cd meta-secret/meta-server/web-server
cargo build
```

## Development Workflow

### 1. Feature Development

```bash
# Create feature branch
git checkout -b feature/your-feature-name

# Make changes and test frequently
cargo watch -x "test"

# Commit changes
git add .
git commit -m "feat(core): add new feature"
```

### 2. Testing Workflow

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Test with output
cargo test -- --nocapture

# Watch mode for continuous testing
cargo watch -x test
```

### 3. Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Fix linter issues automatically
cargo clippy --fix
```

## Testing

### Unit Tests

Each component has its own test suite:

```bash
# Core library tests
cd meta-secret/core
cargo test

# CLI tests
cd meta-secret/cli
cargo test

# Run specific test module
cargo test crypto::tests
```

### Integration Tests

Located in the `tests/` directory:

```bash
cd meta-secret/tests
cargo test
```

### WebAssembly Tests

```bash
cd meta-secret/wasm

# Interactive testing (opens browser)
wasm-pack test --firefox

# Headless testing
make wasm_test_headless
```

### End-to-End Testing

```bash
# Web interface E2E tests
cd meta-secret/web-cli/ui
npm run cypress:run
```

### Test Coverage

```bash
# Generate coverage report
cargo tarpaulin --out Html

# View coverage report
open tarpaulin-report.html
```

## Debugging

### Rust Debugging

1. **Using `println!` and `dbg!` macros:**
   ```rust
   println!("Debug: {:?}", variable);
   dbg!(&variable);
   ```

2. **Using a debugger (VS Code with rust-analyzer):**
   - Install rust-analyzer extension
   - Set breakpoints in your code
   - Run with F5 or use launch configuration

3. **Logging with tracing:**
   ```rust
   use tracing::{info, debug, error};
   
   debug!("Processing secret with config: {:?}", config);
   ```

### WebAssembly Debugging

```bash
# Build with debug symbols
wasm-pack build --dev --target web

# Use browser developer tools
# Set breakpoints in the Sources tab
```

### Web Interface Debugging

```bash
cd meta-secret/web-cli/ui

# Development server with hot reload
npm run dev

# Use Vue DevTools extension in browser
```

## Platform-Specific Setup

### Cross-Compilation

For building on different platforms:

```bash
# Add targets
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Cross-compile
cargo build --target x86_64-pc-windows-gnu
```

### Docker Development

```bash
# Build in Docker container
docker run --rm -v $(pwd):/workspace -w /workspace rust:latest cargo build

# Use provided Dockerfile for consistent environment
docker build -t meta-secret-dev .
docker run -it meta-secret-dev bash
```

### Earthly Builds

If you have Earthly installed:

```bash
# Build all targets
earthly +build

# Build specific component
earthly +build-cli

# Build with AI tools (requires API key)
earthly +build-taskomatic-ai --ANTHROPIC_API_KEY="your_key_here"
```

## Troubleshooting

### Common Issues

1. **Compilation Errors with OpenSSL:**
   ```bash
   # Linux
   sudo apt-get install libssl-dev pkg-config
   
   # macOS
   brew install openssl
   export OPENSSL_DIR=$(brew --prefix openssl)
   ```

2. **WASM Compilation Issues:**
   ```bash
   # Reinstall wasm-pack
   cargo install wasm-pack --force
   
   # Clear cache
   cargo clean
   ```

3. **Node.js Version Issues:**
   ```bash
   # Use correct Node version
   nvm use 16
   
   # Clear npm cache
   npm cache clean --force
   ```

4. **Permission Issues on macOS/Linux:**
   ```bash
   # Fix cargo directory permissions
   sudo chown -R $(whoami) ~/.cargo
   ```

### Getting Help

- Check existing [issues](https://github.com/meta-secret/meta-secret-core/issues)
- Review component-specific README files
- Ask questions in discussions
- Check the main [README.md](../README.md) for basic setup

### Performance Tips

- Use `cargo build --release` for performance testing
- Enable LTO for smaller binaries: `RUSTFLAGS="-C lto=fat" cargo build --release`
- Use `cargo flamegraph` for profiling (requires `cargo install flamegraph`)

## Next Steps

Once your development environment is set up:

1. Read the [Architecture Documentation](ARCHITECTURE.md)
2. Review the [API Documentation](API.md)
3. Check out the [Contributing Guidelines](../CONTRIBUTING.md)
4. Look at open issues to find something to work on!