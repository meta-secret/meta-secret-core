use crate::crypto::utils;
use crate::models::{UserSignature, VaultDoc};
use crate::node::db::commit_log::{generate_commit_log_key, store_names};
use crate::node::db::events::index::vaults_index_created_event;
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType};

pub fn accept_event_sign_up_request(event: &KvLogEvent) -> Vec<KvLogEvent> {
    if event.cmd_type != AppOperationType::Request(AppOperation::SignUp) {
        panic!("Invalid request");
    }

    let user_sig: UserSignature = serde_json::from_value(event.value.clone()).unwrap();
    accept_sign_up_request(&event.key, &user_sig)
}

pub fn accept_sign_up_request(prev: &KvKey, user_sig: &UserSignature) -> Vec<KvLogEvent> {
    let vault_id = utils::to_id(user_sig.vault_name.as_str()).base64_text;

    let vault = VaultDoc {
        vault_name: user_sig.vault_name.clone(),
        signatures: vec![user_sig.clone()],
        pending_joins: vec![],
        declined_joins: vec![],
    };

    let key_id = KvKeyId::object_foundation(vault_id.as_str(), store_names::USER_VAULT);
    let sign_up_event = KvLogEvent {
        key: KvKey {
            store: store_names::USER_VAULT.to_string(),
            id: key_id,
            vault_id: prev.vault_id.clone(),
        },
        cmd_type: AppOperationType::Update(AppOperation::SignUp),
        val_type: KvValueType::Vault,
        value: serde_json::to_value(&vault).unwrap(),
    };

    let vaults_index = vaults_index_created_event(&prev.id, vault_id.as_str());

    vec![sign_up_event, vaults_index]
}

pub fn sign_up_request(prev_key: &KvKey, user_sig: &UserSignature) -> KvLogEvent {
    let vault_id = utils::to_id(user_sig.vault_name.as_str());

    KvLogEvent {
        key: generate_commit_log_key(&prev_key.id, Some(vault_id.base64_text)),
        cmd_type: AppOperationType::Request(AppOperation::SignUp),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_value(user_sig).unwrap(),
    }
}

#[cfg(test)]
pub mod test {
    use std::rc::Rc;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, VaultDoc};
    use crate::node::db::commit_log::transform;
    use crate::node::db::events::genesis::generate_genesis_event;
    use crate::node::db::events::sign_up::{accept_sign_up_request, sign_up_request};
    use crate::node::db::models::LogCommandError;

    #[test]
    fn test_sign_up() -> Result<(), LogCommandError> {
        let vault_name = "test";
        let server_km = KeyManager::generate();

        let genesis_event = generate_genesis_event(vault_name, &server_km.dsa.public_key());


        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_event = sign_up_request(&genesis_event.key, &a_user_sig);

        let sing_up_accept = accept_sign_up_request(&sign_up_event.key, &a_user_sig);

        let mut commit_log = vec![genesis_event, sign_up_event];
        commit_log.extend(sing_up_accept);

        let meta_db = transform(Rc::new(commit_log))?;

        let vault = VaultDoc {
            vault_name: vault_name.to_string(),
            signatures: vec![a_user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };

        assert_eq!(vault, meta_db.meta_store.vault.unwrap());

        Ok(())
    }
}
