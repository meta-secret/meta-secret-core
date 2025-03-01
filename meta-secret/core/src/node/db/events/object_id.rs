use serde_derive::{Deserialize, Serialize};
use crate::node::common::model::IdString;
use crate::node::db::descriptors::object_descriptor::{ObjectFqdn, SeqId, ToObjectDescriptor};

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

impl IdString for ArtifactId {
    fn id_str(self) -> String {
        format!("{}::{}", self.fqdn.id_str(), self.id.curr)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::db::descriptors::object_descriptor::{ObjectFqdn, SeqId};

    #[test]
    fn test_artifact_id_id_str() {
        // Create test data
        let obj_type = "test_namespace".to_string();
        let obj_instance = "test_object".to_string();
        let fqdn = ObjectFqdn { obj_type, obj_instance };
        
        // Create ArtifactId using the From trait implementation
        let artifact_id = ArtifactId::from(fqdn.clone());
        
        // Test the id_str method with hardcoded expected string
        let expected_id_str = "test_namespace:test_object::1";
        assert_eq!(artifact_id.clone().id_str(), expected_id_str);
        
        // Test with a different sequence ID
        let mut seq_id = SeqId::first();
        seq_id.curr = 100;
        
        let artifact_id2 = ArtifactId {
            fqdn: fqdn.clone(),
            id: seq_id,
        };
        
        // Test with hardcoded expected string
        let expected_id_str2 = "test_namespace:test_object::100";
        assert_eq!(artifact_id2.id_str(), expected_id_str2);
    }
}
