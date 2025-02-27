# Meta Secret Project Overview

## Introduction

Meta Secret is a decentralized password manager that leverages advanced cryptography and decentralized storage to securely manage user data. Unlike traditional password managers, Meta Secret doesn't rely on a master password for access. Instead, it implements Shamir's Secret Sharing (SSS) technique to split confidential information into multiple shares, requiring only a subset of these shares for recovery.

## Key Features

- **No Master Password**: Uses biometric authentication and secret sharing techniques
- **Decentralized Architecture**: Operates directly on user devices
- **Encryption-First Approach**: All data is encrypted before being stored or transmitted
- **Cross-Platform Support**: Access from multiple devices without compromising security
- **Open-Source Infrastructure**: Provides transparency and increased security

## Technology Stack

### Core Components

- **Rust**: The core cryptographic library is written in Rust for safety and performance
- **WebAssembly (WASM)**: Enables the core functionality to run in web environments
- **Swift**: Native iOS implementation leverages the core Rust library
- **Cryptographic Libraries**:
  - Shamir's Secret Sharing (shamirsecretsharing)
  - Ed25519 for digital signatures (ed25519-dalek)
  - Age encryption for secure data storage

## Project Structure

```
meta-secret-core/
├── meta-secret/            # Main project directory
│   ├── core/               # Core Rust library with cryptographic functionality
│   │   ├── src/            # Source code for the core library
│   │   │   ├── crypto/     # Cryptographic utilities
│   │   │   ├── errors/     # Error definitions
│   │   │   ├── node/       # Node implementation
│   │   │   ├── secret/     # Secret sharing implementation
│   │   │   └── lib.rs      # Main library entry point
│   ├── cli/                # Command-line interface
│   ├── wasm/               # WebAssembly bindings
│   ├── core-swift-lib/     # Swift bindings for iOS
│   ├── web-cli/            # Web-based CLI interface
│   └── meta-server/        # Server implementation
├── docs/                   # Documentation
└── infra/                  # Infrastructure configuration
```

## Core Functionality

### Secret Splitting (Shamir's Secret Sharing)

The core functionality of Meta Secret is to split a secret (like a password) into multiple shares. The secret can be reconstructed only when a threshold number of shares are combined. This is implemented using Shamir's Secret Sharing algorithm.

Key operations:

1. **Split**: Divides a secret into multiple shares
   ```rust
   pub fn split(secret: String, config: SharedSecretConfig) -> CoreResult<()>
   ```

2. **Recover**: Reconstructs the secret from multiple shares
   ```rust
   pub fn recover() -> CoreResult<PlainText>
   pub fn recover_from_shares(users_shares: Vec<UserShareDto>) -> CoreResult<PlainText>
   ```

3. **QR Code Generation**: Shares can be represented as QR codes for easy storage
   ```rust
   pub fn generate_qr_code(data: &str, path: &str)
   ```

4. **QR Code Parsing**: QR codes can be read back into shares
   ```rust
   pub fn read_qr_code(path: &Path) -> Result<String, QrCodeParserError>
   ```

## Application Workflow

### Password Splitting Process
1. User inputs a password or secret
2. The system applies Shamir's Secret Sharing to split it into multiple shares (typically 3)
3. Each share is encrypted and can be exported as JSON or QR code
4. The user stores these shares in different secure locations

### Password Recovery Process
1. User provides at least the threshold number of shares (typically 2 out of 3)
2. Shares are validated and decrypted
3. The original secret is reconstructed using the SSS algorithm
4. The recovered password is presented to the user

## Data Flow

```
User Secret → Encryption → Secret Splitting → Multiple Shares (as JSON/QR)
Multiple Shares → Validation → Secret Reconstruction → Decryption → Original Secret
```

## Integration Points

### Command Line Interface
The CLI provides a user-friendly way to split and recover secrets using the core library:

```bash
# Split a secret
docker run -ti --rm -v "$(pwd)/secrets:/app/secrets" ghcr.io/meta-secret/cli:latest split --secret top$ecret

# Recover a secret
docker run -ti --rm -v "$(pwd)/secrets:/app/secrets" ghcr.io/meta-secret/cli:latest restore --from qr
```

### Web Application
Meta Secret provides a web interface at https://meta-secret.github.io for users who prefer a graphical interface.

### Mobile Application
The iOS application is available on the App Store: [Meta Secret Mobile Application](https://apps.apple.com/app/metasecret/id1644286751)

## Developer Guide

### Building from Source

1. **Prerequisites**:
   - Rust (latest stable)
   - Cargo
   - Docker (for containerized execution)

2. **Building the Core Library**:
   ```bash
   cd meta-secret
   cargo build --release
   ```

3. **Building the CLI**:
   ```bash
   cd meta-secret/cli
   cargo build --release
   ```

4. **Building for WebAssembly**:
   ```bash
   cd meta-secret/wasm
   wasm-pack build
   ```

### Testing

Run the comprehensive test suite:
```bash
cd meta-secret
cargo test --all-features
```

## Security Considerations

- The security of Meta Secret relies on the shares being stored in different physical or digital locations
- The threshold should be set such that it balances security (higher threshold) with convenience (lower threshold)
- If too many shares are lost, the secret becomes unrecoverable

## License

Meta Secret is licensed under the Apache License 2.0. See the LICENSE file for details.

## Community and Contribution

- **Website**: [meta-secret.org](https://meta-secret.org)
- **GitHub**: [meta-secret/meta-secret-core](https://github.com/meta-secret/meta-secret-core)

## Future Development

- Enhanced support for multiple secret types
- Integration with more decentralized storage solutions
- Advanced key management features
- Multi-device synchronization improvements 