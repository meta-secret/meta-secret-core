//! C1 source-only draft: actual signed ServerApp, no simulated Business success.
#![cfg(not(target_arch = "wasm32"))]
#[path = "guarded_lifecycle_c/fixture.rs"]
mod fixture;
#[path = "guarded_lifecycle_c/network.rs"]
mod network;
#[path = "guarded_lifecycle_c/private_storage/mod.rs"]
mod private_storage;
#[path = "guarded_lifecycle_c/staging.rs"]
mod staging;
