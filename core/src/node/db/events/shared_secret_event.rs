use crate::node::common::model::secret::{SSDistributionClaim, SSLedgerData, SecretDistributionData};
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, UnitId};
use anyhow::{anyhow, bail, Ok};
use super::object_id::{VaultGenesisEvent, VaultUnitEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    LocalShare(KvLogEvent<UnitId, SecretDistributionData>),
    SSDistribution(KvLogEvent<UnitId, SecretDistributionData>),
}

impl SharedSecretObject {
    pub fn to_local_share(&self) -> anyhow::Result<SecretDistributionData> {
        if let SharedSecretObject::LocalShare(event) = self {
            Ok(event.value.clone())
        } else {
            bail!(LogEventCastError::WrongSharedSecret(self.clone()))
        }
    }
}

impl KeyExtractor for SharedSecretObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SharedSecretObject::LocalShare(event) => GenericKvKey::from(event.key.clone()),
            SharedSecretObject::SSDistribution(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SharedSecretObject {
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
pub enum SSDeviceLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    SSDeviceLog(KvLogEvent<ArtifactId, SSDistributionClaim>),
}

impl SSDeviceLogObject {
    pub fn get_unit(&self) -> anyhow::Result<VaultUnitEvent> {
        match self {
            SSDeviceLogObject::Unit(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongSSDeviceLog(self.clone())),
        }
    }

    pub fn get_genesis(&self) -> anyhow::Result<VaultGenesisEvent> {
        match self {
            SSDeviceLogObject::Genesis(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongSSDeviceLog(self.clone())),
        }
    }

    pub fn get_distribution_request(&self) -> anyhow::Result<SSDistributionClaim> {
        match self {
            SSDeviceLogObject::SSDeviceLog(event) => Ok(event.value.clone()),
            _ => bail!(LogEventCastError::WrongSSDeviceLog(self.clone())),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SSDeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SSDeviceLog(ss_device_log) = event {
            Ok(ss_device_log)
        } else {
            Err(anyhow!("Not a shared secret device log event"))
        }
    }
}

impl ObjIdExtractor for SSDeviceLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SSDeviceLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            SSDeviceLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            SSDeviceLogObject::SSDeviceLog(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SSDeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SSDeviceLog(self)
    }
}

impl KeyExtractor for SSDeviceLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SSDeviceLogObject::Unit(event) => GenericKvKey::from(event.key()),
            SSDeviceLogObject::Genesis(event) => GenericKvKey::from(event.key()),
            SSDeviceLogObject::SSDeviceLog(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for SharedSecretObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SharedSecretObject::LocalShare(event) => ObjectId::from(event.key.obj_id.clone()),
            SharedSecretObject::SSDistribution(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SharedSecretObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SharedSecret(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SSLedgerObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Ledger(KvLogEvent<ArtifactId, SSLedgerData>),
}

impl TryFrom<GenericKvLogEvent> for SSLedgerObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SSLedger(ss_obj) = &event {
            Ok(ss_obj.clone())
        } else {
            bail!(LogEventCastError::InvalidSSLedger(event))
        }
    }
}

impl SSLedgerObject {
    pub fn to_ledger_data(&self) -> anyhow::Result<SSLedgerData> {
        if let SSLedgerObject::Ledger(ledger_event) = self {
            Ok(ledger_event.value.clone())
        } else {
            bail!(LogEventCastError::WrongSSLedger(self.clone()))
        }
    }

    pub fn get_ledger_id(&self) -> anyhow::Result<ArtifactId> {
        if let SSLedgerObject::Ledger(ledger_event) = self {
            Ok(ledger_event.key.obj_id.clone())
        } else {
            bail!(LogEventCastError::WrongSSLedger(self.clone()))
        }
    }
}

impl ToGenericEvent for SSLedgerObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SSLedger(self)
    }
}

impl ObjIdExtractor for SSLedgerObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SSLedgerObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            SSLedgerObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            SSLedgerObject::Ledger(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for SSLedgerObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SSLedgerObject::Unit(event) => GenericKvKey::from(event.key()),
            SSLedgerObject::Genesis(event) => GenericKvKey::from(event.key()),
            SSLedgerObject::Ledger(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}
