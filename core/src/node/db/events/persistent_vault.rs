use crate::crypto::utils;
use crate::models::Base64EncodedText;
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType, ObjectType,
};


pub fn vault_formation_key_id_from_vault_name(vault_name: &str) -> KvKey {
    let id = KvKeyId::object_foundation(vault_name, ObjectType::Vault);
    let vault_id = id.key_id.clone();

    KvKey {
        object_type: ObjectType::Vault,
        id,
        vault_id: Some(vault_id),
    }
}

pub fn create_vault_formation_event_on_server(vault_name: &str, server_key: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: vault_formation_key_id_from_vault_name(vault_name),
        cmd_type: AppOperationType::Update(AppOperation::ObjectFormation),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(server_key).unwrap(),
    }
}
