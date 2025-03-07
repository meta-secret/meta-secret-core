use crate::node::common::model::secret::{
    SecretDistributionData, SsClaim, SsLogData,
};
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::{bail, Ok};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SsWorkflowObject {
    // Contains encrypted secret share for the receiver device
    // (will be deleted after sending to the receiver).
    // This is for SecretDistributionType::Recover
    Recovery(KvLogEvent<SecretDistributionData>),
    // This is a secret share that device keeps for a password (means - Split)
    // We don't use split, because when a share on a target device, split - is confusing
    Distribution(KvLogEvent<SecretDistributionData>),
}

impl KeyExtractor for SsWorkflowObject {
    fn key(&self) -> KvKey {
        match self {
            SsWorkflowObject::Distribution(event) => event.key.clone(),
            SsWorkflowObject::Recovery(event) => event.key.clone(),
        }
    }
}

impl SsWorkflowObject {
    pub fn to_distribution_data(self) -> SecretDistributionData {
        match self {
            SsWorkflowObject::Recovery(claim) => claim.value,
            SsWorkflowObject::Distribution(dist) => dist.value
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SsWorkflowObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SsWorkflow(ss_obj) = event {
            Ok(ss_obj)
        } else {
            bail!(LogEventCastError::InvalidSharedSecret(event))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDeviceLogObject(pub KvLogEvent<SsClaim>);

impl SsDeviceLogObject {
    pub fn to_distribution_request(self) -> SsClaim {
        self.0.value
    }
}

impl TryFrom<GenericKvLogEvent> for SsDeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SsDeviceLog(ss_device_log) = event {
            Ok(ss_device_log)
        } else {
            bail!("Not a shared secret device log event")
        }
    }
}

impl ObjIdExtractor for SsDeviceLogObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl ToGenericEvent for SsDeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsDeviceLog(self)
    }
}

impl KeyExtractor for SsDeviceLogObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

impl ObjIdExtractor for SsWorkflowObject {
    fn obj_id(&self) -> ArtifactId {
        match self {
            SsWorkflowObject::Distribution(event) => event.key.obj_id.clone(),
            SsWorkflowObject::Recovery(event) => event.key.obj_id.clone()
        }
    }
}

impl ToGenericEvent for SsWorkflowObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsWorkflow(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsLogObject(pub KvLogEvent<SsLogData>);

impl TryFrom<GenericKvLogEvent> for SsLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SsLog(ss_obj) = &event {
            Ok(ss_obj.clone())
        } else {
            bail!(LogEventCastError::InvalidSsLog(event))
        }
    }
}

impl SsLogObject {
    pub fn to_data(self) -> SsLogData {
        self.0.value
    }

    pub fn as_data(&self) -> &SsLogData {
        &self.0.value
    }

    pub fn insert(mut self, claim: SsClaim) -> Self {
        self.0.value = self.0.value.insert(claim);
        self
    }
}

impl ToGenericEvent for SsLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsLog(self)
    }
}

impl ObjIdExtractor for SsLogObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl KeyExtractor for SsLogObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
