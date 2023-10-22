use crate::node::common::model::user::UserDataCandidate;
use crate::node::db::events::common::{ObjectCreator, PublicKeyRecord};
use crate::node::db::events::global_index::GlobalIndexRecord;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvLogEvent<T> {
    pub key: KvKey,
    pub value: T,
}

impl KvLogEvent<PublicKeyRecord> {
    pub fn genesis(obj_desc: &ObjectDescriptor, server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        KvLogEvent {
            key: KvKey::genesis(obj_desc),
            value: server_pk.clone(),
        }
    }

    pub fn vault_unit(user_sig: &UserDataCandidate) -> KvLogEvent<UserDataCandidate> {
        let obj_desc = &ObjectDescriptor::vault(user_sig.data.vault_name.clone());
        KvLogEvent {
            key: KvKey::unit(obj_desc),
            value: user_sig.clone(),
        }
    }

    pub fn global_index_unit() -> KvLogEvent<()> {
        KvLogEvent {
            key: KvKey::unit(&ObjectDescriptor::GlobalIndex),
            value: (),
        }
    }

    pub fn global_index_genesis(server_pk: &PublicKeyRecord) -> KvLogEvent<PublicKeyRecord> {
        Self::genesis(&ObjectDescriptor::GlobalIndex, server_pk)
    }
}

impl KvLogEvent<GlobalIndexRecord> {
    pub fn new_global_index_event(tail_id: &ObjectId, vault_id: &IdStr) -> KvLogEvent<GlobalIndexRecord> {
        let key = KvKey {
            obj_id: tail_id.clone(),
            obj_desc: ObjectDescriptor::GlobalIndex,
        };

        KvLogEvent {
            key,
            value: GlobalIndexRecord {
                vault_id: vault_id.id.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub obj_id: ObjectId,
    pub obj_desc: ObjectDescriptor,
}

impl ObjectCreator<&ObjectDescriptor> for KvKey {
    fn unit(obj_desc: &ObjectDescriptor) -> Self {
        Self {
            obj_id: ObjectId::unit(obj_desc),
            obj_desc: obj_desc.clone(),
        }
    }

    fn genesis(obj_desc: &ObjectDescriptor) -> Self {
        Self::unit(obj_desc).next()
    }
}

impl IdGen for KvKey {
    fn next(&self) -> Self {
        Self {
            obj_id: self.obj_id.next(),
            obj_desc: self.obj_desc.clone(),
        }
    }
}
