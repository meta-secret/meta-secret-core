//! Strict additive manager controls; the established binding19 target stays unchanged.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
#[path = "liveboard_bindings/unknown_recovery.rs"]
mod unknown_recovery;
