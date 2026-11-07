//! Shared per-Pass removal computation; this module performs no persistence.
use super::*;
use crate::node::db::{
    events::local_guarded_preparation::{GuardedInputRef, GuardedPreparedPass},
    objects::local_guarded_preparation::local_body_commitment,
};
use anyhow::{Context, ensure};

pub(super) struct RemovalPassContext<'a> {
    pub vault: &'a VaultData,
    pub update: &'a RemoveMemberEvent,
    pub claims: &'a SsLogData,
    pub is_initiator: bool,
}

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(super) async fn compute_removal_pass(
        &self,
        context: &RemovalPassContext<'_>,
        transfer: &RemoveMemberShareTransfer,
    ) -> Result<Option<GuardedPreparedPass>> {
        let local = self.user_creds.device_id();
        let mut members = context.vault.members();
        members.sort_by_key(|member| member.user().device.device_id.to_string());
        let remaining: HashSet<_> = members
            .iter()
            .map(|member| member.user().device.device_id.clone())
            .collect();
        if context.is_initiator {
            let initiator = &context.update.request.sender.user_data.device.device_id;
            if !context.claims.claims.values().any(|claim| {
                claim.distribution_type == SecretDistributionType::Split
                    && &claim.sender == initiator
                    && claim.dist_claim_id.pass_id == transfer.pass_id
            }) {
                return Ok(None);
            }
        }
        let descriptor = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: transfer.pass_id.clone(),
            receiver: local.clone(),
        });
        let workflow = self
            .p_obj
            .find_tail_event(descriptor)
            .await?
            .context("Local share is missing during removal")?;
        let SsWorkflowObject::Distribution(distribution) = &workflow else {
            bail!("Local share is missing during removal");
        };
        ensure!(
            transfer
                .claim_id
                .as_ref()
                .is_none_or(|id| id == &distribution.value.claim_id.id),
            "Removal transfer share belongs to a different Split generation"
        );
        let local_plain = distribution
            .value
            .secret_message
            .cipher_text()
            .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
        let local_share = UserShareDto::try_from(&local_plain.msg)?;
        let threshold = local_share
            .share_blocks
            .first()
            .map(|b| b.config.threshold)
            .unwrap_or(0);
        let previous = context
            .claims
            .claims
            .get(&distribution.value.claim_id.id)
            .context("Current local share references a missing Split claim")?;
        ensure!(
            previous.distribution_type == SecretDistributionType::Split
                && previous.dist_claim_id.pass_id == transfer.pass_id,
            "Current local share references an invalid Split claim"
        );
        let topology_matches = previous.receivers.iter().all(|id| remaining.contains(id))
            && previous.receivers.len() == remaining.len() - 1;
        let config = context.vault.sss_cfg();
        if topology_matches && threshold == config.threshold {
            return Ok(None);
        }
        let local_member = members
            .iter()
            .find(|member| &member.user().device.device_id == local)
            .cloned()
            .context("Local device is not in remaining members")?;
        let generation =
            reshare_generation_claim_id(&previous.id, &serde_json::to_string(context.update)?);
        let mut claim = VaultMember {
            member: local_member,
            vault: context.vault.clone(),
        }
        .create_split_claim(transfer.pass_id.clone());
        claim.id = generation.clone();
        claim.dist_claim_id.id = generation;
        claim.vault_name = previous.vault_name.clone();
        let message: EncryptedMessage = serde_json::from_str(&transfer.encrypted_share)?;
        let transferred_plain = message
            .cipher_text()
            .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
        let transferred_share = UserShareDto::try_from(&transferred_plain.msg)?;
        let plain = recover_from_shares(vec![local_share, transferred_share])?;
        let secure = SecurePassInfo::from(PlainPassInfo {
            pass_id: transfer.pass_id.clone(),
            pass: plain.text,
        });
        let split = split2(secure, config)?;
        ensure!(
            split.shares.len() == members.len(),
            "Removal resharing produced an invalid share count"
        );
        let mut distributions = vec![];
        for (index, receiver) in members.iter().enumerate() {
            let encrypted = self
                .user_creds
                .device_creds
                .key_manager()?
                .transport
                .encrypt_string(
                    PlainText::from(split.shares[index].as_json()?),
                    &receiver.user().device.keys.transport_pk,
                )?;
            let id = SsDistributionId {
                pass_id: transfer.pass_id.clone(),
                receiver: receiver.user().device.device_id.clone(),
            };
            distributions.push(SsWorkflowObject::Distribution(KvLogEvent {
                key: KvKey::from(SsWorkflowDescriptor::Distribution(id)),
                value: SecretDistributionData {
                    vault_name: context.update.request.sender.user_data.vault_name(),
                    claim_id: claim.dist_claim_id.clone(),
                    secret_message: EncryptedMessage::CipherShare { share: encrypted },
                },
            }));
        }
        claim.receivers = members
            .iter()
            .filter_map(|member| {
                let id = member.user().device.device_id.clone();
                (&id != local).then_some(id)
            })
            .collect();
        claim.status = SsDistributionCompositeStatus::from(claim.receivers.clone());
        Ok(Some(GuardedPreparedPass {
            pass_id: transfer.pass_id.clone(),
            input: GuardedInputRef {
                distribution_id: workflow.obj_id(),
                distribution_commitment: local_body_commitment(&workflow)?,
                claim_id: distribution.value.claim_id.clone(),
            },
            claim,
            distributions,
        }))
    }
}
