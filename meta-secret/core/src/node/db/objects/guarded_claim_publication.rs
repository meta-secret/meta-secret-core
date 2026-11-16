//! Resolve the exact intended claim before allocating any ordinary stream key.
use super::{
    local_guarded_preparation::UNAVAILABLE, persistent_shared_secret::PersistentSharedSecret,
};
use crate::node::{
    common::model::secret::SsClaim,
    db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        events::{
            generic_log_event::{ObjIdExtractor, ToGenericEvent},
            object_id::ArtifactId,
            shared_secret_event::SsDeviceLogObject,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

pub(super) fn same_stable_claim(actual: &SsClaim, intended: &SsClaim) -> bool {
    let mut stable = actual.clone();
    stable.status = intended.status.clone();
    stable.client_status = intended.client_status.clone();
    stable == *intended
}

impl<R: KvLogEventRepo> PersistentSharedSecret<R> {
    pub(super) async fn publish_guarded_claim(
        &self,
        claim: &SsClaim,
    ) -> Result<(ArtifactId, ArtifactId)> {
        let _owner = self.p_obj.device_log_append_guard.lock().await;
        let history: Vec<SsDeviceLogObject> = self
            .p_obj
            .get_object_events_from_beginning(SsDeviceLogDescriptor::from(claim.sender.clone()))
            .await?;
        let mut matching = history.iter().filter(|event| event.0.value.id == claim.id);
        let device_id = if let Some(event) = matching.next() {
            ensure!(
                matching.next().is_none() && event.0.value == *claim,
                UNAVAILABLE
            );
            event.obj_id()
        } else {
            self.append_claim_owned(claim.clone()).await?
        };
        let current = self
            .find_ss_log_tail_event(claim.vault_name.clone())
            .await?;
        if let Some(event) = &current
            && let Some(actual) = event.0.value.claims.get(&claim.id)
        {
            ensure!(same_stable_claim(actual, claim), UNAVAILABLE);
            return Ok((device_id, event.obj_id()));
        }
        let data = current
            .map(|event| event.0.value)
            .unwrap_or_default()
            .insert(claim.clone());
        let intended = self
            .create_new_ss_log_object(data, claim.vault_name.clone())
            .await?;
        let id = intended.obj_id();
        let saved = self.p_obj.repo.save(intended.clone()).await;
        ensure!(
            self.p_obj.repo.find_one(id.clone()).await? == Some(intended.to_generic()),
            UNAVAILABLE
        );
        if let Ok(saved) = saved {
            ensure!(saved == id, UNAVAILABLE);
        }
        Ok((device_id, id))
    }
}
