use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::request::{SharedSecretRequest, SyncRequest};
use std::sync::Arc;
use tracing::error;
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};

pub struct SSReplicationAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SSReplicationAction<Repo> {

    pub async fn replicate(&self, request: SharedSecretRequest, vault: &VaultData) -> Vec<GenericKvLogEvent> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let ss_log_events = self.persistent_obj
            .find_object_events(request.ss_log)
            .await?;

        commit_log.extend(ss_log_events);

        for member in vault.members() {
            if request.sender == member.user_data  {
                continue;
            }

            let ss_event_id = SharedSecretEventId {
                vault_name: request.sender.vault_name.clone(),
                sender: member.user_data.device.id.clone(),
                receiver: request.sender.device.id.clone(),
            };

            let ss_split_events = {
                let split_desc = SharedSecretDescriptor::Split(ss_event_id.clone());
                let ss_split_obj_desc = ObjectDescriptor::SharedSecret(split_desc);
                self.persistent_obj
                    .find_object_events(ObjectId::unit(ss_split_obj_desc))
                    .await?
            };
            commit_log.extend(ss_split_events);

            let ss_recover_events = {
                let recover_desc = SharedSecretDescriptor::Recover(ss_event_id);
                let ss_recover_obj_desc = ObjectDescriptor::SharedSecret(recover_desc);
                self.persistent_obj
                    .find_object_events(ObjectId::unit(ss_recover_obj_desc))
                    .await?
            };
            commit_log.extend(ss_recover_events);
        }

        commit_log
    }
}

#[cfg(test)]
mod test {
    /*
    #[tokio::test]
    async fn test_s_s_replication() {
        let vault_name = String::from("test_vault");

        let repo = Arc::new(InMemKvLogEventRepo::default());
        let persistent_obj = Arc::new(PersistentObject::new(repo.clone()));

        let creds_a = meta_test_utils::build_user_creds_a(vault_name.as_str());
        let creds_b = meta_test_utils::build_user_creds_b(vault_name.as_str());

        let app_state = JoinedAppState {
            app_state: ApplicationState {
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
                join_component: false,
            },
            creds: creds_a,
            vault_info: VaultInfo::Member {
                vault: VaultDoc {
                    vault_name: vault_name.clone(),
                    signatures: vec![creds_a.user_sig, creds_b.user_sig],
                    pending_joins: vec![],
                    declined_joins: vec![],
                },
            },
        };

        let distributor = MetaDistributor {
            persistent_obj: self.persistent_obj.clone(),
            vault: vault.clone(),
            user_creds: Arc::new(app_state.creds.clone()),
        };

        distributor.distribute(pass_id.to_string(), pass.to_string()).await;

        let recovery_action = RecoveryAction { persistent_obj };

        let meta_pass_id = meta_test_utils::build_meta_pass_1();
        recovery_action.recovery_request(meta_pass_id.clone(), &app_state).await;

        let sync_request = SyncRequest {
            sender: UserSignature {},
            vault_tail_id: None,
            meta_pass_tail_id: None,
            global_index: None,
            s_s_audit: None,
        };

        let replication_action = SharedSecretReplicationAction {
            persistent_obj: persistent_obj.clone(),
        };
        replication_action.replicate(&sync_request).await;
    }

     */
}
