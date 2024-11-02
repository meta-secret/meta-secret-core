use super::object_id::{Next, VaultGenesisEvent, VaultUnitEvent};
use crate::node::common::model::secret::{SecretDistributionData, SsDistributionClaim, SsLogData};
use crate::node::common::model::user::common::UserData;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::error::LogEventCastError;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, UnitId};
use anyhow::{anyhow, bail, Ok};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretObject {
    SsDistribution(KvLogEvent<UnitId, SecretDistributionData>),
}

impl KeyExtractor for SharedSecretObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SharedSecretObject::SsDistribution(event) => GenericKvKey::from(event.key.clone()),
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
pub enum SsDeviceLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Claim(KvLogEvent<ArtifactId, SsDistributionClaim>),
}

impl SsDeviceLogObject {
    pub fn get_unit(&self) -> anyhow::Result<VaultUnitEvent> {
        match self {
            SsDeviceLogObject::Unit(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongSsDeviceLog(self.clone())),
        }
    }

    pub fn get_genesis(&self) -> anyhow::Result<VaultGenesisEvent> {
        match self {
            SsDeviceLogObject::Genesis(event) => Ok(event.clone()),
            _ => bail!(LogEventCastError::WrongSsDeviceLog(self.clone())),
        }
    }

    pub fn get_distribution_request(&self) -> anyhow::Result<SsDistributionClaim> {
        match self {
            SsDeviceLogObject::Claim(event) => Ok(event.value.clone()),
            _ => bail!(LogEventCastError::WrongSsDeviceLog(self.clone())),
        }
    }
}

impl TryFrom<GenericKvLogEvent> for SsDeviceLogObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::SsDeviceLog(ss_device_log) = event {
            Ok(ss_device_log)
        } else {
            Err(anyhow!("Not a shared secret device log event"))
        }
    }
}

impl ObjIdExtractor for SsDeviceLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SsDeviceLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            SsDeviceLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            SsDeviceLogObject::Claim(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl ToGenericEvent for SsDeviceLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsDeviceLog(self)
    }
}

impl KeyExtractor for SsDeviceLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SsDeviceLogObject::Unit(event) => GenericKvKey::from(event.key()),
            SsDeviceLogObject::Genesis(event) => GenericKvKey::from(event.key()),
            SsDeviceLogObject::Claim(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ObjIdExtractor for SharedSecretObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SharedSecretObject::SsDistribution(event) => ObjectId::from(event.key.obj_id.clone()),
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
pub enum SsLogObject {
    Unit(VaultUnitEvent),
    Genesis(VaultGenesisEvent),
    Claims(KvLogEvent<ArtifactId, SsLogData>),
}

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
    pub fn init(user: UserData) -> Vec<GenericKvLogEvent> {
        let vault_name = user.vault_name.clone();

        //create new unit and genesis events
        let unit_key = {
            let desc = SharedSecretDescriptor::SsLog(vault_name.clone()).to_obj_desc();
            KvKey::unit(desc.clone())
        };

        let unit_event = SsLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: unit_key.clone(),
            value: vault_name,
        }));

        let genesis_event = {
            let genesis_key = unit_key.next();
            SsLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
                key: genesis_key,
                value: user,
            }))
        };

        let obj_events = vec![unit_event, genesis_event];
        obj_events.iter().map(|obj| obj.clone().to_generic()).collect()
    }
}

impl SsLogObject {
    pub fn to_data(&self) -> anyhow::Result<SsLogData> {
        if let SsLogObject::Claims(ledger_event) = self {
            Ok(ledger_event.value.clone())
        } else {
            bail!(LogEventCastError::WrongSsLog(self.clone()))
        }
    }

    pub fn get_id(&self) -> anyhow::Result<ArtifactId> {
        if let SsLogObject::Claims(ledger_event) = self {
            Ok(ledger_event.key.obj_id.clone())
        } else {
            bail!(LogEventCastError::WrongSsLogId(self.clone()))
        }
    }
}

impl ToGenericEvent for SsLogObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::SsLog(self)
    }
}

impl ObjIdExtractor for SsLogObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            SsLogObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            SsLogObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            SsLogObject::Claims(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}

impl KeyExtractor for SsLogObject {
    fn key(&self) -> GenericKvKey {
        match self {
            SsLogObject::Unit(event) => GenericKvKey::from(event.key()),
            SsLogObject::Genesis(event) => GenericKvKey::from(event.key()),
            SsLogObject::Claims(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}
