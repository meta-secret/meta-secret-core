//! Independent C1b: actual public producer + storage observations, no helper stub.
mod accepted_promotion;
mod codec;
mod faults;
mod immutable_stage;
mod index;
mod persistence;
mod registry_resume;
mod repo;
mod stage;
mod support;

use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    api::*,
    app::sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    db::{
        descriptors::local_guarded_preparation::*,
        events::{
            generic_log_event::*,
            kv_log_event::*,
            local_guarded_preparation::*,
            object_id::{ArtifactId, Next},
        },
        in_mem_db::InMemKvLogEventRepo,
        objects::{persistent_object::PersistentObject, persistent_vault::PersistentVault},
        repo::generic_db::*,
    },
};
use std::sync::{Arc, atomic::Ordering};
