use std::sync::Arc;

use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{
    SsClaim, SsDistributionId, SsDistributionStatus, SsLogData,
};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SsDeviceLogDescriptor, SsLogDescriptor, SsWorkflowDescriptor,
};
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::{
    SsDeviceLogObject, SsLogObject, SsWorkflowObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{bail, Ok, Result};
use derive_more::From;
use log::info;
use tracing_attributes::instrument;

#[derive(From)]
pub struct PersistentSharedSecret<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub async fn get_ss_workflow_events(&self, ss_claim: SsClaim) -> Result<Vec<SsWorkflowObject>> {
        let mut events = vec![];
        for distribution_id in ss_claim.distribution_ids() {
            let desc = SsWorkflowDescriptor::Distribution(distribution_id);
            let tail_event = self.p_obj.find_tail_event(desc).await?;
            if let Some(event) = tail_event {
                events.push(event);
            }
        }

        // Synchronize claims (recovery requests)
        for claim_id in ss_claim.claim_db_ids() {
            let claim_id_desc = SsWorkflowDescriptor::Recovery(claim_id);
            let tail_event = self.p_obj.find_tail_event(claim_id_desc).await?;
            if let Some(event) = tail_event {
                events.push(event);
            }
        }

        Ok(events)
    }

    pub async fn get_ss_distribution_event_by_id(
        &self,
        id: SsDistributionId,
    ) -> Result<SsWorkflowObject> {
        let desc = SsWorkflowDescriptor::Distribution(id.clone());

        if let Some(event) = self.p_obj.find_tail_event(desc).await? {
            Ok(event)
        } else {
            bail!("Distribution event not found: {:?}", id)
        }
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    #[instrument(skip(self))]
    pub async fn find_ss_log_tail_event(
        &self,
        vault_name: VaultName,
    ) -> Result<Option<SsLogObject>> {
        let obj_desc = SsLogDescriptor::from(vault_name);
        self.p_obj.find_tail_event(obj_desc).await
    }

    #[instrument(skip(self))]
    pub async fn save_ss_log_event(&self, claim: SsClaim) -> Result<()> {
        info!("Saving ss_log event");

        let vault_name = claim.vault_name.clone();

        let maybe_ss_log_event = self.find_ss_log_tail_event(vault_name.clone()).await?;

        let new_ss_log_event = match maybe_ss_log_event {
            None => {
                let ss_log_data = SsLogData::new(claim);
                self.create_new_ss_log_object(ss_log_data, vault_name)
                    .await?
            }
            Some(ss_log_event) => {
                let new_log_data = ss_log_event.0.value.insert(claim);
                self.create_new_ss_log_object(new_log_data, vault_name)
                    .await?
            }
        };

        self.p_obj.repo.save(new_ss_log_event).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_new_ss_log_object(
        &self,
        ss_log_data: SsLogData,
        vault_name: VaultName,
    ) -> Result<SsLogObject> {
        info!("Creating new ss_log_claim");

        let obj_desc = SsLogDescriptor::from(vault_name);

        let free_id = self
            .p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;

        Ok(SsLogObject(KvLogEvent {
            key: KvKey::artifact(obj_desc.to_obj_desc(), free_id),
            value: ss_log_data,
        }))
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    #[instrument(skip(self))]
    pub async fn save_claim_in_ss_device_log(&self, claim: SsClaim) -> Result<()> {
        info!("Saving claim in_ss_device_log");

        let obj_desc = SsDeviceLogDescriptor::from(claim.sender.clone());
        let free_id = self
            .p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;

        let obj = SsDeviceLogObject(KvLogEvent {
            key: KvKey::artifact(obj_desc.to_obj_desc(), free_id),
            value: claim,
        });

        self.p_obj.repo.save(obj.to_generic()).await?;

        Ok(())
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub async fn find_ss_device_log_tail_id(
        &self,
        device_id: &DeviceId,
    ) -> Result<Option<ArtifactId>> {
        let obj_desc = SsDeviceLogDescriptor::from(device_id.clone());
        self.p_obj.find_tail_id_by_obj_desc(obj_desc).await
    }

    pub async fn get_ss_log_obj(&self, vault_name: VaultName) -> Result<SsLogData> {
        let obj_desc = SsLogDescriptor::from(vault_name);
        let maybe_log_event = self.p_obj.find_tail_event(obj_desc).await?;

        let log_event = maybe_log_event
            .map(|ss_log_event| ss_log_event.to_data())
            .unwrap_or_else(SsLogData::default);

        Ok(log_event)
    }
}
