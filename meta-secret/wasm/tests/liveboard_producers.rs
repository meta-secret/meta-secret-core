//! Actual manager→service FIFO→signed ServerApp tests; no fake business responses.
#![cfg(not(target_arch = "wasm32"))]
#[path = "liveboard_producers/composite.rs"]
mod composite;
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_producers/fixture.rs"]
mod fixture;
#[path = "liveboard_producers/messaging.rs"]
mod messaging;
#[path = "liveboard_producers/privacy.rs"]
mod privacy;
#[path = "liveboard_producers/protocol.rs"]
mod protocol;
#[path = "liveboard_producers/receipts.rs"]
mod receipts;
#[path = "liveboard_producers/reconcile.rs"]
mod reconcile;
#[path = "liveboard_producers/repo.rs"]
mod repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod seed_fixture;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
