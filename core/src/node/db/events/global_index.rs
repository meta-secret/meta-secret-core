use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType, ObjectType,
};

pub trait GlobalIndexAction {
    fn new_event(&self, tail_id: &KvKeyId, vault_id: &str) -> KvLogEvent {
        let key = KvKey {
            key_id: tail_id.next(),
            object_type: ObjectType::GlobalIndex,
        };

        KvLogEvent {
            key,
            cmd_type: AppOperationType::Update(AppOperation::GlobalIndex),
            val_type: KvValueType::String,
            value: serde_json::to_value(vault_id).unwrap(),
        }
    }
}
