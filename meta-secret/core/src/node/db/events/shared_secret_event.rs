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
pub enum SsDistributionObject {
    // Contains encrypted secret share for the receiver device
    // (will be deleted after sending to the receiver).
    // This is for SecretDistributionType::Recover
    Claim(KvLogEvent<SecretDistributionData>),
    ClaimStatus(KvLogEvent<SsDistributionStatus>),
    
    // This is a secret share that device keeps for a password (means - Split)
    // We don't use split, because when a share on a target device, split - is confusing
    Distribution(KvLogEvent<SecretDistributionData>),
    DistributionStatus(KvLogEvent<SsDistributionStatus>),
}

impl KeyExtractor for SsDistributionObject {
    fn key(&self) -> KvKey {
        match self {
            SsDistributionObject::Distribution(event) => event.key.clone(),
            SsDistributionObject::DistributionStatus(event) => event.key.clone(),
            SsDistributionObject::Claim(event) => event.key.clone(),
            SsDistributionObject::ClaimStatus(event) => event.key.clone()
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SsDistributionObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SsDistribution(ss_obj) = event {
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

impl ObjIdExtractor for SsDistributionObject {
    fn obj_id(&self) -> ArtifactId {
        match self {
            SsDistributionObject::Distribution(event) => event.key.obj_id.clone(),
            SsDistributionObject::DistributionStatus(event) => event.key.obj_id.clone(),
            SsDistributionObject::Claim(event) => event.key.obj_id.clone(),
            SsDistributionObject::ClaimStatus(event) => event.key.obj_id.clone()
        }
    }
}

impl ToGenericEvent for SsDistributionObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsDistribution(self)
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
