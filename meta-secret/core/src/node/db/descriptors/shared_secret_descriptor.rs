use crate::node::common::model::IdString;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SsDistributionId, SsRecoveryId};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{
    ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor,
};
use crate::node::db::events::shared_secret_event::{
    SsDeviceLogObject, SsLogObject, SsWorkflowObject,
};
use derive_more::From;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsWorkflowDescriptor {
    Recovery(SsRecoveryId),
    /// Allows devices distributing their shares (split operation)
    Distribution(SsDistributionId),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDeviceLogDescriptor(DeviceId);

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogDescriptor(VaultName);

impl ObjectType for SsWorkflowDescriptor {
    fn object_type(&self) -> String {
        let obj_type = match self {
            SsWorkflowDescriptor::Distribution(_) => "SsDistribution",
            SsWorkflowDescriptor::Recovery(_) => "SsRecovery",
        };

        String::from(obj_type)
    }
}

impl ObjectName for SsWorkflowDescriptor {
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

impl IdString for SsWorkflowDescriptor {
    fn id_str(self) -> String {
        match self {
            SsWorkflowDescriptor::Distribution(event_id) => event_id.id_str(),
            SsWorkflowDescriptor::Recovery(db_id) => db_id.id_str(),
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

impl ToObjectDescriptor for SsWorkflowDescriptor {
    type EventType = SsWorkflowObject;

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
