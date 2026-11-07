//! Read-only material availability. Never reconstructs/reveals or completes a Secret.
use crate::node::common::model::{
    meta_pass::MetaPasswordId,
    secret::{SecretDistributionData, SecretDistributionType, SsClaim, SsLogData},
    user::user_creds::UserCreds,
    vault::vault::VaultMember,
};
use crate::node::db::{
    events::shared_secret_event::SsWorkflowObject, objects::persistent_object::PersistentObject,
    repo::generic_db::KvLogEventRepo,
};
use crate::secret::{
    local_share::{
        LocalShareProfile, compatible_share_blocks, decode_local_share,
        load_local_distribution_source,
    },
    shared_secret::UserShareDto,
};
use anyhow::{Result, ensure};
use std::sync::Arc;

pub(super) struct Material<'a, R: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<R>>,
    pub creds: &'a UserCreds,
    pub member: &'a VaultMember,
    pub claims: &'a SsLogData,
}
pub(super) struct LocalMaterial {
    pub source: SsWorkflowObject,
    pub share: UserShareDto,
}
impl<R: KvLogEventRepo> Material<'_, R> {
    pub async fn local(&self, pass: &MetaPasswordId) -> Result<Option<UserShareDto>> {
        Ok(self
            .local_material(pass)
            .await?
            .map(|material| material.share))
    }
    pub async fn local_material(&self, pass: &MetaPasswordId) -> Result<Option<LocalMaterial>> {
        let vault = &self.member.vault;
        vault.ensure_secret_available(pass)?;
        vault.ensure_valid_member_count()?;
        ensure!(
            self.member.member.user_data == self.creds.user()
                && vault.vault_name == self.creds.vault_name
                && vault.is_member(self.creds.device_id()),
            "MaterialOwnerMismatch"
        );
        let whole_copy = vault.members().len() <= 2;
        if whole_copy {
            vault.ensure_local_secret_available(self.creds.device_id(), pass)?;
        }
        let Some(source) = load_local_distribution_source(&self.p_obj, self.creds, pass).await?
        else {
            return Ok(None);
        };
        let data = source.clone().to_distribution_data()?;
        let claim = self
            .claims
            .claims
            .get(&data.claim_id.id)
            .ok_or_else(|| anyhow::anyhow!("SplitNotSynchronized"))?;
        ensure!(
            claim.id == data.claim_id.id
                && claim.dist_claim_id == data.claim_id
                && claim.vault_name == self.creds.vault_name
                && claim.distribution_type == SecretDistributionType::Split,
            "SplitIdentityMismatch"
        );
        let profile = if whole_copy {
            // Canonical ownership changes after2→1; the existing encrypted
            // k1 copy retains its original origin and generation shape.
            LocalShareProfile::WholeCopy
        } else {
            let sender = data
                .secret_message
                .cipher_text()
                .channel
                .sender()
                .to_device_id();
            ensure!(
                sender == claim.sender && vault.is_member(&sender),
                "SplitSenderMismatch"
            );
            LocalShareProfile::Exact(vault.sss_cfg())
        };
        let share = decode_local_share(&data, self.creds, profile)?;
        Ok(Some(LocalMaterial { source, share }))
    }
    // Preserve the existing non-revealing availability entry point.
    #[allow(dead_code)]
    pub async fn recovered(&self, claim: &SsClaim, local: Option<&UserShareDto>) -> Result<bool> {
        let events =
            crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret::from(
                self.p_obj.clone(),
            )
            .get_recoveries(claim.clone())
            .await?;
        Ok(self
            .recovered_material(claim, local, &events)
            .await?
            .is_some())
    }
    pub async fn recovered_material(
        &self,
        claim: &SsClaim,
        local: Option<&UserShareDto>,
        events: &[SsWorkflowObject],
    ) -> Result<Option<Vec<UserShareDto>>> {
        ensure!(
            claim.sender == *self.creds.device_id() && claim.vault_name == self.creds.vault_name,
            "RecoveryOwnerMismatch"
        );
        let receiver = claim.approved_recovery_receiver()?;
        let mut shares = local.cloned().into_iter().collect::<Vec<_>>();
        for event in events {
            let data = event.clone().to_distribution_data()?;
            let channel = &data.secret_message.cipher_text().channel;
            if data.claim_id != claim.dist_claim_id
                || data.vault_name != claim.vault_name
                || channel.sender().to_device_id() != receiver
            {
                continue;
            }
            let share = self.decode(&data)?;
            if shares.iter().any(|s| s.share_id == share.share_id) {
                continue;
            }
            if shares
                .first()
                .is_some_and(|s| !compatible_share_blocks(s, &share))
            {
                return Ok(None);
            }
            shares.push(share);
        }
        if shares.len() < self.member.vault.sss_cfg().threshold {
            return Ok(None);
        }
        // Match the handler's actual complete input order: Recovery inputs, then local.
        if local.is_some() {
            let own = shares.remove(0);
            shares.push(own);
        }
        Ok(Some(shares))
    }
    fn decode(&self, data: &SecretDistributionData) -> Result<UserShareDto> {
        decode_local_share(
            data,
            self.creds,
            LocalShareProfile::Exact(self.member.vault.sss_cfg()),
        )
    }
}
