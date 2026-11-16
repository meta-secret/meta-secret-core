use super::object_descriptor::{ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor};
use crate::node::common::model::user::common::UserId;
use crate::node::db::events::local_removal_envelope::LocalRemovalEnvelopeObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRemovalEnvelopeDescriptor(pub UserId);

impl ToObjectDescriptor for LocalRemovalEnvelopeDescriptor {
    type EventType = LocalRemovalEnvelopeObject;
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::LocalRemovalEnvelope(self)
    }
}
impl ObjectType for LocalRemovalEnvelopeDescriptor {
    fn object_type(&self) -> String {
        "LocalRemovalEnvelope".into()
    }
}
impl ObjectName for LocalRemovalEnvelopeDescriptor {
    fn object_name(&self) -> String {
        self.0.device_id.to_string()
    }
}
