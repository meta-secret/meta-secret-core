//! Exact original recovered Show policy and completion, plus optional captured receipt.
use super::*;
use meta_secret_core::node::app::app_manager_shared::recover_plain_text_by_claim_checked;
use meta_secret_core::node::common::model::liveboard_reveal::RecoveredReveal;
use meta_secret_core::node::{
    api::DataSyncResponse,
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SsLogDescriptor,
        },
        events::generic_log_event::GenericKvLogEvent,
    },
};

impl<Repo: KvLogEventRepo + Send + Sync + 'static, SyncP: SyncProtocol + Send + Sync + 'static>
    ApplicationManager<Repo, SyncP>
{
    pub async fn show_recovered(&self, claim_id: ClaimId) -> Result<PlainText> {
        Ok(self.show_recovered_checked(claim_id).await?.plaintext)
    }

    /// Show a recovered secret for the exact claim selected by Core.
    ///
    /// The Pass ID is derived from that claim, so a client cannot combine a
    /// Claim ID belonging to one secret with another secret identifier.
    pub async fn show_recovered_checked(&self, claim_id: ClaimId) -> Result<RecoveredReveal> {
        let user_creds = self.meta_client_service.find_user_creds().await?;
        let state = self.get_state().await?;

        match state {
            ApplicationState::Local(_) => {
                bail!("Show recovered is not allowed in local state");
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Show recovered is not allowed in not exists state");
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Show recovered is not allowed in outsider state");
                }
                VaultFullInfo::Member(member) => {
                    let vault_members_count = member.member.vault.members().len();
                    if vault_members_count <= 2 {
                        bail!("Recovery claim is not required for a replicated vault");
                    }

                    let claim = member
                        .ss_claims
                        .claims
                        .get(&claim_id)
                        .ok_or_else(|| anyhow::anyhow!("Claim id not found"))?;
                    if claim.distribution_type != SecretDistributionType::Recover {
                        bail!("Claim is not a recovery claim");
                    }
                    if claim.sender != *user_creds.device_id() {
                        bail!("Recovery claim belongs to another device");
                    }
                    if claim.client_status != Some(RecoveryClientStatus::Accepted) {
                        bail!("Recovery claim is not accepted");
                    }
                    let pass_id = claim.dist_claim_id.pass_id.clone();

                    info!(
                        pass_id = %pass_id.name,
                        sender = ?user_creds.device_id(),
                        claim_id = ?claim_id,
                        "mobile selected recovery claim for show_recovered"
                    );

                    let mut pass = recover_plain_text_by_claim_checked(
                        self.sync_gateway.as_ref(),
                        user_creds.clone(),
                        claim_id.clone(),
                    )
                    .await?;

                    // Send recovery completion to mark claim as Delivered
                    let ts = log_timestamp::log_timestamp_utc();
                    println!(
                        "[{ts}] 🦀 App Manager: Send recovery completion to mark claim as Delivered"
                    );
                    let receiver = claim.approved_recovery_receiver()?;
                    let vault_name = user_creds.vault_name.clone();

                    let recovery_id = SsRecoveryId {
                        claim_id: claim.dist_claim_id.clone(),
                        sender: claim.sender.clone(),
                        distribution_id: SsDistributionId {
                            pass_id: pass_id.clone(),
                            receiver,
                        },
                    };

                    let completion = SsRecoveryCompletion {
                        vault_name,
                        recovery_id,
                        receiver_status: SsDistributionStatus::Sent,
                    };

                    let key_manager = user_creds.device_creds.key_manager()?;
                    let signed = sign_recovery_completion(
                        completion,
                        user_creds.device_id().clone(),
                        &key_manager.dsa,
                    )?;
                    let sync_request =
                        SyncRequest::Read(Box::from(ReadSyncRequest::SsRecoveryCompletion(signed)));

                    let completion_response = self.server.send(sync_request).await?;
                    // Preserve legacy response behavior; unconfirmed completion
                    // cannot receive new continuity proof.
                    if !confirmed_completion(&completion_response, claim, &user_creds) {
                        pass.reveal_receipt = None;
                    }

                    // The sender's local claim is marked Delivered while
                    // recovering the secret.  Flush that terminal state before
                    // returning to the UI so the next Recover action cannot see
                    // the previous claim as still Accepted and reuse it.
                    self.sync_gateway.sync(user_creds.user()).await?;
                    info!(
                        pass_id = %pass_id.name,
                        claim_id = ?claim_id,
                        "mobile recovery completion synchronized"
                    );

                    Ok(pass)
                }
            },
        }
    }
}

/// Metadata confirmation only. The normal response is Data(updatedSsLog), not Empty.
fn confirmed_completion(
    response: &DataSyncResponse,
    expected: &SsClaim,
    creds: &UserCreds,
) -> bool {
    let DataSyncResponse::Data(data) = response else {
        return false;
    };
    let [GenericKvLogEvent::SsLog(log)] = data.0.as_slice() else {
        return false;
    };
    let descriptor = SsLogDescriptor::from(creds.vault_name.clone()).to_obj_desc();
    if log.0.key.obj_desc != descriptor
        || log.0.key.obj_id.fqdn != descriptor.fqdn()
        || log.0.key.obj_id.id.curr == 0
    {
        return false;
    }
    let Some(actual) = log.as_data().claims.get(&expected.id) else {
        return false;
    };
    let mut actual = actual.clone();
    actual.client_status = None;
    let mut expected = expected.clone();
    expected.client_status = None;
    expected.status = expected.status.complete(creds.device_id().clone());
    actual == expected
        && actual.sender == *creds.device_id()
        && actual.status.statuses.get(creds.device_id()) == Some(&SsDistributionStatus::Delivered)
        && actual.approved_recovery_receiver().ok() == expected.approved_recovery_receiver().ok()
        && actual.approved_recovery_receiver().is_ok()
}
