//! Real local outbox paths; no mocked cryptographic/domain acceptance.
#[path = "guarded_removal_outbox/append_ownership.rs"]
mod append_ownership;
#[path = "guarded_removal_outbox/fixtures.rs"]
mod fixtures;
#[path = "guarded_removal_outbox/producer.rs"]
mod producer;
#[path = "guarded_removal_outbox/selector.rs"]
mod selector;
