use crate::crypto::utils;
use crate::models::{Base64EncodedText, UserSignature, VaultDoc};
use crate::node::db::commit_log::store_names;
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType};

pub fn accept_event_sign_up_request(sign_up_request: KvLogEvent, server_pk: Base64EncodedText) -> Vec<KvLogEvent> {
    if sign_up_request.cmd_type != AppOperationType::Request(AppOperation::SignUp) {
        panic!("Invalid request");
    }

    accept_sign_up_request(sign_up_request, server_pk)
}

pub fn accept_sign_up_request(sign_up_request: KvLogEvent, server_pk: Base64EncodedText) -> Vec<KvLogEvent> {
    let user_sig: UserSignature = serde_json::from_value(sign_up_request.value.clone()).unwrap();

    let vault = VaultDoc {
        vault_name: user_sig.vault_name.clone(),
        signatures: vec![user_sig.clone()],
        pending_joins: vec![],
        declined_joins: vec![],
    };

    let vault_id = sign_up_request.key.vault_id.clone().unwrap();

    let vault_formation_event = KvLogEvent {
        key: KvKey {
            store: store_names::VAULT.to_string(),
            id: KvKeyId::object_foundation(vault_id.as_str(), store_names::VAULT),
            vault_id: sign_up_request.key.vault_id.clone(),
        },
        cmd_type: AppOperationType::Update(AppOperation::VaultFormation),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(&server_pk).unwrap(),
    };

    let expected_sign_request_id = vault_formation_event.key.id.next();
    let actual_sign_up_request_id = sign_up_request.key.id.clone();
    if actual_sign_up_request_id != expected_sign_request_id {
        panic!("Rogue request: invalid id");
    }

    let sign_up_event = KvLogEvent {
        key: KvKey {
            store: store_names::VAULT.to_string(),
            id: expected_sign_request_id.next(),
            vault_id: Some(vault_id),
        },
        cmd_type: AppOperationType::Update(AppOperation::SignUp),
        val_type: KvValueType::Vault,
        value: serde_json::to_value(&vault).unwrap(),
    };

    vec![vault_formation_event, sign_up_request, sign_up_event]
}

pub fn sign_up_request(user_sig: &UserSignature) -> KvLogEvent {
    let vault_id = utils::to_id(user_sig.vault_name.as_str()).base64_text;
    let sign_up_key = KvKey {
        id: KvKeyId::object_foundation(vault_id.as_str(), store_names::VAULT).next(),
        store: store_names::VAULT.to_string(),
        vault_id: Some(vault_id),
    };

    KvLogEvent {
        key: sign_up_key,
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
    use crate::node::db::events::persistent_vault::create_vault_formation_event_on_server;
    use crate::node::db::events::sign_up::{accept_sign_up_request, sign_up_request};
    use crate::node::db::models::LogCommandError;

    #[test]
    fn test_sign_up() -> Result<(), LogCommandError> {
        let vault_name = "test";
        let server_km = KeyManager::generate();

        let formation_event = create_vault_formation_event_on_server(vault_name, &server_km.dsa.public_key());

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_request = sign_up_request(&a_user_sig);

        let sing_up_accept = accept_sign_up_request(sign_up_request, server_km.dsa.public_key());

        let mut commit_log = vec![formation_event, sign_up_request, sing_up_accept[2]];
        commit_log.extend(sing_up_accept);

        let meta_db = transform(Rc::new(commit_log))?;

        let vault = VaultDoc {
            vault_name: vault_name.to_string(),
            signatures: vec![a_user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };

        assert_eq!(vault, meta_db.vault_store.vault.unwrap());

        Ok(())
    }
}
