//! A distinct technical authorization. It carries no domain command or past proof.
use crate::crypto::{
    encoding::base64::Base64Text, key_pair::DsaKeyPair, keys::DsaPk, utils::sha256_hex,
};
use crate::node::{
    common::model::{
        IdString, device::common::DeviceId, user::common::UserId, vault::vault::VaultName,
    },
    db::events::object_id::ArtifactId,
    security::canonical_json,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const DEVICE_TRANSPORT_NOOP_PURPOSE: &str = "meta-secret.device-transport-noop.v1";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "AuthorizationWire")]
pub struct DeviceTransportNoop {
    pub version: u32,
    pub purpose: String,
    pub vault_name: VaultName,
    pub actor_id: DeviceId,
    pub event_id: ArtifactId,
    pub stream: String,
    pub nonce: u64,
    pub client_entry_commitment: String,
    pub signature: Base64Text,
}

impl std::fmt::Debug for DeviceTransportNoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceTransportNoop")
            .field("event_id", &self.event_id)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthorizationWire {
    version: u32,
    purpose: String,
    vault_name: VaultName,
    actor_id: DeviceId,
    event_id: StrictArtifactId,
    stream: String,
    nonce: u64,
    client_entry_commitment: String,
    signature: Base64Text,
}

// These strict intermediates apply ONLY to the new purpose. Old signed schemas
// retain their existing serialization and deserialization behavior.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictArtifactId {
    fqdn: StrictFqdn,
    id: StrictSequence,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictFqdn {
    obj_type: String,
    obj_instance: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictSequence {
    curr: usize,
    prev: usize,
}

impl TryFrom<AuthorizationWire> for DeviceTransportNoop {
    type Error = anyhow::Error;
    fn try_from(w: AuthorizationWire) -> Result<Self> {
        let authorization = Self {
            version: w.version,
            purpose: w.purpose,
            vault_name: w.vault_name,
            actor_id: w.actor_id,
            event_id: serde_json::from_value(serde_json::to_value(w.event_id)?)?,
            stream: w.stream,
            nonce: w.nonce,
            client_entry_commitment: w.client_entry_commitment,
            signature: w.signature,
        };
        authorization.validate_shape()?;
        Ok(authorization)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsignedAuthorization<'a> {
    version: u32,
    purpose: &'a str,
    vault_name: &'a VaultName,
    actor_id: &'a DeviceId,
    event_id: &'a ArtifactId,
    stream: &'a str,
    nonce: u64,
    client_entry_commitment: &'a str,
}

pub fn is_transport_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Check the complete sequence without changing legacy ArtifactId/SeqId serde.
pub fn validate_transport_id(id: &ArtifactId) -> Result<()> {
    let seq: StrictSequence = serde_json::from_value(serde_json::to_value(&id.id)?)?;
    ensure!(
        seq.curr > 0
            && seq.prev.checked_add(1) == Some(seq.curr)
            && seq.curr.checked_add(1).is_some(),
        "InvalidDeviceTransportSequence"
    );
    Ok(())
}

pub fn next_transport_id(id: &ArtifactId) -> Result<ArtifactId> {
    validate_transport_id(id)?;
    let mut next = id.clone();
    next.id = serde_json::from_value(serde_json::json!({
        "curr": id.id.curr.checked_add(1).ok_or_else(|| anyhow::anyhow!("InvalidDeviceTransportSequence"))?,
        "prev": id.id.curr,
    }))?;
    Ok(next)
}

impl DeviceTransportNoop {
    pub(crate) fn sign(
        owner: &UserId,
        event_id: ArtifactId,
        client_entry_commitment: String,
        key: &DsaKeyPair,
    ) -> Result<Self> {
        validate_transport_id(&event_id)?;
        let stream = event_id.fqdn.clone().id_str();
        let nonce = u64::try_from(event_id.id.curr)?;
        let unsigned = UnsignedAuthorization {
            version: 1,
            purpose: DEVICE_TRANSPORT_NOOP_PURPOSE,
            vault_name: &owner.vault_name,
            actor_id: &owner.device_id,
            event_id: &event_id,
            stream: &stream,
            nonce,
            client_entry_commitment: &client_entry_commitment,
        };
        let signature = key.sign(String::from_utf8(canonical_json(&unsigned)?)?);
        let authorization = Self {
            version: 1,
            purpose: DEVICE_TRANSPORT_NOOP_PURPOSE.into(),
            vault_name: owner.vault_name.clone(),
            actor_id: owner.device_id.clone(),
            event_id,
            stream,
            nonce,
            client_entry_commitment,
            signature,
        };
        authorization.validate_shape()?;
        Ok(authorization)
    }

    pub fn validate_shape(&self) -> Result<()> {
        validate_transport_id(&self.event_id)?;
        ensure!(
            self.version == 1
                && self.purpose == DEVICE_TRANSPORT_NOOP_PURPOSE
                && self.nonce == u64::try_from(self.event_id.id.curr)?
                && is_transport_digest(&self.client_entry_commitment)
                && canonical_json(self)?.len() <= 4096,
            "InvalidDeviceTransportNoop"
        );
        Ok(())
    }
    pub fn verify(&self, key: &DsaPk) -> Result<()> {
        self.validate_shape()?;
        let unsigned = UnsignedAuthorization {
            version: self.version,
            purpose: &self.purpose,
            vault_name: &self.vault_name,
            actor_id: &self.actor_id,
            event_id: &self.event_id,
            stream: &self.stream,
            nonce: self.nonce,
            client_entry_commitment: &self.client_entry_commitment,
        };
        key.verify(
            &String::from_utf8(canonical_json(&unsigned)?)?,
            &self.signature,
        )
    }
    pub fn authorization_hash(&self) -> Result<String> {
        Ok(sha256_hex(&String::from_utf8(canonical_json(self)?)?))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportNoopState {
    NoopCommitted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportNoopEffect {
    NotApplied,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceTransportNoopReceipt {
    pub version: u32,
    pub event_id: ArtifactId,
    pub actor_id: DeviceId,
    pub vault_name: VaultName,
    pub client_entry_commitment: String,
    pub authorization_hash: String,
    pub transport_state: TransportNoopState,
    pub record_effect: TransportNoopEffect,
}
