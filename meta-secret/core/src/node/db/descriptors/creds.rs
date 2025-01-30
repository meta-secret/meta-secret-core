use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor};
use crate::node::db::events::local_event::CredentialsObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsDescriptor;

impl ToObjectDescriptor for CredentialsDescriptor {
    type EventType = CredentialsObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::Creds(self)
    }
}

impl ObjectType for CredentialsDescriptor {
    fn object_type(&self) -> String {
        String::from("DeviceCreds")
    }
}

impl ObjectName for CredentialsDescriptor {
    fn object_name(&self) -> String {
        String::from("index")
    }
}