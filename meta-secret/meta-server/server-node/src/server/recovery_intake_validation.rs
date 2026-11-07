//! Validate only a server-created fixed initial-request continuation.
use super::server_data_sync::ServerSyncGateway;
use anyhow::Result;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        common::model::{
            secret::{SecretDistributionType, SsDistributionStatus, SsLogData},
            vault::vault_data::VaultData,
        },
        db::{
            descriptors::{
                object_descriptor::ToObjectDescriptor,
                shared_secret_descriptor::{SsDeviceLogDescriptor, SsLogDescriptor},
            },
            events::{
                generic_log_event::{ObjIdExtractor, ToGenericEvent},
                kv_log_event::{KvKey, KvLogEvent},
                object_id::{ArtifactId, Next},
                shared_secret_event::{SsDeviceLogObject, SsLogObject},
            },
            objects::persistent_vault::PersistentVault,
            repo::generic_db::KvLogEventRepo,
        },
        security::canonical_json,
    },
};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug)]
struct IntakeUnavailable;
impl std::fmt::Display for IntakeUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryRequestIntake.Unavailable")
    }
}
impl std::error::Error for IntakeUnavailable {}
pub(super) fn unavailable() -> anyhow::Error {
    IntakeUnavailable.into()
}
pub(super) fn safe<T>(result: Result<T>) -> Result<T> {
    result.map_err(|_| unavailable())
}
pub(super) fn require(valid: bool) -> Result<()> {
    if valid { Ok(()) } else { Err(unavailable()) }
}
pub(super) fn digest<T: Serialize>(value: &T) -> Result<String> {
    let bytes = safe(canonical_json(value))?;
    let text = String::from_utf8(bytes).map_err(|_| unavailable())?;
    Ok(sha256_hex(&text))
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())
}

pub(super) struct PreparedIntake {
    pub record: SsLogObject,
    pub source: SsDeviceLogObject,
}

fn initial_source_shape(source: &SsDeviceLogObject) -> bool {
    let claim = &source.0.value;
    let descriptor = SsDeviceLogDescriptor::from(claim.sender.clone());
    let receivers: HashSet<_> = claim.receivers.iter().cloned().collect();
    claim.distribution_type == SecretDistributionType::Recover
        && source.obj_id().id.curr > 0
        && claim.id == claim.dist_claim_id.id
        && claim.client_status.is_none()
        && !receivers.contains(&claim.sender)
        && receivers.len() == claim.receivers.len()
        && claim.status.statuses.len() == receivers.len()
        && claim
            .status
            .statuses
            .iter()
            .all(|(id, s)| receivers.contains(id) && *s == SsDistributionStatus::Pending)
        && source.0.key.obj_desc == descriptor.clone().to_obj_desc()
        && source.obj_id().fqdn == ArtifactId::from(descriptor).fqdn
}

/// Original receivers belong to the signed source, not today's topology.
pub(super) fn initial_source(vault: &VaultData, source: &SsDeviceLogObject) -> bool {
    let claim = &source.0.value;
    initial_source_shape(source)
        && claim.vault_name == vault.vault_name
        && vault.is_member(&claim.sender)
        && vault
            .ensure_secret_available(&claim.dist_claim_id.pass_id)
            .is_ok()
}

#[tracing::instrument(skip_all)]
pub(super) async fn validate<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    record: &SsLogObject,
) -> Result<PreparedIntake> {
    let receipt = record
        .0
        .value
        .pending_recovery_request_intake
        .as_ref()
        .ok_or_else(unavailable)?;
    let claim = record
        .0
        .value
        .claims
        .get(&receipt.claim_id)
        .ok_or_else(unavailable)?;
    let name = claim.vault_name.clone();
    let descriptor = SsLogDescriptor::from(name.clone());
    require(
        receipt.version == 1
            && receipt.requester_id == claim.sender
            && receipt.claim_id == claim.id
            && receipt.target_ss_log_artifact_id == record.obj_id()
            && record.0.key.obj_desc == descriptor.clone().to_obj_desc()
            && record.obj_id().fqdn == ArtifactId::from(descriptor.clone()).fqdn
            && hash_valid(&receipt.source_body_hash)
            && hash_valid(&receipt.verified_action_hash),
    )?;
    let source = SsDeviceLogObject(KvLogEvent {
        key: receipt.source_key.clone(),
        value: claim.clone(),
    });
    require(digest(&source)? == receipt.source_body_hash)?;
    let vault_repo = PersistentVault::from(gateway.p_obj.clone());
    let vault = safe(vault_repo.get_vault(name.clone()).await)?;
    require(
        vault.obj_id() == receipt.verified_vault_artifact_id
            && initial_source(&vault.0.value, &source),
    )?;
    let base = match &receipt.base_ss_log_artifact_id {
        Some(id) => {
            require(id.fqdn == record.obj_id().fqdn && id.clone().next() == record.obj_id())?;
            let base = safe(
                gateway
                    .p_obj
                    .repo
                    .find_one_obj::<SsLogObject>(id.clone())
                    .await,
            )?
            .ok_or_else(unavailable)?;
            require(
                base.obj_id() == *id && base.0.key.obj_desc == descriptor.clone().to_obj_desc(),
            )?;
            base.0.value
        }
        None => {
            require(record.obj_id() == ArtifactId::from(descriptor.clone()))?;
            SsLogData::default()
        }
    };
    require(
        base.pending_recovery_request_intake.is_none()
            && !base.claims.contains_key(&claim.id)
            && !base.liveboard_recovery_results.contains_key(&claim.id),
    )?;
    require(
        safe(
            base.clone()
                .with_client_status(&claim.sender)
                .find_unique_active_recovery_claim_id(&claim.sender, &claim.dist_claim_id.pass_id),
        )?
        .is_none(),
    )?;
    let mut expected = base.insert(claim.clone());
    expected.pending_recovery_request_intake = Some(receipt.clone());
    require(
        record.0.value == expected
            && record.0.key == KvKey::artifact(descriptor.to_obj_desc(), record.obj_id()),
    )?;
    if let Some(raw) = safe(gateway.p_obj.repo.find_one(source.obj_id()).await)? {
        require(raw == source.clone().to_generic())?;
    }
    let store = safe(gateway.p_obj.repo.server_recovery_response_store())?;
    require(safe(store.load(&name).await)?.is_none())?;
    if let Some(log) = safe(vault_repo.vault_log(name).await)? {
        require(log.0.value.liveboard_removal_applications.is_empty())?;
    }
    Ok(PreparedIntake {
        record: record.clone(),
        source,
    })
}
