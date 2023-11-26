use crate::node::common::model::user::UserDataCandidate;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::object_id::{ArtifactId, GenesisId, Next, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvLogEvent<Id, T> {
    pub key: KvKey<Id>,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvEvent<T> {
    pub obj_desc: ObjectDescriptor,
    pub value: T,
}

impl KvLogEvent<GenesisId, PublicKeyRecord> {
    pub fn genesis(obj_desc: ObjectDescriptor, server_pk: PublicKeyRecord) -> KvLogEvent<GenesisId, PublicKeyRecord> {
        KvLogEvent {
            key: KvKey::genesis(obj_desc),
            value: server_pk,
        }
    }

    pub fn vault_unit(user_sig: UserDataCandidate) -> KvLogEvent<UnitId, UserDataCandidate> {
        let obj_desc = ObjectDescriptor::vault(user_sig.data.vault_name.clone());
        KvLogEvent {
            key: KvKey::unit(obj_desc),
            value: user_sig,
        }
    }

    pub fn global_index_unit() -> KvLogEvent<UnitId, ()> {
        KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index)),
            value: (),
        }
    }

    pub fn global_index_genesis(server_pk: PublicKeyRecord) -> KvLogEvent<GenesisId, PublicKeyRecord> {
        Self::genesis(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index), server_pk)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericKvKey {
    UnitKey { key: KvKey<UnitId> },
    GenesisKey { key: KvKey<GenesisId> },
    ArtifactKey { key: KvKey<ArtifactId> },
}

impl From<KvKey<UnitId>> for GenericKvKey {
    fn from(unit_key: KvKey<UnitId>) -> Self {
        GenericKvKey::UnitKey { key: unit_key }
    }
}

impl From<KvKey<GenesisId>> for GenericKvKey {
    fn from(genesis_key: KvKey<GenesisId>) -> Self {
        GenericKvKey::GenesisKey { key: genesis_key }
    }
}

impl From<KvKey<ArtifactId>> for GenericKvKey {
    fn from(artifact_key: KvKey<ArtifactId>) -> Self {
        GenericKvKey::ArtifactKey { key: artifact_key }
    }
}

impl GenericKvKey {
    pub fn obj_desc(&self) -> ObjectDescriptor {
        match self {
            GenericKvKey::UnitKey { key } => key.obj_desc.clone(),
            GenericKvKey::GenesisKey { key } => key.obj_desc.clone(),
            GenericKvKey::ArtifactKey { key } => key.obj_desc.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey<Id> {
    pub obj_id: Id,
    pub obj_desc: ObjectDescriptor,
}

impl KvKey<UnitId> {
    pub fn unit(obj_desc: ObjectDescriptor) -> Self {
        Self {
            obj_id: UnitId::unit(obj_desc),
            obj_desc: obj_desc.clone(),
        }
    }
}

impl KvKey<GenesisId> {
    pub fn genesis(obj_desc: ObjectDescriptor) -> Self {
        let unit_id = KvKey::unit(obj_desc.clone());
        Self {
            obj_id: unit_id.next().obj_id,
            obj_desc,
        }
    }
}

impl Next<KvKey<GenesisId>> for KvKey<UnitId> {
    fn next(&self) -> KvKey<GenesisId> {
        KvKey {
            obj_id: self.obj_id.next(),
            obj_desc: self.obj_desc.clone(),
        }
    }
}
