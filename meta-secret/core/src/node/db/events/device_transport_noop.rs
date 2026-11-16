//! Server-verified transport slot; never a Vault action or public event DTO.
use super::{generic_log_event::*, kv_log_event::*, object_id::ArtifactId};
use crate::{
    crypto::{keys::DsaPk, utils::sha256_hex},
    node::{
        common::model::{IdString, user::common::UserId},
        db::descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        device_transport_noop::*,
        security::canonical_json,
    },
};
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceTransportNoopRecord {
    pub authorization: DeviceTransportNoop,
    pub authorization_hash: String,
    pub registered_key_fingerprint: String,
    pub record_effect: TransportNoopEffect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTransportNoopObject(pub KvLogEvent<DeviceTransportNoopRecord>);

impl DeviceTransportNoopObject {
    pub fn new(authorization: DeviceTransportNoop, user: &UserId, key: &DsaPk) -> Result<Self> {
        let object = Self(KvLogEvent {
            key: KvKey::artifact(
                DeviceLogDescriptor::from(user.clone()).to_obj_desc(),
                authorization.event_id.clone(),
            ),
            value: DeviceTransportNoopRecord {
                authorization_hash: authorization.authorization_hash()?,
                registered_key_fingerprint: key_fingerprint(key)?,
                authorization,
                record_effect: TransportNoopEffect::NotApplied,
            },
        });
        object.validate(user, key)?;
        Ok(object)
    }
    pub fn validate(&self, user: &UserId, key: &DsaPk) -> Result<()> {
        let auth = &self.0.value.authorization;
        let descriptor = DeviceLogDescriptor::from(user.clone());
        let expected = ArtifactId::from(descriptor.clone());
        ensure!(
            self.0.key.obj_id == auth.event_id
                && self.0.key.obj_desc == descriptor.to_obj_desc()
                && auth.event_id.fqdn == expected.fqdn
                && auth.stream == expected.fqdn.id_str()
                && auth.actor_id == user.device_id
                && auth.vault_name == user.vault_name
                && self.0.value.authorization_hash == auth.authorization_hash()?
                && self.0.value.registered_key_fingerprint == key_fingerprint(key)?,
            "InvalidDeviceTransportSlot"
        );
        auth.verify(key)
    }
    pub fn receipt(&self) -> DeviceTransportNoopReceipt {
        let a = &self.0.value.authorization;
        DeviceTransportNoopReceipt {
            version: a.version,
            event_id: a.event_id.clone(),
            actor_id: a.actor_id.clone(),
            vault_name: a.vault_name.clone(),
            client_entry_commitment: a.client_entry_commitment.clone(),
            authorization_hash: self.0.value.authorization_hash.clone(),
            transport_state: TransportNoopState::NoopCommitted,
            record_effect: TransportNoopEffect::NotApplied,
        }
    }
}
fn key_fingerprint(key: &DsaPk) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(key)?)?))
}
impl ToGenericEvent for DeviceTransportNoopObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::DeviceTransportNoop(self)
    }
}
impl KeyExtractor for DeviceTransportNoopObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
impl ObjIdExtractor for DeviceTransportNoopObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}
