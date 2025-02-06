use crate::node::common::model::device::common::DeviceData;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
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

impl KvLogEvent<GenesisId, DeviceData> {
    pub fn genesis(
        obj_desc: ObjectDescriptor,
        server_pk: DeviceData,
    ) -> KvLogEvent<GenesisId, DeviceData> {
        KvLogEvent {
            key: KvKey::genesis(obj_desc),
            value: server_pk,
        }
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
            GenericKvKey::ArtifactKey { key } => key.obj_desc.clone(),
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
    pub fn unit_from<D: ToObjectDescriptor>(obj_desc: D) -> Self {
        KvKey::unit(obj_desc.to_obj_desc())
    }

    pub fn unit(obj_desc: ObjectDescriptor) -> Self {
        Self {
            obj_id: UnitId::from(obj_desc.fqdn()),
            obj_desc,
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
    fn next(self) -> KvKey<GenesisId> {
        KvKey {
            obj_id: self.obj_id.next(),
            obj_desc: self.obj_desc.clone(),
        }
    }
}

impl Next<KvKey<ArtifactId>> for KvKey<GenesisId> {
    fn next(self) -> KvKey<ArtifactId> {
        KvKey {
            obj_id: self.obj_id.next(),
            obj_desc: self.obj_desc.clone(),
        }
    }
}

impl Next<KvKey<ArtifactId>> for KvKey<ArtifactId> {
    fn next(self) -> Self {
        Self { obj_id: self.obj_id.next(), obj_desc: self.obj_desc }
    }
}

impl KvKey<ArtifactId> {
    pub fn artifact(obj_desc: ObjectDescriptor, obj_id: ArtifactId) -> Self {
        Self { obj_id, obj_desc }
    }
}