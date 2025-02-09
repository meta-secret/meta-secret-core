use serde_derive::{Deserialize, Serialize};

use crate::node::db::descriptors::object_descriptor::{
    ChainId, ObjectFqdn, SeqId, ToObjectDescriptor,
};

pub trait Next<To> {
    fn next(self) -> To;
}

pub trait Prev<To> {
    fn prev(self) -> To;
}

/// Any regular request or update event in the objects' lifetime
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactId {
    pub fqdn: ObjectFqdn,
    pub id: ChainId,
}

impl ArtifactId {
    pub fn first(self) -> ArtifactId {
        ArtifactId::from(self.fqdn)
    }
}

impl From<ObjectFqdn> for ArtifactId {
    fn from(fqdn: ObjectFqdn) -> Self {
        Self {
            fqdn,
            id: ChainId::Seq(SeqId::first()),
        }
    }
}

/// Generate first artifact from object descriptor
impl<T: ToObjectDescriptor> From<T> for ArtifactId {
    fn from(obj_desc: T) -> Self {
        ArtifactId::from(obj_desc.to_obj_desc().fqdn())
    }
}

/// Generate next artifact from the previous one
impl Next<ArtifactId> for ArtifactId {
    fn next(mut self) -> Self {
        self.id = match self.id {
            ChainId::Genesis(genesis_id) => ChainId::Seq(genesis_id.next()),
            ChainId::Seq(seq_id) => ChainId::Seq(seq_id.next()),
        };

        self
    }
}
