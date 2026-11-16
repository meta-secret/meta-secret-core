//! Actual reconstruction and optional source capture; no added legacy authorization.
use super::*;
use crate::node::common::model::liveboard_reveal::{LiveboardRevealReceipt, RecoveredReveal};
use crate::node::db::actions::recovery_reveal::{RevealEvidence, RevealInputs, try_reveal_context};

impl<Repo: KvLogEventRepo> RecoveryHandler<Repo> {
    #[instrument(skip_all)]
    pub(super) async fn recover_captured(
        &self,
        user_creds: UserCreds,
        claim_id: ClaimId,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<RecoveredReveal> {
        let vault_source = PersistentVault::from(self.p_obj.clone())
            .get_vault(user_creds.vault_name.clone())
            .await?;
        vault_source.0.value.ensure_secret_available(&pass_id)?;
        info!(
            "🔑 [recover v2] claim_id={:?} pass_id={:?}",
            claim_id, pass_id
        );

        // Create PersistentSharedSecret to access shared secret data
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

        // 2. Get the SS log to find the claim
        let vault_name = user_creds.vault_name.clone();
        let ss_log_data = p_ss.get_ss_log_obj(vault_name).await?;

        // Find the claim using the ID in the recovery_id
        let claim = ss_log_data
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found for recovery ID"))?
            .clone();

        info!(
            "🔑 [recover v2] claim statuses: {:?}",
            claim.status.statuses
        );

        // Get recoveries and distributions from the claim
        let recoveries = p_ss.get_recoveries(claim.clone()).await?;
        info!(
            "🔑 [recover v2] recovery shares count: {}",
            recoveries.len()
        );

        let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: user_creds.device_id().clone(),
        });
        let maybe_dist = self.p_obj.find_tail_event(desc).await?;

        // Extract all SecretDistributionData objects from recoveries and dists
        let recovery_data: Vec<SecretDistributionData> = recoveries
            .clone()
            .into_iter()
            .map(|r| r.to_distribution_data())
            .collect::<Result<Vec<_>, _>>()?;

        let distribution_data: Vec<SecretDistributionData> = maybe_dist
            .clone()
            .map(|dist| dist.to_distribution_data())
            .transpose()?
            .into_iter()
            .collect();

        info!(
            "🔑 [recover v2] own local share found: {}, distribution_data count: {}, recovery_data count: {}",
            !distribution_data.is_empty(),
            distribution_data.len(),
            recovery_data.len()
        );

        if recovery_data.is_empty() && distribution_data.is_empty() {
            bail!("No recovery shares found for selected claim");
        }

        // Decrypt the secret shares using the transport key
        let transport_sk = &user_creds.device_creds.secret_box.transport.sk;

        // Prepare vectors to collect all shares
        let mut user_shares = Vec::new();

        // Process recovery shares
        for data in recovery_data {
            let decrypted = data.secret_message.cipher_text().decrypt(transport_sk)?;
            let share = UserShareDto::try_from(&decrypted.msg)?;
            if let Some(block) = share.share_blocks.first() {
                info!(
                    "🔑 [recover v2] recovery share diag: share_id={}, blocks={}, threshold={}, number_of_shares={}",
                    share.share_id,
                    share.share_blocks.len(),
                    block.config.threshold,
                    block.config.number_of_shares
                );
            }
            user_shares.push(share);
        }

        // Process distribution shares
        for data in distribution_data {
            let decrypted = data.secret_message.cipher_text().decrypt(transport_sk)?;
            let share = UserShareDto::try_from(&decrypted.msg)?;
            if let Some(block) = share.share_blocks.first() {
                info!(
                    "🔑 [recover v2] own distribution share diag: share_id={}, blocks={}, threshold={}, number_of_shares={}",
                    share.share_id,
                    share.share_blocks.len(),
                    block.config.threshold,
                    block.config.number_of_shares
                );
            }
            user_shares.push(share);
        }

        // Bind only the actual loaded inputs while they are used, before Delivered.
        // Failure of optional metadata cannot change a legacy recovery result.
        let reveal_receipt = (pass_id == claim.dist_claim_id.pass_id)
            .then(|| {
                try_reveal_context(RevealInputs {
                    vault: &vault_source,
                    creds: &user_creds,
                    claims: &ss_log_data,
                    claim: &claim,
                    local: maybe_dist.as_ref(),
                    recoveries: &recoveries,
                    evidence: RevealEvidence::VerifiedDecoded(&user_shares),
                })
                .map(|reveal_context| LiveboardRevealReceipt {
                    version: 1,
                    secret_id: pass_id.id_str(),
                    recovery_claim_id: claim.id.wasm_id_str(),
                    requester_id: user_creds.device_id().to_string(),
                    reveal_context,
                })
                .filter(LiveboardRevealReceipt::is_valid)
            })
            .flatten();

        // Recover the secret using the collected shares
        let plain_text = recover_from_shares(user_shares)?;

        // Mark the sender as Delivered so compute_client_status returns Done only after
        // this device has actually recovered the secret.
        // The server intentionally skips this transition for Recovery claims (see
        // server_data_sync.rs — "Completion event needs to be sent by the recovery claim creator").
        self.mark_claim_delivered(&user_creds, &claim_id).await?;

        Ok(RecoveredReveal {
            plaintext: plain_text,
            reveal_receipt,
        })
    }
}
