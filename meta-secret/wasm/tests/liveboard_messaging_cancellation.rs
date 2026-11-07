//! Actual manager/service cancellation regression; isolated host records, no UI E2E.
#![cfg(not(target_arch = "wasm32"))]
#[path = "liveboard_messaging_cancellation/cases.rs"]
mod cases;
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_messaging_cancellation/repo.rs"]
mod repo;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
