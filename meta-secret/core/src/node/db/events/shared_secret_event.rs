use crate::node::common::model::secret::{
    SecretDistributionData, SsDistributionClaim, SsDistributionStatus, SsLogData,
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
pub enum SsObject {
    // Contains encrypted secret share for the receiver device
    // (will be deleted after sending to the receiver).
    // This is for SecretDistributionType::Recover
    SsClaim(KvLogEvent<SecretDistributionData>),
    // This is for SecretDistributionType::Split
    SsDistribution(KvLogEvent<SecretDistributionData>),

    SsDistributionStatus(KvLogEvent<SsDistributionStatus>),
}

impl KeyExtractor for SsObject {
    fn key(&self) -> KvKey {
        match self {
            SsObject::SsDistribution(event) => event.key.clone(),
            SsObject::SsDistributionStatus(event) => event.key.clone(),
            SsObject::SsClaim(event) => event.key.clone(),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SsObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SharedSecret(ss_obj) = event {
            Ok(ss_obj)
        } else {
            bail!(LogEventCastError::InvalidSharedSecret(event))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsDeviceLogObject(pub KvLogEvent<SsDistributionClaim>);

impl SsDeviceLogObject {
    pub fn get_distribution_request(&self) -> SsDistributionClaim {
        self.0.value.clone()
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

impl ObjIdExtractor for SsObject {
    fn obj_id(&self) -> ArtifactId {
        match self {
            SsObject::SsDistribution(event) => event.key.obj_id.clone(),
            SsObject::SsDistributionStatus(event) => event.key.obj_id.clone(),
            SsObject::SsClaim(event) => event.key.obj_id.clone(),
        }
    }
}

impl ToGenericEvent for SsObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SharedSecret(self)
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
    pub fn to_data(&self) -> SsLogData {
        self.0.value.clone()
    }

    pub fn insert(mut self, claim: SsDistributionClaim) -> Self {
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
