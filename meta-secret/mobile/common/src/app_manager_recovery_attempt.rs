//! Preserve the existing mobile read/FIFO path; only final entry gains a fresh owned check.
use super::ApplicationManager;
use anyhow::{Result, bail};
use meta_secret_core::node::{
    app::{meta_app::RecoveryApproveAttempt, sync::sync_protocol::SyncProtocol},
    common::model::{ApplicationState, VaultFullInfo, secret::ClaimId, user::common::UserData},
    db::repo::generic_db::KvLogEventRepo,
};
use tracing::info;
impl<Repo: KvLogEventRepo + Send + Sync + 'static, SyncP: SyncProtocol + Send + Sync + 'static>
    ApplicationManager<Repo, SyncP>
{
    pub async fn accept_recover_mobile_attempt(&self, claim_id: ClaimId) -> RecoveryApproveAttempt {
        info!(claim_id = ?claim_id, "accept_recover_mobile: started");
        let user = match self.meta_client_service.find_user_creds().await {
            Ok(user) => user,
            Err(error) => {
                return RecoveryApproveAttempt {
                    result: Err(error),
                    not_submitted: None,
                };
            }
        };
        info!(claim_id = ?claim_id, "accept_recover_mobile: syncing before claim lookup");
        if let Err(original) = self.sync_gateway.sync(user.user()).await {
            return self
                .meta_client_service
                .recover_approve_presync_failure(claim_id, user.user(), original)
                .await;
        }
        info!(claim_id = ?claim_id, "accept_recover_mobile: pre-accept sync completed");
        RecoveryApproveAttempt {
            result: self
                .approve_after_original_mobile_read(claim_id, user.user())
                .await,
            not_submitted: None,
        }
    }

    async fn approve_after_original_mobile_read(
        &self,
        claim_id: ClaimId,
        expected: UserData,
    ) -> Result<()> {
        // Original get_state keeps its pre-sync, FIFO command and service_state publication.
        let state = self.get_state().await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state")
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member")
        };
        let claim = member
            .ss_claims
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?;
        info!(claim_id = ?claim_id, sender = ?claim.sender, client_status = ?claim.client_status,
            "accept_recover_mobile: claim found, dispatching approval");
        self.meta_client_service
            .accept_recover_revalidated(claim_id, expected)
            .await
    }
}
