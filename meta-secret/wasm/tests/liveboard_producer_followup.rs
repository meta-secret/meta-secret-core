//! Independent exact material and terminal-context regressions through actual host bindings.
#![cfg(not(target_arch = "wasm32"))]
#[path = "liveboard_producer_followup/context_repo.rs"]
mod context_repo;
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_producers/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "liveboard_producer_followup/fresh_observation.rs"]
mod fresh_observation;
#[path = "liveboard_producer_followup/healthy.rs"]
mod healthy;
#[path = "liveboard_producer_followup/material.rs"]
mod material;
#[path = "liveboard_producers/protocol.rs"]
mod protocol;
#[path = "liveboard_producers/repo.rs"]
#[allow(dead_code)]
mod repo;
#[path = "liveboard_producer_followup/reservation.rs"]
mod reservation;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod seed_fixture;
#[path = "liveboard_producer_followup/terminal.rs"]
mod terminal;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

#[path = "liveboard_producer_followup/known_identity.rs"]
mod known_identity;

#[path = "liveboard_producer_followup/supplied_u.rs"]
mod supplied_u;
