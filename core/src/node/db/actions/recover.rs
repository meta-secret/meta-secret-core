use std::sync::Arc;

use anyhow::anyhow;
use async_std::prelude::StreamExt;
use tracing_attributes::instrument;

use crate::node::common::model::ApplicationState;
use crate::node::common::model::device::DeviceLinkBuilder;
use crate::node::common::model::secret::{MetaPasswordId, PasswordRecoveryRequest};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::events::common::SSDeviceLogObject;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    /// Send recover request to all vault members except current user
    #[instrument(skip_all)]
    pub async fn recovery_request(&self, meta_pass_id: MetaPasswordId, app_state: &ApplicationState) -> anyhow::Result<()> {
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
            VaultStatus::Member(vault) => {
                for curr_member in &vault.members() {
                    let curr_device = curr_member.user_data.device.clone();
                    if sender_device.id == curr_device.id {
                        continue;
                    }

                    let device_link = DeviceLinkBuilder::new()
                        .sender(sender_device.id.clone())
                        .receiver(curr_device.id.clone())
                        .build()?;

                    let recovery_request = PasswordRecoveryRequest {
                        id: meta_pass_id.clone(),
                        device_link,
                    };

                    let ss_device_log_desc = SharedSecretDescriptor::SSDeviceLog(sender_device.id.clone())
                        .to_obj_desc();

                    let log_event = {
                        let device_log_slot_id = self
                            .persistent_obj
                            .find_free_id_by_obj_desc(ss_device_log_desc.clone())
                            .await?;

                        let ObjectId::Artifact(ss_artifact_id) = device_log_slot_id else {
                            return Err(anyhow!("SSDeviceLog is not initialized"));
                        };

                        SSDeviceLogObject::DeviceLog(KvLogEvent {
                            key: KvKey {
                                obj_id: ss_artifact_id,
                                obj_desc: ss_device_log_desc.clone(),
                            },
                            value: recovery_request,
                        }).to_generic()
                    };
                    self.persistent_obj.repo.save(log_event).await?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;
    use std::sync::Arc;

    use crate::node::common::model::ApplicationState;
    use crate::node::common::model::vault::VaultStatus;
    use crate::node::db::actions::recover::RecoveryAction;
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::test_utils::meta_test_utils;

    #[tokio::test]
    async fn test_recovery_request() {
        let vault_name = String::from("test_vault");

        let repo = Arc::new(InMemKvLogEventRepo::default());
        let persistent_obj = PersistentObject::new(repo.clone());

        let meta_pass_id = meta_test_utils::build_meta_pass_1();

        let creds_a = meta_test_utils::build_user_creds_a(vault_name.as_str());
        let creds_b = meta_test_utils::build_user_creds_b(vault_name.as_str());

        let app_state = MemberAppState {
            app_state: ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            },
            creds: creds_a.clone(),
            vault_info: VaultStatus::Member {
                vault: VaultDoc {
                    vault_name: vault_name.clone(),
                    signatures: vec![creds_a.user_sig, creds_b.user_sig.clone()],
                    pending_joins: vec![],
                    declined_joins: vec![],
                },
            },
        };

        let action = RecoveryAction {
            persistent_obj: Arc::new(persistent_obj),
        };
        action
            .recovery_request(meta_pass_id.clone(), &app_state)
            .await?;

        let tmp_db = repo.db.clone();
        let db = tmp_db.as_ref().lock().await;
        let events = db.deref();

        assert_eq!(2, events.len());

        let recovery_request_desc = {
            let s_s_event_id = SharedSecretEventId {
                vault_name: vault_name.clone(),
                meta_pass_id: meta_pass_id.clone(),
                receiver: creds_b.user_sig.vault.device.as_ref().clone(),
            };

            ObjectDescriptor::SharedSecret(SharedSecretDescriptor::RecoveryRequest(s_s_event_id))
        };

        {
            let audit_event_id = {
                let audit_obj_id = ObjectId::unit(&ObjectDescriptor::SharedSecretAudit { vault_name });
                events.get(&audit_obj_id)
            };

            let Some(GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit { event })) = audit_event_id else {
                panic!("Invalid event");
            };

            assert_eq!(recovery_request_desc.to_fqdn(), event.value.id_str());
        }

        {
            let r_r_event = events.get(&ObjectId::unit(&recovery_request_desc));
            assert_eq!(recovery_request_desc, r_r_event.unwrap().key().obj_desc);
        }
    }
}
