use crate::models::Base64EncodedText;
use crate::node::db::commit_log::generate_key;
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType, ObjectType,
};

const GLOBAL_INDEX_ID: &str = "meta-g";

pub fn generate_global_index_formation_key_id() -> KvKeyId {
    KvKeyId::object_foundation(GLOBAL_INDEX_ID, ObjectType::GlobalIndex)
}

pub fn generate_global_index_formation_key() -> KvKey {
    let id = generate_global_index_formation_key_id();

    KvKey {
        object_type: ObjectType::GlobalIndex,
        id,
        vault_id: None,
    }
}

pub fn generate_global_index_formation_event(server_pk: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: generate_global_index_formation_key(),
        cmd_type: AppOperationType::Update(AppOperation::ObjectFormation),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_value(server_pk).unwrap(),
    }
}

pub fn generate_next_global_index_key(prev_id: &KvKeyId) -> KvKey {
    generate_key(ObjectType::GlobalIndex, prev_id, None)
}

pub fn new_global_index_record_created_event(tail_id: &KvKeyId, vault_id: &str) -> KvLogEvent {
    KvLogEvent {
        key: generate_next_global_index_key(tail_id),
        cmd_type: AppOperationType::Update(AppOperation::GlobalIndex),
        val_type: KvValueType::String,
        value: serde_json::to_value(vault_id).unwrap(),
    }
}
