//! Independent source-review regressions; no product implementation is owned here.
#[path = "liveboard_ports_security/confirmation.rs"]
mod confirmation;
#[path = "liveboard_ports_security/confirmation_fixture.rs"]
mod confirmation_fixture;
#[path = "liveboard_ports_security/confirmation_repeat.rs"]
mod confirmation_repeat;
#[path = "liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_ports_security/history.rs"]
mod history;
#[path = "liveboard_ports_security/history_controls.rs"]
mod history_controls;
#[path = "liveboard_ports_security/projection.rs"]
mod projection;
#[path = "liveboard_ports_security/receiver_identity.rs"]
mod receiver_identity;
#[path = "liveboard_ports_security/recovery.rs"]
mod recovery;
#[path = "guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
