//! Private-record shape checks. All failures drop payload-bearing decoder causes.
use super::recovery_response_store::*;
use crate::crypto::utils::sha256_hex;
use crate::node::db::{
    descriptors::shared_secret_descriptor::SsLogDescriptor,
    events::{
        generic_log_event::ObjIdExtractor,
        object_id::{ArtifactId, Next},
        shared_secret_event::SsWorkflowObject,
    },
};
use crate::node::{common::model::vault::vault::VaultName, security::canonical_json};
use anyhow::Result;

impl PrivateRecoveryResponse {
    pub fn validate(&self, vault: &VaultName) -> Result<()> {
        let valid = self.version == 1
            && self.vault_name == *vault
            && self.base_ss_log_artifact_id.fqdn
                == ArtifactId::from(SsLogDescriptor::from(vault.clone())).fqdn
            && self.target_ss_log_artifact_id == self.base_ss_log_artifact_id.clone().next()
            && self.source_proof.verified_envelope_hash.len() == 64
            && self
                .source_proof
                .verified_envelope_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit());
        if !valid {
            return Err(RecoveryResponseStoreError::CorruptRecord.into());
        }
        match (&self.phase, &self.packet) {
            (RecoveryResponsePhase::Committed, None) => Ok(()),
            (RecoveryResponsePhase::Prepared, Some(packet)) => {
                let (name, claim, outcome) = match packet.event().ok().and_then(|e| e.clone().shared_secret().ok()) {
                    Some(SsWorkflowObject::Recovery(e)) => (e.value.vault_name, e.value.claim_id.id, crate::node::db::events::vault::liveboard_metadata::DecisionOutcome::Accepted),
                    Some(SsWorkflowObject::Decline(e)) => (e.value.vault_name, e.value.claim_id, crate::node::db::events::vault::liveboard_metadata::DecisionOutcome::Declined),
                    _ => return Err(RecoveryResponseStoreError::CorruptRecord.into()),
                };
                let event = packet
                    .event()
                    .map_err(|_| RecoveryResponseStoreError::CorruptRecord)?;
                let proof = recovery_source_proof(packet, outcome)?;
                if name != *vault
                    || claim != self.claim_id
                    || proof != self.source_proof
                    || packet.stream
                        != crate::node::common::model::IdString::id_str(event.obj_id().fqdn)
                    || packet.nonce != event.obj_id().id.curr as u64
                {
                    return Err(RecoveryResponseStoreError::CorruptRecord.into());
                }
                Ok(())
            }
            _ => Err(RecoveryResponseStoreError::CorruptRecord.into()),
        }
    }
    pub fn encode_private(&self, vault: &VaultName) -> Result<Vec<u8>> {
        self.validate(vault)?;
        serde_json::to_vec(self).map_err(|_| RecoveryResponseStoreError::CorruptRecord.into())
    }
    pub fn decode_private(vault: &VaultName, bytes: &[u8]) -> Result<Self> {
        let row: Self =
            serde_json::from_slice(bytes).map_err(|_| RecoveryResponseStoreError::CorruptRecord)?;
        row.validate(vault)?;
        Ok(row)
    }
    pub fn compare_reservation(&self, vault: &VaultName, existing: Option<&Self>) -> Result<()> {
        self.validate(vault)?;
        if self.phase != RecoveryResponsePhase::Prepared || existing.is_some_and(|e| e != self) {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        Ok(())
    }
    pub fn to_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<Self> {
        self.validate(vault)?;
        if self.source_proof != *source {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        let mut committed = self.clone();
        committed.phase = RecoveryResponsePhase::Committed;
        committed.packet = None;
        Ok(committed)
    }
    pub fn ensure_removable(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        self.validate(vault)?;
        if self.phase != RecoveryResponsePhase::Committed || self.source_proof != *source {
            return Err(RecoveryResponseStoreError::Conflict.into());
        }
        Ok(())
    }
}
pub fn recovery_source_proof(
    packet: &crate::node::api::SignedAction,
    outcome: crate::node::db::events::vault::liveboard_metadata::DecisionOutcome,
) -> Result<RecoveryResponseSourceProof> {
    let event = packet
        .event()
        .map_err(|_| RecoveryResponseStoreError::CorruptRecord)?;
    let text = String::from_utf8(
        canonical_json(packet).map_err(|_| RecoveryResponseStoreError::CorruptRecord)?,
    )
    .map_err(|_| RecoveryResponseStoreError::CorruptRecord)?;
    Ok(RecoveryResponseSourceProof {
        workflow_artifact_id: event.obj_id(),
        verified_envelope_hash: sha256_hex(&text),
        responder_id: packet.signer.clone(),
        outcome,
    })
}
/// Retain only the closed safe error code; never a backend/serde source chain.
pub fn private_store_error(error: anyhow::Error) -> anyhow::Error {
    use RecoveryResponseStoreError::*;
    match error.downcast_ref::<RecoveryResponseStoreError>() {
        Some(UnsupportedBackend) => UnsupportedBackend,
        Some(CorruptRecord) => CorruptRecord,
        Some(Conflict) => Conflict,
        _ => Unavailable,
    }
    .into()
}
