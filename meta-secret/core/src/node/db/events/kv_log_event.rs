use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::events::object_id::{ArtifactId, Next};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvLogEvent<T> {
    pub key: KvKey,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvEvent<T> {
    pub obj_desc: ObjectDescriptor,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub obj_id: ArtifactId,
    pub obj_desc: ObjectDescriptor,
}

impl<T: ToObjectDescriptor> From<T> for KvKey {
    fn from(obj_desc: T) -> Self {
        Self {
            obj_id: ArtifactId::from(obj_desc.clone()),
            obj_desc: obj_desc.to_obj_desc(),
        }
    }
}

impl Next<KvKey> for KvKey {
    fn next(mut self) -> Self {
        self.obj_id = self.obj_id.next();
        self
    }
}

impl KvKey {
    pub fn artifact(obj_desc: ObjectDescriptor, obj_id: ArtifactId) -> Self {
        Self { obj_id, obj_desc }
    }
}