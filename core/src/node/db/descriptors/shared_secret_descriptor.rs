use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{MetaPasswordId, SSDistributionId};
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectType, ToObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretDescriptor {
    /// Local share of a secret
    LocalShare(MetaPasswordId),

    SSDeviceLog(DeviceId),

    /// Ledgers traditionally track financial transactions
    /// but can be applied metaphorically to any situation where maintaining a detailed history of exchanges is crucial.
    /// In this case, the ledger logs password shards transferred between devices.
    SSLedger(VaultName),

    /// Allows devices distributing their shares (split/recover opeations)
    SSDistribution(SSDistributionId),
}

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        match self {
            SharedSecretDescriptor::LocalShare { .. } => String::from("SSLocalShare"),
            SharedSecretDescriptor::SSDeviceLog(_) => String::from("SSDeviceLog"),
            SharedSecretDescriptor::SSLedger(_) => String::from("SSLedger"),
            SharedSecretDescriptor::SSDistribution(_) => String::from("SSDistribution"),
        }
    }
}

impl SharedSecretDescriptor {
    pub fn as_id_str(&self) -> String {
        match self {
            SharedSecretDescriptor::SSDistribution(event_id) => serde_json::to_string(event_id).unwrap(),
            SharedSecretDescriptor::SSLedger(vault_name) => vault_name.to_string(),
            SharedSecretDescriptor::LocalShare { .. } => serde_json::to_string(self).unwrap(),
            SharedSecretDescriptor::SSDeviceLog(device_id) => device_id.to_string(),
        }
    }
}

impl ToObjectDescriptor for SharedSecretDescriptor {
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(self)
    }
}
