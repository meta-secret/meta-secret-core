use crate::models::Base64EncodedText;
use crate::node::db::commit_log::store_names;
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType};

pub fn generate_genesis_key_from_vault_id(vault_id: &str) -> KvKey {
    let id = KvKeyId::genesis_from_vault_id(vault_id);

    KvKey  {
        store: store_names::GENESIS.to_string(),
        id,
        vault_id: Some(vault_id.to_string()),
    }
}

pub fn generate_genesis_key(vault_name: &str) -> KvKey {
    let id = KvKeyId::genesis(vault_name);

    KvKey  {
        store: store_names::GENESIS.to_string(),
        id: id.clone(),
        vault_id: Some(id.key_id),
    }
}

pub fn generate_genesis_event_with_key(key: &KvKey, server_key: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: key.clone(),
        cmd_type: AppOperationType::Update(AppOperation::Genesis),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(server_key).unwrap(),
    }
}

pub fn generate_genesis_event(vault_name: &str, server_key: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: generate_genesis_key(vault_name),
        cmd_type: AppOperationType::Update(AppOperation::Genesis),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(server_key).unwrap(),
    }
}

#[cfg(test)]
pub mod test {
    use std::rc::Rc;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::crypto::utils;
    use crate::node::db::commit_log::{store_names, transform};
    use crate::node::db::events::genesis::{generate_genesis_event, generate_genesis_key};
    use crate::node::db::models::LogCommandError;

    #[test]
    fn generate_genesis_key_test() {
        let vault_name = "test_vault";
        let vault_id = utils::to_id(vault_name).base64_text;

        let genesis_key = generate_genesis_key(vault_name);
        assert_eq!(store_names::GENESIS, genesis_key.store);
        assert_eq!("-1".to_string(), genesis_key.id.prev_key_id);
        assert_eq!(vault_id, genesis_key.id.key_id);
        assert_eq!(vault_id, genesis_key.vault_id.unwrap());
    }

    #[test]
    fn test_genesis_event() -> Result<(), LogCommandError> {
        let vault_name = "test_vault";
        let server_km = KeyManager::generate();

        let genesis_event = generate_genesis_event(vault_name, &server_km.dsa.public_key());

        let commit_log = vec![genesis_event];
        let meta_db = transform(Rc::new(commit_log))?;
        assert_eq!(meta_db.meta_store.server_pk, Some(server_km.dsa.public_key()));
        Ok(())
    }
}
