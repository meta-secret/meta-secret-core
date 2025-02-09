use serde_derive::{Deserialize, Serialize};

use crate::node::db::descriptors::object_descriptor::{
    ObjectFqdn, SeqId, ToObjectDescriptor,
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
    pub id: SeqId,
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
            id: SeqId::first(),
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
        self.id = self.id.next();
        self
    }
}
