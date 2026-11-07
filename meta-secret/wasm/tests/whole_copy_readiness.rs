//! Independent Stage5a: real signed membership + readonly private-source instrumentation.
#![cfg(not(target_arch = "wasm32"))]
pub use meta_secret_core::node;
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_producers/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "whole_copy_readiness/legacy.rs"]
mod legacy;
#[path = "whole_copy_readiness/lifecycle.rs"]
mod lifecycle;
#[path = "../../core/src/node/app/meta_app/liveboard_material.rs"]
#[expect(
    dead_code,
    reason = "LocalMaterial.source is consumed by projection, outside this legacy fixture"
)]
mod liveboard_material;
#[path = "whole_copy_readiness/observed_repo.rs"]
mod observed_repo;
#[path = "liveboard_producers/protocol.rs"]
mod protocol;
#[path = "liveboard_producers/repo.rs"]
#[allow(dead_code)]
mod repo;
#[path = "whole_copy_readiness/secret.rs"]
pub mod secret;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod seed_fixture;
#[path = "whole_copy_readiness/support.rs"]
mod support;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;
