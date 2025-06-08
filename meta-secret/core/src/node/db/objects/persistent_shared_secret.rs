use std::sync::Arc;

use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SsClaim, SsDistributionId, SsLogData};
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
use tracing::info;
use tracing_attributes::instrument;

#[derive(From)]
pub struct PersistentSharedSecret<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub async fn get_ss_workflow_events(&self, ss_claim: SsClaim) -> Result<Vec<SsWorkflowObject>> {
        let mut events = vec![];

        let distributions = self.get_distributions(ss_claim.clone()).await?;
        events.extend(distributions);

        // Synchronize claims (recovery requests)
        let recoveries = self.get_recoveries(ss_claim).await?;
        events.extend(recoveries);

        Ok(events)
    }

    pub async fn get_recoveries(&self, ss_claim: SsClaim) -> Result<Vec<SsWorkflowObject>> {
        let mut events = vec![];
        // Synchronize claims (recovery requests)
        for claim_id in ss_claim.recovery_db_ids() {
            let claim_id_desc = SsWorkflowDescriptor::Recovery(claim_id);
            let tail_event = self.p_obj.find_tail_event(claim_id_desc).await?;
            if let Some(event) = tail_event {
                events.push(event);
            }
        }

        Ok(events)
    }

    pub async fn get_distributions(&self, ss_claim: SsClaim) -> Result<Vec<SsWorkflowObject>> {
        let mut events = vec![];
        for distribution_id in ss_claim.distribution_ids() {
            let desc = SsWorkflowDescriptor::Distribution(distribution_id);
            let tail_event = self.p_obj.find_tail_event(desc).await?;
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
    pub async fn find_ss_device_log_tail_event(
        &self,
        device_id: DeviceId,
    ) -> Result<Option<SsDeviceLogObject>> {
        let obj_desc = SsDeviceLogDescriptor::from(device_id);
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
        let obj_desc = SsLogDescriptor::from(vault_name.clone());
        let maybe_log_event = self.p_obj.find_tail_event(obj_desc).await?;

        let log_event = maybe_log_event
            .map(|ss_log_event| ss_log_event.to_data())
            .unwrap_or_else(|| SsLogData::from(vault_name));

        Ok(log_event)
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::SecretDistributionType;
    use crate::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor;
    use crate::node::db::events::shared_secret_event::SsDeviceLogObject;
    use anyhow::Result;

    #[tokio::test]
    async fn test_save_claim_in_ss_device_log() -> Result<()> {
        // Setup
        let registry = FixtureRegistry::empty();
        let p_obj = registry.state.p_obj.client;
        let persistent_ss = super::PersistentSharedSecret {
            p_obj: p_obj.clone(),
        };

        // Get vault member from the fixture and create a claim using its method
        let vault_member = registry.state.vault_data.client_vault_member;
        let pass_id = MetaPasswordId::build_from_str("test_password");

        // Create a claim using the VaultMember helper (which properly creates receivers list)
        let test_claim = vault_member.create_split_claim(pass_id);

        // Execute function being tested
        persistent_ss
            .save_claim_in_ss_device_log(test_claim.clone())
            .await?;

        // Verify results
        let sender_device_id = test_claim.sender.clone();
        let obj_desc = SsDeviceLogDescriptor::from(sender_device_id);
        let tail_id = p_obj.find_tail_id_by_obj_desc(obj_desc.clone()).await?;

        assert!(
            tail_id.is_some(),
            "Expected to find tail ID but none was found"
        );

        let event = p_obj
            .find_tail_event_by_obj_id::<SsDeviceLogObject>(tail_id.unwrap())
            .await?;
        assert!(event.is_some(), "Expected to find event but none was found");

        let ss_device_log_obj = event.unwrap();
        let saved_claim = ss_device_log_obj.to_distribution_request();

        // Verify the claim properties
        assert_eq!(saved_claim.id, test_claim.id, "Claim IDs do not match");
        assert_eq!(
            saved_claim.vault_name, test_claim.vault_name,
            "Vault names do not match"
        );
        assert_eq!(
            saved_claim.distribution_type,
            SecretDistributionType::Split,
            "Distribution types do not match"
        );
        assert_eq!(
            saved_claim.sender, test_claim.sender,
            "Sender does not match"
        );

        // Verify receivers
        assert_eq!(saved_claim.receivers.len(), 2, "Should have 2 receivers");
        assert!(
            saved_claim
                .receivers
                .contains(&registry.state.vault_data.client_b_membership.device_id()),
            "Client B should be a receiver"
        );
        assert!(
            saved_claim
                .receivers
                .contains(&registry.state.vault_data.vd_membership.device_id()),
            "VD should be a receiver"
        );

        // Verify receiver statuses
        for receiver in saved_claim.receivers.iter() {
            let status = saved_claim.status.get(receiver);
            assert!(status.is_some(), "Receiver should have a status");
            assert_eq!(
                *status.unwrap(),
                crate::node::common::model::secret::SsDistributionStatus::Pending,
                "Status should be Pending"
            );
        }

        Ok(())
    }
}
