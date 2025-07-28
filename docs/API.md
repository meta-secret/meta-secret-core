# Meta Secret Core API Documentation

This document provides comprehensive API documentation for the Meta Secret core library.

## Table of Contents

- [Overview](#overview)
- [Core Functions](#core-functions)
- [Data Types](#data-types)
- [Error Handling](#error-handling)
- [Usage Examples](#usage-examples)
- [Advanced Usage](#advanced-usage)
- [Platform Integration](#platform-integration)

## Overview

The Meta Secret core library provides a simple yet powerful API for secret splitting and recovery using Shamir's Secret Sharing algorithm. The main entry points are:

- `split()` - Split a secret into multiple shares
- `recover()` - Recover a secret from available shares
- `recover_from_shares()` - Recover from explicitly provided shares

## Core Functions

### `split(secret: String, config: SharedSecretConfig) -> CoreResult<()>`

Splits a secret into multiple shares using Shamir's Secret Sharing algorithm.

**Parameters:**
- `secret: String` - The secret to be split
- `config: SharedSecretConfig` - Configuration specifying the number of shares and threshold

**Returns:**
- `CoreResult<()>` - Success or error result

**Side Effects:**
- Creates a `secrets/` directory in the current working directory
- Generates JSON files: `shared-secret-{index}.json`
- Generates QR code files: `shared-secret-{index}.png`

**Example:**
```rust
use meta_secret_core::{split, SharedSecretConfig};

let secret = "my_super_secret_password".to_string();
let config = SharedSecretConfig {
    number_of_shares: 5,
    threshold: 3,
};

match split(secret, config) {
    Ok(()) => println!("Secret split successfully!"),
    Err(e) => eprintln!("Error splitting secret: {}", e),
}
```

### `recover() -> CoreResult<PlainText>`

Recovers a secret from JSON share files in the `secrets/` directory.

**Returns:**
- `CoreResult<PlainText>` - The recovered secret or error

**Prerequisites:**
- `secrets/` directory must exist
- Must contain sufficient JSON share files to meet the threshold

**Example:**
```rust
use meta_secret_core::recover;

match recover() {
    Ok(secret) => println!("Recovered secret: {}", secret.as_str()),
    Err(e) => eprintln!("Error recovering secret: {}", e),
}
```

### `recover_from_shares(users_shares: Vec<UserShareDto>) -> CoreResult<PlainText>`

Recovers a secret from explicitly provided share data.

**Parameters:**
- `users_shares: Vec<UserShareDto>` - Vector of user share objects

**Returns:**
- `CoreResult<PlainText>` - The recovered secret or error

**Example:**
```rust
use meta_secret_core::{recover_from_shares, UserShareDto};

let shares: Vec<UserShareDto> = load_shares_from_somewhere();

match recover_from_shares(shares) {
    Ok(secret) => println!("Recovered: {}", secret.as_str()),
    Err(e) => eprintln!("Recovery failed: {}", e),
}
```

### `generate_qr_code(data: &str, path: &str)`

Generates a QR code image from text data.

**Parameters:**
- `data: &str` - The data to encode in the QR code
- `path: &str` - File path where the QR code image will be saved

**Example:**
```rust
use meta_secret_core::generate_qr_code;

let json_data = r#"{"share_id": 0, "share_blocks": [...]}"#;
generate_qr_code(json_data, "my_share.png");
```

### `read_qr_code(path: &Path) -> Result<String, QrCodeParserError>`

Reads and decodes a QR code image file.

**Parameters:**
- `path: &Path` - Path to the QR code image file

**Returns:**
- `Result<String, QrCodeParserError>` - Decoded text data or error

**Example:**
```rust
use meta_secret_core::read_qr_code;
use std::path::Path;

let qr_path = Path::new("my_share.png");
match read_qr_code(qr_path) {
    Ok(data) => println!("QR code contains: {}", data),
    Err(e) => eprintln!("Failed to read QR code: {}", e),
}
```

### `convert_qr_images_to_json_files() -> Result<Vec<String>, QrToJsonParserError>`

Converts all PNG QR code files in the `secrets/` directory to JSON files.

**Returns:**
- `Result<Vec<String>, QrToJsonParserError>` - Vector of JSON strings or error

**Example:**
```rust
use meta_secret_core::convert_qr_images_to_json_files;

match convert_qr_images_to_json_files() {
    Ok(json_shares) => {
        println!("Converted {} QR codes to JSON", json_shares.len());
    },
    Err(e) => eprintln!("Conversion failed: {}", e),
}
```

## Data Types

### `SharedSecretConfig`

Configuration for secret splitting operations.

```rust
#[derive(Debug, Clone, Copy)]
pub struct SharedSecretConfig {
    pub number_of_shares: u8,
    pub threshold: u8,
}
```

**Fields:**
- `number_of_shares` - Total number of shares to generate (1-255)
- `threshold` - Minimum number of shares needed for recovery (1-255)

**Constraints:**
- `threshold <= number_of_shares`
- `threshold >= 1`
- `number_of_shares >= 1`

### `UserShareDto`

Represents a user's share of a secret.

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct UserShareDto {
    pub share_id: u8,
    pub share_blocks: Vec<ShareBlock>,
}
```

**Fields:**
- `share_id` - Unique identifier for this share (0-254)
- `share_blocks` - Vector of encrypted data blocks

### `PlainText`

Wrapper for recovered secret text.

```rust
pub struct PlainText(String);

impl PlainText {
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn into_string(self) -> String { self.0 }
}

impl From<String> for PlainText {
    fn from(s: String) -> Self { PlainText(s) }
}
```

### `CoreResult<T>`

Type alias for the library's result type.

```rust
pub type CoreResult<T> = Result<T, CoreError>;
```

## Error Handling

The library uses a comprehensive error system with specific error types for different failure modes.

### `CoreError`

Main error type that wraps all possible errors.

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    SplitError(#[from] SplitError),
    
    #[error(transparent)]
    RecoveryError { source: RecoveryError },
    
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    
    // ... other error variants
}
```

### `RecoveryError`

Errors specific to secret recovery operations.

```rust
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("Empty input: {0}")]
    EmptyInput(String),
    
    #[error("Insufficient shares for recovery")]
    InsufficientShares,
    
    #[error("Invalid share format")]
    InvalidShareFormat,
    
    // ... other variants
}
```

### `SplitError`

Errors that can occur during secret splitting.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SplitError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    // ... other variants
}
```

## Usage Examples

### Basic Secret Splitting and Recovery

```rust
use meta_secret_core::{split, recover, SharedSecretConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Split a secret
    let secret = "password123".to_string();
    let config = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };
    
    split(secret.clone(), config)?;
    println!("Secret split into 3 shares (2 needed for recovery)");
    
    // Recover the secret
    let recovered = recover()?;
    assert_eq!(recovered.as_str(), "password123");
    println!("Secret recovered successfully!");
    
    Ok(())
}
```

### Working with QR Codes

```rust
use meta_secret_core::{
    split, convert_qr_images_to_json_files, recover, 
    SharedSecretConfig
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Split secret (creates both JSON and QR files)
    let config = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };
    split("my_secret".to_string(), config)?;
    
    // Remove JSON files to simulate having only QR codes
    std::fs::remove_file("secrets/shared-secret-0.json")?;
    std::fs::remove_file("secrets/shared-secret-1.json")?;
    std::fs::remove_file("secrets/shared-secret-2.json")?;
    
    // Convert QR codes back to JSON
    let json_shares = convert_qr_images_to_json_files()?;
    println!("Converted {} QR codes", json_shares.len());
    
    // Now recover from the converted JSON files
    let recovered = recover()?;
    println!("Recovered from QR codes: {}", recovered.as_str());
    
    Ok(())
}
```

### Manual Share Management

```rust
use meta_secret_core::{recover_from_shares, UserShareDto};
use std::fs;

fn load_shares_manually() -> Result<(), Box<dyn std::error::Error>> {
    // Load shares from specific files
    let mut shares = Vec::new();
    
    for i in 0..2 {  // Load first 2 shares
        let filename = format!("secrets/shared-secret-{}.json", i);
        let json_data = fs::read_to_string(filename)?;
        let share: UserShareDto = serde_json::from_str(&json_data)?;
        shares.push(share);
    }
    
    // Recover using specific shares
    let recovered = recover_from_shares(shares)?;
    println!("Recovered using manual share loading: {}", recovered.as_str());
    
    Ok(())
}
```

### Error Handling Example

```rust
use meta_secret_core::{recover, CoreError, RecoveryError};

fn handle_recovery_errors() {
    match recover() {
        Ok(secret) => {
            println!("Successfully recovered: {}", secret.as_str());
        },
        Err(CoreError::RecoveryError { source }) => {
            match source {
                RecoveryError::EmptyInput(msg) => {
                    eprintln!("No shares found: {}", msg);
                },
                RecoveryError::InsufficientShares => {
                    eprintln!("Not enough shares to recover the secret");
                },
                _ => {
                    eprintln!("Recovery error: {}", source);
                }
            }
        },
        Err(other) => {
            eprintln!("Other error: {}", other);
        }
    }
}
```

## Advanced Usage

### Custom Share Configuration

```rust
use meta_secret_core::{split, SharedSecretConfig};

// High security: 7 shares, need 4 to recover
let high_security = SharedSecretConfig {
    number_of_shares: 7,
    threshold: 4,
};

// Quick backup: 3 shares, need 2 to recover
let quick_backup = SharedSecretConfig {
    number_of_shares: 3,
    threshold: 2,
};

// Maximum security: 10 shares, need 6 to recover
let max_security = SharedSecretConfig {
    number_of_shares: 10,
    threshold: 6,
};
```

### Batch Processing

```rust
use meta_secret_core::{split, recover_from_shares, SharedSecretConfig};

fn process_multiple_secrets() -> Result<(), Box<dyn std::error::Error>> {
    let secrets = vec![
        "password1".to_string(),
        "password2".to_string(),
        "password3".to_string(),
    ];
    
    let config = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };
    
    for (i, secret) in secrets.iter().enumerate() {
        // Use different directories for each secret
        std::env::set_current_dir(format!("secret_{}", i))?;
        std::fs::create_dir_all(".")?;
        
        split(secret.clone(), config)?;
        println!("Split secret {}: {}", i, secret);
    }
    
    Ok(())
}
```

### Integration with Storage Systems

```rust
use meta_secret_core::{UserShareDto, recover_from_shares};
use std::collections::HashMap;

// Example: Loading shares from a database or network
struct ShareStorage {
    shares: HashMap<String, Vec<UserShareDto>>,
}

impl ShareStorage {
    fn load_shares(&self, secret_id: &str) -> Option<&Vec<UserShareDto>> {
        self.shares.get(secret_id)
    }
    
    fn recover_secret(&self, secret_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        let shares = self.load_shares(secret_id)
            .ok_or("Secret not found")?;
        
        let recovered = recover_from_shares(shares.clone())?;
        Ok(recovered.into_string())
    }
}
```

## Platform Integration

### WebAssembly (WASM) Bindings

The core functionality is available in WebAssembly for browser integration:

```javascript
import init, { split_secret, recover_secret } from './pkg/meta_secret_wasm.js';

// Initialize the WASM module
await init();

// Split a secret
const config = { number_of_shares: 3, threshold: 2 };
const shares = split_secret("my_secret", config);

// Recover the secret
const recovered = recover_secret(shares);
console.log("Recovered:", recovered);
```

### Mobile Integration

For mobile platforms, the core library is wrapped with platform-specific bindings:

**iOS (Swift):**
```swift
import MetaSecretCore

let config = SharedSecretConfig(numberOfShares: 3, threshold: 2)
let result = MetaSecret.split(secret: "my_secret", config: config)
```

**Android (Kotlin):**
```kotlin
import com.metasecret.core.MetaSecret
import com.metasecret.core.SharedSecretConfig

val config = SharedSecretConfig(numberOfShares = 3, threshold = 2)
val result = MetaSecret.split("my_secret", config)
```

## Thread Safety

The core library is thread-safe for read operations but requires external synchronization for write operations that modify the file system:

```rust
use meta_secret_core::{split, recover, SharedSecretConfig};
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref SPLIT_MUTEX: Mutex<()> = Mutex::new(());
}

fn thread_safe_split(secret: String, config: SharedSecretConfig) -> meta_secret_core::CoreResult<()> {
    let _lock = SPLIT_MUTEX.lock().unwrap();
    split(secret, config)
}
```

## Performance Considerations

- **Memory Usage**: The library has minimal memory overhead, typically <1MB
- **CPU Usage**: Cryptographic operations are optimized for performance
- **File I/O**: Operations are synchronous; consider async wrappers for UI applications
- **QR Code Generation**: Can be CPU-intensive for large secrets

For high-performance applications, consider:
- Pre-computing shares during off-peak times
- Caching frequently accessed shares
- Using async wrappers for non-blocking operations

This API documentation covers the core functionality of Meta Secret. For platform-specific integrations and advanced cryptographic details, refer to the respective module documentation.