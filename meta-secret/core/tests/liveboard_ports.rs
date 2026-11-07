//! Independent real-record tests for the additive Liveboard service contract.
#[path = "liveboard_ports/checked_identity.rs"]
mod checked_identity;
#[path = "liveboard_ports/exact_accept.rs"]
mod exact_accept;
#[path = "liveboard_ports/exact_join.rs"]
mod exact_join;
#[path = "liveboard_ports/fault_repo.rs"]
mod fault_repo;
#[path = "liveboard_ports/fixtures.rs"]
mod fixtures;
#[path = "liveboard_ports/projection.rs"]
mod projection;
#[path = "liveboard_ports/projection_material.rs"]
mod projection_material;
#[path = "liveboard_ports/projection_operations.rs"]
mod projection_operations;
#[path = "guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
