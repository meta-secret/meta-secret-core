use crate::node::db::models::{GlobalIndexRecord, KeyIdGen, KvKey, KvKeyId, KvLogEvent, ObjectType};

pub trait GlobalIndexAction {
    fn new_event(&self, tail_id: &KvKeyId, vault_id: &str) -> KvLogEvent<GlobalIndexRecord> {
        let key = KvKey {
            key_id: tail_id.next(),
            object_type: ObjectType::GlobalIndexObj,
        };

        KvLogEvent {
            key,
            value: GlobalIndexRecord {
                vault_id: vault_id.to_string(),
            },
        }
    }
}
