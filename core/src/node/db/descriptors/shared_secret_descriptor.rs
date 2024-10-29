use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{MetaPasswordId, SsDistributionId};
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectType, ToObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretDescriptor {
    /// Local share of a secret
    LocalShare(MetaPasswordId),

    SsDeviceLog(DeviceId),

    /// SsLog is an event queue created on server and used by devices 
    /// to handle request from other devices. It is the same as VaultLog
    SsLog(VaultName),

    /// Allows devices distributing their shares (split/recover operations)
    SsDistribution(SsDistributionId),
}

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        match self {
            SharedSecretDescriptor::LocalShare { .. } => String::from("SsLocalShare"),
            SharedSecretDescriptor::SsDeviceLog(_) => String::from("SsDeviceLog"),
            SharedSecretDescriptor::SsLog(_) => String::from("SsLog"),
            SharedSecretDescriptor::SsDistribution(_) => String::from("SsDistribution"),
        }
    }
}

impl SharedSecretDescriptor {
    pub fn as_id_str(&self) -> String {
        match self {
            SharedSecretDescriptor::SsDistribution(event_id) => serde_json::to_string(event_id).unwrap(),
            SharedSecretDescriptor::SsLog(vault_name) => vault_name.to_string(),
            SharedSecretDescriptor::LocalShare { .. } => serde_json::to_string(self).unwrap(),
            SharedSecretDescriptor::SsDeviceLog(device_id) => device_id.to_string(),
        }
    }
}

impl ToObjectDescriptor for SharedSecretDescriptor {
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(self)
    }
}
