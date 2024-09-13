use std::sync::Arc;

use anyhow::anyhow;
use tracing_attributes::instrument;

use crate::node::common::model::device::{DeviceData, DeviceLinkBuilder};
use crate::node::common::model::secret::MetaPasswordId;
use crate::node::common::model::user::UserDataMember;
use crate::node::common::model::vault::VaultStatus;
use crate::node::common::model::ApplicationState;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    /// Send recover request to all vault members except current user
    #[instrument(skip_all)]
    pub async fn recovery_request(
        &self,
        meta_pass_id: MetaPasswordId,
        app_state: &ApplicationState,
    ) -> anyhow::Result<()> {
        /*
        let Some(sender_device) = app_state.device.clone() else {
            return Err(anyhow!("Device not found"));
        };

        let Some(vault_status) = app_state.vault.clone() else {
            return Err(anyhow!("Vault not found"));
        };

        match vault_status {
            VaultStatus::Outsider(_) => {
                return Err(anyhow!("Vault not found"));
            }
            VaultStatus::Member { vault, .. } => {
                for UserDataMember(curr_member) in &vault.members() {
                    let curr_device = curr_member.device.clone();
                    if sender_device.id == curr_device.id {
                        continue;
                    }

                    let device_link = DeviceLinkBuilder::builder()
                        .sender(sender_device.id.clone())
                        .receiver(curr_device.id.clone())
                        .build()?;

                    let ss_device_log_desc =
                        SharedSecretDescriptor::SSDeviceLog(sender_device.id.clone()).to_obj_desc();

                    let log_event = {
                        let device_log_slot_id = self
                            .persistent_obj
                            .find_free_id_by_obj_desc(ss_device_log_desc.clone())
                            .await?;

                        let ObjectId::Artifact(ss_artifact_id) = device_log_slot_id else {
                            return Err(anyhow!("SSDeviceLog is not initialized"));
                        };

                        unimplemented!("Distribution algorithm has been changed")
                    };
                    //self.persistent_obj.repo.save(log_event).await?;
                }
            }
        }
         */

        Ok(())
    }
}
