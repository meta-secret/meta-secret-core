use crate::models::{UserSignature, VaultDoc};
use crate::node::db::commit_log::generate_next;
use crate::node::db::models::{AppOperation, AppOperationType, KvKey, KvLogEvent, KvValueType};

pub fn join_cluster_request(prev_key: &KvKey, user_sig: &UserSignature) -> KvLogEvent {
    KvLogEvent {
        key: generate_next(prev_key),
        cmd_type: AppOperationType::Request(AppOperation::JoinCluster),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_value(user_sig).unwrap(),
    }
}

pub fn accept_join_request(request: &KvLogEvent, vault: &VaultDoc) -> KvLogEvent {
    let mut maybe_error = None;
    if request.cmd_type != AppOperationType::Request(AppOperation::JoinCluster) {
        maybe_error = Some("Not allowed cmd_type");
    }

    if request.val_type != KvValueType::UserSignature {
        maybe_error = Some("Not allowed val_type");
    }

    if let Some(err_msg) = maybe_error {
        return KvLogEvent {
            key: generate_next(&request.key),
            cmd_type: AppOperationType::Update(AppOperation::JoinCluster),
            val_type: KvValueType::Error,
            value: serde_json::from_str(err_msg).unwrap(),
        };
    }

    let user_sig: UserSignature = serde_json::from_value(request.value.clone()).unwrap();

    let mut new_vault = vault.clone();
    new_vault.signatures.push(user_sig);

    KvLogEvent {
        key: generate_next(&request.key),
        cmd_type: AppOperationType::Update(AppOperation::JoinCluster),
        val_type: KvValueType::Vault,
        value: serde_json::to_value(&new_vault).unwrap(),
    }
}

#[cfg(test)]
pub mod test {
    use std::rc::Rc;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, VaultDoc};
    use crate::node::db::commit_log::transform;
    use crate::node::db::events::persistent_vault::generate_genesis_event;
    use crate::node::db::events::join::{accept_join_request, join_cluster_request};
    use crate::node::db::events::sign_up::{accept_sign_up_request, sign_up_request};
    use crate::node::db::models::LogCommandError;

    #[test]
    fn test_join_cluster() -> Result<(), LogCommandError> {
        let vault_name = "test";
        let server_km = KeyManager::generate();

        let genesis_event = generate_genesis_event(vault_name, &server_km.dsa.public_key());

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_event = sign_up_request(&genesis_event.key, &a_user_sig);

        let sing_up_accept = accept_sign_up_request(&sign_up_event.key, &a_user_sig);

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let join_request = join_cluster_request(&sing_up_accept[1].key, &b_user_sig);

        let vault = VaultDoc {
            vault_name: vault_name.to_string(),
            signatures: vec![a_user_sig.clone()],
            pending_joins: vec![],
            declined_joins: vec![],
        };
        let join_cluster_event = accept_join_request(&join_request, &vault);

        let mut commit_log = vec![genesis_event, sign_up_event];
        commit_log.extend(sing_up_accept);
        commit_log.push(join_cluster_event);

        println!("{}", serde_json::to_string_pretty(&commit_log).unwrap());

        let meta_db = transform(Rc::new(commit_log))?;

        println!("meta db: {}", serde_json::to_string_pretty(&meta_db).unwrap());

        let expected_sigs = vec![a_user_sig, b_user_sig];
        assert_eq!(expected_sigs, meta_db.vault_store.vault.unwrap().signatures);

        Ok(())
    }
}