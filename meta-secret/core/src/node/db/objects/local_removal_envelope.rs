//! Registry read seam. Mutation/nonce ownership is implemented only after the
//! independent outbox fault/restart RED batch.
use super::persistent_object::PersistentObject;
use crate::node::common::model::user::common::UserId;
use crate::node::db::descriptors::local_removal_envelope::LocalRemovalEnvelopeDescriptor;
use crate::node::db::descriptors::{
    object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
};
use crate::node::db::events::local_removal_envelope::{
    LocalRemovalEnvelopeEntry, LocalRemovalEnvelopeObject, LocalRemovalEnvelopeRegistry,
    RemovalEnvelopeMode,
};
use crate::node::db::events::{
    generic_log_event::ObjIdExtractor,
    kv_log_event::{KvKey, KvLogEvent},
    object_id::{ArtifactId, Next},
};
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{Result, ensure};
use std::sync::Arc;

pub struct PersistentRemovalEnvelope<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}
impl<Repo: KvLogEventRepo> PersistentRemovalEnvelope<Repo> {
    pub async fn load(&self, user: &UserId) -> Result<Option<LocalRemovalEnvelopeObject>> {
        let object = self
            .p_obj
            .find_tail_event(LocalRemovalEnvelopeDescriptor(user.clone()))
            .await?;
        if let Some(object) = &object {
            ensure!(
                object.0.value.owner == *user,
                "local removal envelope owner mismatch"
            );
            let registry = &object.0.value;
            ensure!(
                registry.first_managed_removal_nonce > 0,
                "invalid managed removal boundary"
            );
            let stream = ArtifactId::from(DeviceLogDescriptor::from(user.clone())).fqdn;
            let mut ids = std::collections::HashSet::new();
            for entry in &registry.entries {
                ensure!(
                    entry.event_id.fqdn == stream
                        && entry.event_id.id.curr as u64 >= registry.first_managed_removal_nonce
                        && ids.insert(&entry.event_id),
                    "invalid local removal envelope identity"
                );
                match entry.mode {
                    RemovalEnvelopeMode::Guarded => ensure!(
                        entry
                            .request_id
                            .as_ref()
                            .is_some_and(|id| crate::node::api::is_liveboard_request_id(id)),
                        "missing or invalid guarded removal reference"
                    ),
                    RemovalEnvelopeMode::Legacy => ensure!(
                        entry.request_id.is_none(),
                        "legacy envelope cannot contain guarded reference"
                    ),
                }
            }
        }
        Ok(object)
    }

    /// Caller holds the shared DeviceLog append owner. Retain the managed
    /// boundary even when its orphan entry is removed; absence is not legacy.
    pub(crate) async fn clear_orphan(&self, user: &UserId, id: &ArtifactId) -> Result<()> {
        let Some(mut object) = self.load(user).await? else {
            return Ok(());
        };
        if !object
            .0
            .value
            .entries
            .iter()
            .any(|entry| &entry.event_id == id)
        {
            return Ok(());
        }
        ensure!(
            self.p_obj.repo.find_one(id.clone()).await?.is_none(),
            "occupied nonce cannot be reclaimed"
        );
        object.0.value.entries.retain(|entry| &entry.event_id != id);
        object.0.key.obj_id = object.obj_id().next();
        self.p_obj.save_verified(object).await?;
        ensure!(
            self.load(user).await?.is_some_and(|record| record
                .0
                .value
                .entries
                .iter()
                .all(|entry| &entry.event_id != id)),
            "orphan ownership cleanup failed"
        );
        Ok(())
    }

    pub(crate) async fn register(
        &self,
        user: &UserId,
        entry: LocalRemovalEnvelopeEntry,
    ) -> Result<()> {
        let previous = self.load(user).await?;
        let descriptor = LocalRemovalEnvelopeDescriptor(user.clone());
        let key = match &previous {
            Some(object) => KvKey::artifact(descriptor.to_obj_desc(), object.obj_id().next()),
            None => KvKey::from(descriptor),
        };
        let mut registry =
            previous
                .map(|object| object.0.value)
                .unwrap_or(LocalRemovalEnvelopeRegistry {
                    owner: user.clone(),
                    first_managed_removal_nonce: entry.event_id.id.curr as u64,
                    entries: vec![],
                    producer_context: None,
                });
        ensure!(
            registry
                .entries
                .iter()
                .all(|existing| existing.event_id != entry.event_id),
            "nonce already has envelope ownership"
        );
        registry.entries.push(entry);
        self.p_obj
            .save_verified(LocalRemovalEnvelopeObject(KvLogEvent {
                key,
                value: registry,
            }))
            .await?;
        Ok(())
    }
}
