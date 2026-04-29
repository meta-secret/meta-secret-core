pub mod server;

/// Compile-time package version of this crate (`Cargo.toml` / `VERSIONS.md`).
pub const CRATE_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
