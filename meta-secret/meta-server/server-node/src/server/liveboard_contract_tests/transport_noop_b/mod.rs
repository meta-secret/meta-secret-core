//! Layer B: actual wire/storage/reader boundaries; no business handler replacement.
mod business;
mod races;
mod raw;
mod reads;
mod recovery;
mod repository;
mod storage;
mod support;
mod workflow;
mod workflow_probe;

use super::{integration_binding_fixture::Seed, *};
use meta_secret_core::node::{
    common::model::IdString,
    db::{events::generic_log_event::GenericKvLogEvent, repo::generic_db::*},
    device_transport_noop::DeviceTransportNoop,
};
use serde_json::json;
