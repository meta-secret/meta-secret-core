use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SsDistributionClaimDbId, SsDistributionId};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::IdString;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};
use crate::node::db::events::shared_secret_event::{
    SharedSecretObject, SsDeviceLogObject, SsLogObject,
};
use derive_more::From;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretDescriptor {
    /// Allows devices distributing their shares (split operation)
    SsDistribution(SsDistributionId),

    SsClaim(SsDistributionClaimDbId),
    SsDistributionStatus(SsDistributionClaimDbId),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDeviceLogDescriptor(DeviceId);

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogDescriptor(VaultName);

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        let obj_type = match self {
            SharedSecretDescriptor::SsDistribution(_) => "SsDistribution",
            SharedSecretDescriptor::SsDistributionStatus(_) => "SsDistributionStatus",
            SharedSecretDescriptor::SsClaim(_) => "SsClaim",
        };

        String::from(obj_type)
    }
}

impl ObjectName for SharedSecretDescriptor {
    fn object_name(&self) -> String {
        self.clone().id_str()
    }
}

impl ObjectType for SsDeviceLogDescriptor {
    fn object_type(&self) -> String {
        String::from("SsDeviceLog")
    }
}

impl ObjectType for SsLogDescriptor {
    fn object_type(&self) -> String {
        String::from("SsLog")
    }
}

impl IdString for SharedSecretDescriptor {
    fn id_str(self) -> String {
        match self {
            SharedSecretDescriptor::SsDistribution(event_id) => event_id.clone().id_str(),
            SharedSecretDescriptor::SsDistributionStatus(id) => id.clone().id_str(),
            SharedSecretDescriptor::SsClaim(db_id) => db_id.clone().id_str(),
        }
    }
}

impl IdString for SsLogDescriptor {
    fn id_str(self) -> String {
        self.0.to_string()
    }
}

impl IdString for SsDeviceLogDescriptor {
    fn id_str(self) -> String {
        self.0.to_string()
    }
}

impl ToObjectDescriptor for SharedSecretDescriptor {
    type EventType = SharedSecretObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(self)
    }
}

impl ToObjectDescriptor for SsLogDescriptor {
    type EventType = SsLogObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SsLog(self)
    }
}

impl ToObjectDescriptor for SsDeviceLogDescriptor {
    type EventType = SsDeviceLogObject;

    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SsDeviceLog(self)
    }
}
