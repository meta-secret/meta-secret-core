//! Retained admission/receipt-codec controls; current private/publication fault gates
//! live in guarded_lifecycle_c. Original16 intent mapping/preimages are archived.
#[path = "liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "guarded_preparation/receipts.rs"]
mod receipts;
#[path = "guarded_preparation/tests.rs"]
mod tests;
#[path = "guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
