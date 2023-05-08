use crate::crypto::utils;
use crate::models::Base64EncodedText;
use crate::node::db::commit_log::store_names;
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType};

pub fn vault_formation_key_id(vault_id: &str) -> KvKey {
    let id = KvKeyId::object_foundation(vault_id, store_names::VAULT);

    KvKey  {
        store: store_names::VAULT.to_string(),
        id,
        vault_id: Some(vault_id.to_string()),
    }
}

pub fn vault_formation_key_id_from_vault_name(vault_name: &str) -> KvKey {
    let vault_id = utils::to_id(vault_name).base64_text;
    vault_formation_key_id(vault_id.as_str())
}

pub fn create_vault_formation_event_on_server(vault_name: &str, server_key: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: vault_formation_key_id_from_vault_name(vault_name),
        cmd_type: AppOperationType::Update(AppOperation::VaultFormation),
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
    use crate::node::db::events::persistent_vault::{generate_genesis_event, generate_genesis_key};
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
        assert_eq!(meta_db.vault_store.server_pk, Some(server_km.dsa.public_key()));
        Ok(())
    }
}
