use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::request::SyncRequest;
use std::sync::Arc;
use tracing::error;

pub struct SSReplicationAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SSReplicationAction<Repo> {

    pub async fn replicate(&self, request: &SyncRequest) -> Vec<GenericKvLogEvent> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let Some(_) = &request.vault_tail_id else {
            error!("Ignore empty vault request");
            return vec![];
        };

        let audit_desc = ObjectDescriptor::SharedSecretAudit {
            vault_name: request.sender.vault.name.clone(),
        };

        let audit_tail_id = request.s_s_audit.clone().unwrap_or(ObjectId::unit(&audit_desc));

        let events = self.persistent_obj.find_object_events(&audit_tail_id).await;

        for audit_event in events {
            commit_log.push(audit_event.clone());

            if let GenericKvLogEvent::SharedSecret(SharedSecretObject::Audit { event }) = audit_event {
                let ss_event_res = self.persistent_obj.repo.find_one(event.value.clone()).await;

                let Ok(Some(ss_event)) = ss_event_res else {
                    error!("Invalid event type: not an audit event");
                    continue;
                };

                let GenericKvLogEvent::SharedSecret(ss_obj) = &ss_event else {
                    error!("Invalid event type: not shared secret");
                    continue;
                };

                if let SharedSecretObject::Audit { .. } = ss_obj {
                    error!("Audit log events not allowed");
                    continue;
                }

                commit_log.push(ss_event);
            }
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
