//! Local legacy publication proofs; direct domain processing is not HTTP/UI E2E.
mod fixture;
mod lifecycle;
mod owner_interval;
mod proof;
mod repo;

use super::*;
use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::SsDistributionStatus;
use crate::node::db::actions::vault::vault_action::ServerVaultAction;
use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::vault::liveboard_metadata::RemovalApplicationState;
use crate::node::db::events::vault::vault_log_event::{
    CreateVaultEvent, RemoveMemberDecision, VaultActionEvent, VaultActionInitEvent,
};
use crate::node::db::in_mem_db::InMemKvLogEventRepo;
use crate::node::db::repo::generic_db::{
    DbCleanUpCommand, DeleteCommand, FindOneQuery, SaveCommand,
};
use crate::secret::MetaDistributor;
use anyhow::{Context, ensure};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const SECRET: &str = "legacy-owner-approver-private-fixture";

fn descriptor(pass: &MetaPasswordId, receiver: &DeviceId) -> SsWorkflowDescriptor {
    SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: receiver.clone(),
    })
}

async fn packet<R: KvLogEventRepo>(
    p: &Arc<PersistentObject<R>>,
    pass: &MetaPasswordId,
    receiver: &DeviceId,
) -> Result<SsWorkflowObject> {
    p.find_tail_event(descriptor(pass, receiver))
        .await?
        .context("actual distribution is missing")
}
