//! Host-side real Core fixture parity. This is not a browser or native UI E2E.
#![cfg(not(target_arch = "wasm32"))]
#[path = "liveboard_bindings/decisions.rs"]
mod decisions;
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_bindings/managers.rs"]
mod managers;
#[path = "liveboard_bindings/messaging.rs"]
mod messaging;
#[path = "liveboard_bindings/native_json.rs"]
mod native_json;
#[path = "liveboard_bindings/snapshot.rs"]
mod snapshot;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
