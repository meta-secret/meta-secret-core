use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexObject {
    Unit { event: KvLogEvent<()> },
    Genesis { event: KvLogEvent<PublicKeyRecord> },
    Update { event: KvLogEvent<GlobalIndexRecord> },
}

impl GlobalIndexObject {
    pub fn key(&self) -> &KvKey {
        match self {
            GlobalIndexObject::Unit { event } => &event.key,
            GlobalIndexObject::Genesis { event } => &event.key,
            GlobalIndexObject::Update { event } => &event.key,
        }
    }
}

impl GlobalIndexObject {
    pub fn unit() -> Self {
        GlobalIndexObject::Unit {
            event: KvLogEvent::global_index_unit(),
        }
    }

    pub fn genesis(server_pk: &PublicKeyRecord) -> Self {
        let genesis_log_event = KvLogEvent::global_index_genesis(server_pk);
        GlobalIndexObject::Genesis {
            event: genesis_log_event,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexRecord {
    pub vault_id: String,
}

#[cfg(test)]
mod test {
    use crate::node::db::events::object_id::ObjectId;

    use super::*;

    #[test]
    fn unit_test() {
        let unit = GlobalIndexObject::unit();
        match unit {
            GlobalIndexObject::Unit { event } => match event.key {
                KvKey::Empty { .. } => {
                    panic!()
                }
                KvKey::Key { obj_id, .. } => match obj_id {
                    ObjectId::Unit { id } => {
                        assert_eq!("GlobalIndex:index::0", id);
                    }
                    _ => {
                        panic!("Invalid event");
                    }
                },
            },
            _ => {
                panic!("Invalid event");
            }
        }
    }
}
