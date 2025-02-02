use std::collections::HashMap;
use std::sync::Arc;

use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{
    SsDistributionClaim, SsDistributionClaimDbId, SsDistributionId, SsDistributionStatus, SsLogData,
};
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SharedSecretDescriptor, SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::events::generic_log_event::{KeyExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{
    Next, ObjectId, UnitId, VaultGenesisEvent, VaultUnitEvent,
};
use crate::node::db::events::shared_secret_event::{
    SharedSecretObject, SsDeviceLogObject, SsLogObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{bail, Ok, Result};
use log::info;
use tracing::debug;
use tracing_attributes::instrument;

pub struct PersistentSharedSecret<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    #[instrument(skip(self))]
    pub async fn create_distribution_completion_status(
        &self,
        id: SsDistributionClaimDbId,
    ) -> Result<()> {
        info!("create_distribution_completion_status");

        let desc = SharedSecretDescriptor::SsDistributionStatus(id).to_obj_desc();

        let unit_event = SharedSecretObject::SsDistributionStatus(KvLogEvent {
            key: KvKey::unit(desc),
            value: SsDistributionStatus::Delivered,
        });

        self.p_obj.repo.save(unit_event).await?;
        Ok(())
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub async fn get_ss_distribution_events(
        &self,
        distribution_claim: SsDistributionClaim,
    ) -> Result<Vec<SharedSecretObject>> {
        let mut events = vec![];
        for distribution_id in distribution_claim.distribution_ids() {
            let desc = SharedSecretDescriptor::SsDistribution(distribution_id);
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
    ) -> Result<SharedSecretObject> {
        let desc = SharedSecretDescriptor::SsDistribution(id);

        if let Some(event) = self.p_obj.find_tail_event(desc).await? {
            Ok(event)
        } else {
            bail!("No distribution event found")
        }
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    #[instrument(skip(self))]
    pub async fn save_ss_log_event(&self, claim: SsDistributionClaim) -> Result<()> {
        info!("Saving ss_log event");

        let maybe_ss_log_event = {
            let obj_desc = SsLogDescriptor::from(claim.vault_name.clone());
            self.p_obj.find_tail_event(obj_desc.clone()).await?
        };

        let vault_name = claim.vault_name.clone();

        let new_ss_log_event = match maybe_ss_log_event {
            None => {
                bail!("SsLog not initialized")
            }
            Some(ss_log_event) => match ss_log_event.key() {
                GenericKvKey::UnitKey { .. } => {
                    bail!("Invalid state, expected unit key, it has to be at least genesis")
                }
                GenericKvKey::GenesisKey { .. } => {
                    let ss_log_data = {
                        let mut claims = HashMap::new();
                        claims.insert(claim.id.clone(), claim);
                        SsLogData { claims }
                    };
                    self.create_new_ss_log_claim(ss_log_data, vault_name)
                        .await?
                }
                GenericKvKey::ArtifactKey { .. } => {
                    if let SsLogObject::Claims(ss_log_event) = ss_log_event {
                        let mut new_log_data = ss_log_event.value.clone();
                        new_log_data.claims.insert(claim.id.clone(), claim);

                        self.create_new_ss_log_claim(new_log_data, vault_name)
                            .await?
                    } else {
                        bail!("Invalid SsLog event, the event has to be a claim")
                    }
                }
            },
        };

        self.p_obj.repo.save(new_ss_log_event).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn create_new_ss_log_claim(
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
        let ObjectId::Artifact(free_artifact_id) = free_id else {
            bail!("Invalid ss_log free id: {:?}", free_id);
        };

        Ok(SsLogObject::Claims(KvLogEvent {
            key: KvKey::artifact(obj_desc.to_obj_desc(), free_artifact_id),
            value: ss_log_data,
        }))
    }
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    #[instrument(skip(self))]
    pub async fn save_claim_in_ss_device_log(&self, claim: SsDistributionClaim) -> Result<()> {
        info!("Saving claim in_ss_device_log");

        let obj_desc = SsDeviceLogDescriptor::from(claim.sender.clone());
        let free_id = self
            .p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;
        let ObjectId::Artifact(free_artifact_id) = free_id else {
            bail!("Invalid ss_device_log free id: {:?}", free_id);
        };

        let obj = SsDeviceLogObject::Claim(KvLogEvent {
            key: KvKey::artifact(obj_desc.to_obj_desc(), free_artifact_id),
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
    ) -> Result<Option<ObjectId>> {
        let obj_desc = SsDeviceLogDescriptor::from(device_id.clone());
        self.p_obj.find_tail_id_by_obj_desc(obj_desc).await
    }

    pub async fn get_ss_log_obj(&self, vault_name: VaultName) -> Result<SsLogData> {
        let obj_desc = SsLogDescriptor::from(vault_name);
        let maybe_log_event = self.p_obj.find_tail_event(obj_desc).await?;

        let log_event = maybe_log_event
            .map(|ss_log_event| ss_log_event.to_data())
            .unwrap_or_else(|| SsLogData::empty());

        Ok(log_event)
    }

    #[instrument(skip(self))]
    pub async fn init(&self, user: UserData) -> Result<()> {
        self.init_ss_device_log(user.clone()).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn init_ss_device_log(&self, user: UserData) -> Result<()> {
        let user_id = user.user_id();
        let obj_desc = SsDeviceLogDescriptor::from(user_id.device_id);
        let unit_id = UnitId::unit_from_desc(obj_desc.clone());

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("SsDeviceLog already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit_from(obj_desc);
        let unit_event = SsDeviceLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: unit_key.clone(),
            value: user_id.vault_name.clone(),
        }));

        self.p_obj.repo.save(unit_event).await?;

        let genesis_key = unit_key.next();
        let genesis_event = SsDeviceLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: genesis_key,
            value: user.clone(),
        }));
        self.p_obj.repo.save(genesis_event).await?;

        Ok(())
    }
}

#[cfg(test)]
pub mod spec {
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::descriptors::shared_secret_descriptor::{
        SharedSecretDescriptor, SsDeviceLogDescriptor,
    };
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::{bail, Result};
    use log::info;
    use std::sync::Arc;
    use tracing_attributes::instrument;

    pub struct SsDeviceLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub client_user: UserData,
    }

    impl<Repo: KvLogEventRepo> SsDeviceLogSpec<Repo> {
        #[instrument(skip(self))]
        pub async fn check_initialization(&self) -> Result<()> {
            info!("SSDeviceLogSpec check_initialization");

            let device_id = self.client_user.device.device_id.clone();
            let ss_obj_desc = SsDeviceLogDescriptor::from(device_id);

            let ss_unit_id = ObjectId::unit_from(ss_obj_desc.clone());
            let ss_genesis_id = ObjectId::genesis(ss_obj_desc.to_obj_desc());

            let maybe_unit_event = self.p_obj.repo.find_one(ss_unit_id).await?;

            if let Some(unit_event) = maybe_unit_event {
                let vault_name = unit_event.ss_device_log()?.get_unit()?.vault_name();
                assert_eq!(vault_name, self.client_user.vault_name());
            } else {
                bail!("SSDevice, unit event not found");
            }

            let maybe_genesis_event = self.p_obj.repo.find_one(ss_genesis_id).await?;

            if let Some(genesis_event) = maybe_genesis_event {
                let user = genesis_event.ss_device_log()?.get_genesis()?.user();
                assert_eq!(user, self.client_user);
            } else {
                bail!("SSDevice, genesis event not found");
            }

            Ok(())
        }
    }
}
