use crate::models::Base64EncodedText;
use crate::node::db::commit_log::{generate_key, store_names};
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType};

pub fn generate_vaults_genesis_key_id() -> KvKeyId {
    KvKeyId::genesis(store_names::VAULTS_IDX)
}

pub fn generate_vaults_genesis_key() -> KvKey {
    let id = generate_vaults_genesis_key_id();

    KvKey  {
        store: store_names::VAULTS_IDX.to_string(),
        id,
        vault_id: None,
    }
}

pub fn generate_vaults_genesis_event(server_pk: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: generate_vaults_genesis_key(),
        cmd_type: AppOperationType::Update(AppOperation::VaultsIndex),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(server_pk).unwrap(),
    }
}

pub fn generate_vaults_index_key(prev_id: &KvKeyId, vault_id: &str) -> KvKey {
    generate_key(store_names::VAULTS_IDX, prev_id, Some(vault_id.to_string()))
}

pub fn vaults_index_created_event(prev_id: &KvKeyId, vault_id: &str) -> KvLogEvent {
    KvLogEvent {
        key: generate_vaults_index_key(prev_id, vault_id),
        cmd_type: AppOperationType::Update(AppOperation::VaultsIndex),
        val_type: KvValueType::String,
        value: serde_json::to_value(vault_id).unwrap(),
    }
}

#[cfg(test)]
pub mod test {
    use std::collections::HashSet;
    use std::rc::Rc;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::crypto::utils;
    use crate::node::db::commit_log::transform;
    use crate::node::db::events::genesis::generate_genesis_event;
    use crate::node::db::events::index::vaults_index_created_event;
    use crate::node::db::models::{KvLogEvent, LogCommandError};

    #[test]
    fn vaults_index_test() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();

        let vault_name = "test_vault";
        let genesis_event: KvLogEvent = generate_genesis_event(vault_name, &server_km.dsa.public_key());
        // vaultName -> sha256 -> uuid
        let vault_id = utils::to_id(vault_name);
        let vaults_index_event = vaults_index_created_event(&genesis_event.key.id, vault_id.base64_text.as_str());

        let commit_log = vec![genesis_event, vaults_index_event];

        let meta_db = transform(Rc::new(commit_log))?;

        let mut expected = HashSet::new();
        expected.insert(vault_id.base64_text);

        assert_eq!(expected, meta_db.vaults.vaults_index);

        Ok(())
    }
}
