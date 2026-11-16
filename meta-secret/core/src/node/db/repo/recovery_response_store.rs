//! Narrow server-private persistence contract. No event-log conversion is provided.
//! Concrete adapters keep this row outside generic ArtifactId event storage.
use crate::node::api::SignedAction;
use crate::node::common::model::{
    device::common::DeviceId, secret::ClaimId, vault::vault::VaultName,
};
use crate::node::db::events::{object_id::ArtifactId, vault::liveboard_metadata::DecisionOutcome};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResponseSourceProof {
    pub workflow_artifact_id: ArtifactId,
    pub verified_envelope_hash: String,
    pub responder_id: DeviceId,
    pub outcome: DecisionOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryResponsePhase {
    Prepared,
    Committed,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateRecoveryResponse {
    pub version: u32,
    pub vault_name: VaultName,
    pub claim_id: ClaimId,
    pub base_ss_log_artifact_id: ArtifactId,
    pub target_ss_log_artifact_id: ArtifactId,
    pub source_proof: RecoveryResponseSourceProof,
    pub phase: RecoveryResponsePhase,
    pub packet: Option<SignedAction>,
}

impl std::fmt::Debug for PrivateRecoveryResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateRecoveryResponse")
            .field("version", &self.version)
            .field("claim_id", &self.claim_id)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RecoveryResponseStoreError {
    #[error("RecoveryResponseStore.UnsupportedBackend")]
    UnsupportedBackend,
    #[error("RecoveryResponseStore.Unavailable")]
    Unavailable,
    #[error("RecoveryResponseStore.CorruptRecord")]
    CorruptRecord,
    #[error("RecoveryResponseStore.Conflict")]
    Conflict,
}

#[async_trait(?Send)]
pub trait ServerRecoveryResponseStore {
    async fn ensure_ready(&self) -> Result<()>;
    async fn load(&self, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>>;
    async fn reserve(&self, vault: &VaultName, prepared: &PrivateRecoveryResponse) -> Result<()>;
    async fn mark_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()>;
    async fn try_remove_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()>;
}
