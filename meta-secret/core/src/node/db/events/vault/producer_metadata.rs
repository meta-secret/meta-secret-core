//! Server-created admission metadata; never a field of a signed legacy request.
use crate::crypto::utils::sha256_hex;
use crate::node::common::model::{device::common::DeviceId, vault::vault::VaultName};
use crate::node::db::events::{
    object_id::ArtifactId,
    vault::{device_log_event::DeviceLogObject, vault_log_event::VaultActionEvents},
};
use crate::node::security::canonical_json;
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerifiedProducerKind {
    DeleteRequest,
    RemoveRequest,
    LocalRemoveAccept,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerCompletionMode {
    RemotePending,
    AutomaticPending,
    AutomaticComplete,
    TerminalObserved,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "camelCase")]
pub enum VerifiedProducerSubject {
    Secret(String),
    Device(DeviceId),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedProducerSourceRef {
    pub kind: VerifiedProducerKind,
    pub source_event_id: ArtifactId,
    pub body_commitment: String,
    pub actor_id: DeviceId,
    pub vault_name: VaultName,
    pub request_id: String,
    pub subject: VerifiedProducerSubject,
    pub verified_envelope_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_mode: Option<ProducerCompletionMode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomaticDeletePhase {
    Prepared,
    PurgePending,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticDeleteApplication {
    pub request_id: String,
    pub source_event_id: ArtifactId,
    pub source_body_commitment: String,
    pub base_vault_artifact_id: ArtifactId,
    pub target_vault_artifact_id: ArtifactId,
    pub phase: AutomaticDeletePhase,
}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($ty), "(<redacted>)"))
            }
        }
    )+};
}
redacted_debug!(
    VerifiedProducerSubject,
    VerifiedProducerSourceRef,
    AutomaticDeleteApplication
);

/// Identity of exact original source bytes, including its complete key.
/// This hash is not proof of signature verification or admission.
pub fn producer_body_commitment(source: &DeviceLogObject) -> Result<String> {
    let bytes = canonical_json(source).map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?;
    let body = String::from_utf8(bytes).map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?;
    Ok(sha256_hex(&format!(
        "MetaSecret.LiveboardProducer.Source.v1\n{body}"
    )))
}

impl VaultActionEvents {
    pub fn producer_source(&self, id: &ArtifactId) -> Result<Option<&VerifiedProducerSourceRef>> {
        let mut matching = self
            .producer_source_refs
            .iter()
            .filter(|r| r.source_event_id == *id);
        let selected = matching.next();
        ensure!(matching.next().is_none(), "ProducerMetadata.Unavailable");
        Ok(selected)
    }

    /// Only an already verified server admission may call this writer.
    pub fn add_verified_producer_source(
        &mut self,
        source: VerifiedProducerSourceRef,
    ) -> Result<()> {
        if let Some(existing) = self.producer_source(&source.source_event_id)? {
            ensure!(*existing == source, "ProducerMetadata.Unavailable");
        } else {
            self.producer_source_refs.push(source);
        }
        Ok(())
    }

    pub fn without_private_producer_metadata(mut self) -> Self {
        self.producer_source_refs.clear();
        self.automatic_delete_application = None;
        self
    }
}
