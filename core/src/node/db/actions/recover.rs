use crate::models::{MetaPasswordId, PasswordRecoveryRequest};
use crate::node::app::meta_app::app_state::JoinedAppState;
use crate::node::db::events::common::{ObjectCreator, SharedSecretObject, VaultInfo};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::{ObjectDescriptor, SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use std::sync::Arc;

pub struct RecoveryAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> RecoveryAction<Repo> {
    pub async fn recovery_request(&self, meta_pass_id: MetaPasswordId, app_state: &JoinedAppState) {
        if let VaultInfo::Member { vault } = &app_state.vault_info {
            for curr_sig in &vault.signatures {
                if app_state.creds.user_sig.public_key.base64_text == curr_sig.public_key.base64_text {
                    continue;
                }

                let recovery_request = PasswordRecoveryRequest {
                    id: Box::new(meta_pass_id.clone()),
                    consumer: Box::new(curr_sig.clone()),
                    provider: app_state.creds.user_sig.clone(),
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

                let _ = self.persistent_obj.repo.save_event(audit_event).await;

                let recovery_request_event = GenericKvLogEvent::SharedSecret(SharedSecretObject::RecoveryRequest {
                    event: KvLogEvent {
                        key: KvKey {
                            obj_id: ObjectId::unit(&ss_obj_desc),
                            obj_desc: ss_obj_desc,
                        },
                        value: recovery_request,
                    },
                });

                let _ = self.persistent_obj.repo.save_event(recovery_request_event).await;
            }
        } else {
            panic!("You must be a member of the vault");
        }
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::keys::KeyManager;
    use crate::models::{ApplicationState, DeviceInfo, MetaPasswordId, MetaVault, UserCredentials, VaultDoc};
    use crate::node::app::meta_app::app_state::JoinedAppState;
    use crate::node::db::actions::recover::RecoveryAction;
    use crate::node::db::events::common::{ObjectCreator, SharedSecretObject, VaultInfo};
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::object_descriptor::{ObjectDescriptor, SharedSecretDescriptor, SharedSecretEventId};
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::ops::Deref;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_recovery_request() {
        let vault_name = String::from("test_vault");

        let repo = Arc::new(InMemKvLogEventRepo::default());
        let persistent_obj = PersistentObject::new(repo.clone());

        let action = RecoveryAction {
            persistent_obj: Arc::new(persistent_obj),
        };

        let meta_pass_id = MetaPasswordId {
            id: "test_pass_id_123".to_string(),
            salt: "pass_salt".to_string(),
            name: "test_pass".to_string(),
        };

        let s_box_a = KeyManager::generate_security_box(vault_name.to_string());
        let device_a = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig_a = s_box_a.get_user_sig(&device_a);

        let s_box_b = KeyManager::generate_security_box(vault_name.to_string());
        let device_b = DeviceInfo {
            device_id: "b".to_string(),
            device_name: "b".to_string(),
        };
        let user_sig_b = s_box_b.get_user_sig(&device_b);

        let _meta_vault = MetaVault {
            name: vault_name.clone(),
            device: Box::new(device_a.clone()),
        };

        let creds = UserCredentials {
            security_box: Box::new(s_box_a),
            user_sig: Box::new(user_sig_a.clone()),
        };

        let app_state = JoinedAppState {
            app_state: ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            },
            creds,
            vault_info: VaultInfo::Member {
                vault: VaultDoc {
                    vault_name: vault_name.clone(),
                    signatures: vec![user_sig_a, user_sig_b],
                    pending_joins: vec![],
                    declined_joins: vec![],
                },
            },
        };

        action.recovery_request(meta_pass_id.clone(), &app_state).await;

        let tmp_db = repo.db.clone();
        let db = tmp_db.as_ref().lock().await;
        let events = db.deref();

        assert_eq!(2, events.len());
        let audit_obj_id = ObjectId::unit(&ObjectDescriptor::SharedSecretAudit {
            vault_name: vault_name.clone(),
        });
        let audit_event_id = events.get(&audit_obj_id).unwrap();

        let GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit { event }) = audit_event_id else {
            panic!();
        };

        let recovery_request_desc =
            ObjectDescriptor::SharedSecret(SharedSecretDescriptor::RecoveryRequest(SharedSecretEventId {
                vault_name,
                meta_pass_id,
                receiver: device_b,
            }));
        assert_eq!(recovery_request_desc.to_id(), event.value.id_str());
    }
}
