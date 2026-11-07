//! Native Recover-Approve failure evidence; never a permission or acknowledgement.
use super::meta_client_service::MetaClientService;
use crate::node::{
    app::sync::sync_protocol::SyncProtocol,
    common::model::{
        ApplicationState, VaultFullInfo,
        liveboard::LiveboardOperationKind,
        secret::{ClaimId, SsDistributionStatus},
        user::{
            common::{UserData, UserMembership},
            user_creds::UserCreds,
        },
        vault::vault::{VaultMember, VaultStatus},
    },
    db::{
        descriptors::{
            shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
            vault_descriptor::{VaultDescriptor, VaultStatusDescriptor},
        },
        events::{generic_log_event::ObjIdExtractor, object_id::ArtifactId},
        objects::persistent_vault::PersistentVault,
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, bail, ensure};
use serde::Serialize;
use std::collections::HashSet;

/// The original failure is private transport state, not part of this public projection.
pub struct RecoveryApproveAttempt {
    pub result: Result<()>,
    pub not_submitted: Option<RecoveryApproveNotSubmitted>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryApproveNotSubmitted {
    schema_version: u8,
    kind: &'static str,
    pub claim_id: String,
    pub actor_id: String,
    pub vault_name: String,
    phase: &'static str,
}
impl<Repo: KvLogEventRepo, SyncP: SyncProtocol> MetaClientService<Repo, SyncP> {
    /// Only this invocation's failed first sync can request before-dispatch evidence.
    pub async fn recover_approve_presync_failure(
        &self,
        claim_id: ClaimId,
        expected: UserData,
        original: anyhow::Error,
    ) -> RecoveryApproveAttempt {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let not_submitted = match self.find_user_creds().await {
            Ok(user) if user.user() == expected => self
                .recover_approve_absence_owned(&user, &claim_id)
                .await
                .ok(),
            _ => None,
        };
        RecoveryApproveAttempt {
            result: Err(original),
            not_submitted,
        }
    }

    /// Original mobile FIFO state publication has completed before entering here.
    /// One additional fresh canonical read closes the read-to-response writer race.
    pub async fn accept_recover_revalidated(
        &self,
        claim_id: ClaimId,
        expected: UserData,
    ) -> Result<()> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let user = self.find_user_creds().await?;
        ensure!(
            user.user() == expected,
            "Recovery approval identity changed"
        );
        let state = self.get_app_state_owned().await?;
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            bail!("Not a member")
        };
        let projection = member
            .liveboard_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Recovery approval unavailable"))?;
        let matches: Vec<_> = projection
            .operations
            .iter()
            .filter(|operation| {
                operation.kind == LiveboardOperationKind::Recover
                    && operation.request_id == claim_id.wasm_id_str()
            })
            .collect();
        let [operation] = matches.as_slice() else {
            bail!("Recovery approval unavailable")
        };
        ensure!(
            projection.current_device_id == user.device_id().to_string()
                && projection.vault_name == user.vault_name.to_string()
                && operation.can_approve,
            "Recovery approval unavailable"
        );
        self.recover_approve_absence_owned(&user, &claim_id).await?;
        self.accept_recover_owned(claim_id.clone()).await?;
        tracing::info!(claim_id = ?claim_id, "accept_recover_mobile: approval event created");
        self.sync_gateway.sync(user.user()).await?;
        tracing::info!(claim_id = ?claim_id, "accept_recover_mobile: approval synced");
        Ok(())
    }

    /// Caller owns client_operation_guard; client -> sync is the existing lock order.
    async fn recover_approve_absence_owned(
        &self,
        expected: &UserCreds,
        claim_id: &ClaimId,
    ) -> Result<RecoveryApproveNotSubmitted> {
        let _sync = self.p_obj.device_sync_guard.lock().await;
        let current = self.find_user_creds().await?;
        ensure!(
            current.user() == expected.user() && current.device() == self.device_data,
            "Recovery approval identity changed"
        );
        let status_descriptor = VaultStatusDescriptor::from(current.user_id());
        let status = self
            .p_obj
            .find_tail_event(status_descriptor.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Recovery approval state unavailable"))?;
        ensure!(
            status.obj_id().fqdn == ArtifactId::from(status_descriptor).fqdn,
            "Recovery approval identity changed"
        );
        let VaultStatus::Member(actor) = status.0.value else {
            bail!("Not a member")
        };
        ensure!(
            actor.user_data == current.user(),
            "Recovery approval identity changed"
        );
        let vault_object = PersistentVault::from(self.p_obj.clone())
            .get_vault(current.vault_name.clone())
            .await?;
        ensure!(
            vault_object.obj_id().fqdn
                == ArtifactId::from(VaultDescriptor::from(current.vault_name.clone())).fqdn,
            "Recovery approval identity changed"
        );
        let vault = vault_object.to_data();
        ensure!(
            vault.vault_name == current.vault_name,
            "Recovery approval identity changed"
        );
        ensure!(
            matches!(vault.find_user(current.device_id()), Some(
            UserMembership::Member(member))
            if member.user_data == current.user()),
            "Recovery approval identity changed"
        );
        let log_descriptor = SsLogDescriptor::from(current.vault_name.clone());
        let log = self
            .p_obj
            .find_tail_event(log_descriptor.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Recovery approval state unavailable"))?;
        ensure!(
            log.obj_id().fqdn == ArtifactId::from(log_descriptor).fqdn,
            "Recovery approval identity changed"
        );
        let claims = log.to_data().with_client_status(current.device_id());
        let claim = claims
            .claims
            .get(claim_id)
            .ok_or_else(|| anyhow::anyhow!("Recovery approval state unavailable"))?;
        ensure!(
            claim.id == *claim_id
                && claim.dist_claim_id.id == *claim_id
                && claim.vault_name == current.vault_name
                && vault.secrets.contains(&claim.dist_claim_id.pass_id)
                && !vault.deleted_secrets.contains(&claim.dist_claim_id.pass_id),
            "Recovery approval identity changed"
        );
        // The legacy projection accepts a vacuous/all-present Pending view. Evidence
        // and final checked entry require the complete actual receiver/status binding.
        let receivers: HashSet<_> = claim.receivers.iter().cloned().collect();
        ensure!(
            !receivers.is_empty()
                && receivers.len() == claim.receivers.len()
                && !receivers.contains(&claim.sender)
                && receivers.contains(current.device_id())
                && claim.status.statuses.len() == receivers.len()
                && claim.status.statuses.get(current.device_id())
                    == Some(&SsDistributionStatus::Pending)
                && claim
                    .status
                    .statuses
                    .iter()
                    .all(|(receiver, status)| receivers.contains(receiver)
                        && *status == SsDistributionStatus::Pending),
            "Recovery approval status unavailable"
        );
        let events = PersistentVault::from(self.p_obj.clone())
            .vault_log(current.vault_name.clone())
            .await?
            .map(|value| value.0.value)
            .unwrap_or_default();
        let member = VaultMember {
            member: actor,
            vault,
        };
        let operations = super::liveboard_operations::operations(&member, &events, &claims);
        let matching: Vec<_> = operations
            .iter()
            .filter(|operation| {
                operation.kind == LiveboardOperationKind::Recover
                    && operation.request_id == claim_id.wasm_id_str()
            })
            .collect();
        let [operation] = matching.as_slice() else {
            bail!("Recovery approval unavailable")
        };
        ensure!(
            operation.can_approve
                && operation.outcome.is_none()
                && !claims.liveboard_recovery_results.contains_key(claim_id),
            "Recovery approval unavailable"
        );
        let own_sources: Vec<_> = claim
            .recovery_db_ids()
            .into_iter()
            .filter(|source| &source.distribution_id.receiver == current.device_id())
            .collect();
        ensure!(own_sources.len() == 1, "Recovery approval identity changed");
        for source in own_sources {
            ensure!(
                source.claim_id == claim.dist_claim_id
                    && source.sender == claim.sender
                    && source.distribution_id.pass_id == claim.dist_claim_id.pass_id,
                "Recovery approval identity changed"
            );
            // The actual producer writes the first fixed descriptor key. Any value,
            // including a malformed body, vetoes absence; it is never interpreted as free.
            for descriptor in [
                SsWorkflowDescriptor::Recovery(source.clone()),
                SsWorkflowDescriptor::Decline(source),
            ] {
                ensure!(
                    self.p_obj
                        .repo
                        .find_one(ArtifactId::from(descriptor))
                        .await?
                        .is_none(),
                    "Recovery response already exists"
                );
            }
        }
        Ok(RecoveryApproveNotSubmitted {
            schema_version: 1,
            kind: "recoverApprove",
            claim_id: claim_id.wasm_id_str(),
            actor_id: current.device_id().to_string(),
            vault_name: current.vault_name.to_string(),
            phase: "notSubmitted",
        })
    }
}
