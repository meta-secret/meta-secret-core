use std::sync::Arc;

use tracing::error;

use crate::node::app::meta_app::app_state::MemberAppState;
use crate::node::common::model::{MetaPasswordId, PasswordRecoveryRequest};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::events::common::{SharedSecretObject};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::{ObjectDescriptor};
use crate::node::db::events::object_descriptor::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;

pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    pub async fn recovery_request(&self, meta_pass_id: MetaPasswordId, app_state: &MemberAppState) {
        let VaultStatus::Member { vault } = &app_state.vault_info else {
            error!("You must be a member of the vault");
            return;
        };

        for curr_sig in &vault.signatures {
            if app_state.creds.user_sig.public_key.base64_text == curr_sig.public_key.base64_text {
                continue;
            }

            let recovery_request = PasswordRecoveryRequest {
                id: Box::new(meta_pass_id.clone()),
                consumer: Box::new(curr_sig.clone()),
                provider: Box::new(app_state.creds.user_sig.clone()),
            };

            let recovery_request_id = SharedSecretEventId {
                vault_name: curr_sig.vault.name.clone(),
                meta_pass_id: meta_pass_id.clone(),
                receiver: curr_sig.vault.device.as_ref().clone(),
            };

            let ss_obj_desc = {
                let ss_desc = SharedSecretDescriptor::RecoveryRequest(recovery_request_id);
                ObjectDescriptor::SharedSecret(ss_desc)
            };

            let audit_obj_desc = ObjectDescriptor::SharedSecretAudit {
                vault_name: curr_sig.vault.name.clone(),
            };

            let audit_event = {
                let audit_slot_id = self
                    .persistent_obj
                    .find_tail_id_by_obj_desc(&audit_obj_desc)
                    .await
                    .map(|id| id.next())
                    .unwrap_or(ObjectId::unit(&audit_obj_desc));

                GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit {
                    event: KvLogEvent {
                        key: KvKey {
                            obj_id: audit_slot_id,
                            obj_desc: audit_obj_desc.clone(),
                        },
                        value: ObjectId::unit(&ss_obj_desc),
                    },
                })
            };

            let _ = self.persistent_obj.repo.save(audit_event).await;

            let recovery_request_event = GenericKvLogEvent::SharedSecret(SharedSecretObject::RecoveryRequest {
                event: KvLogEvent {
                    key: KvKey {
                        obj_id: ObjectId::unit(&ss_obj_desc),
                        obj_desc: ss_obj_desc,
                    },
                    value: recovery_request,
                },
            });

            let _ = self.persistent_obj.repo.save(recovery_request_event).await;
        }
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;
    use std::sync::Arc;

    use crate::models::{ApplicationState, VaultDoc};
    use crate::node::app::meta_app::app_state::MemberAppState;
    use crate::node::common::model::ApplicationState;
    use crate::node::common::model::vault::VaultStatus;
    use crate::node::db::actions::recover::RecoveryAction;
    use crate::node::db::events::common::{LogEventKeyBasedRecord, ObjectCreator, SharedSecretObject, VaultInfo};
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::object_descriptor::{ObjectDescriptor, SharedSecretDescriptor, SharedSecretEventId};
    use crate::node::db::events::object_descriptor::shared_secret::SharedSecretEventId;
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
        action.recovery_request(meta_pass_id.clone(), &app_state).await;

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
