//! Host/in-process Core recovery contract. This is not real Core UI E2E.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_producers/protocol.rs"]
#[allow(dead_code)]
mod protocol;
#[path = "liveboard_reveal_context/recovery.rs"]
mod recovery;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
