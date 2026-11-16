//! Exact obsolete-target cleanup after local legacy publication, never domain approval.
use super::*;
use crate::node::{
    common::model::{meta_pass::MetaPasswordId, user::common::UserDataOutsiderStatus},
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SsLogDescriptor,
        },
        events::{
            generic_log_event::ToGenericEvent, local_guarded_preparation::GuardedPreparedPass,
            object_id::ArtifactId, shared_secret_event::SsLogObject,
        },
        objects::local_guarded_preparation::local_body_commitment,
    },
};
use anyhow::{Context, ensure};

pub(super) struct LegacyTargetCleanup {
    id: ArtifactId,
    packet: Option<SsWorkflowObject>,
}

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(super) fn validate_legacy_claim_object(
        claim: &SsClaim,
        object: &SsDeviceLogObject,
    ) -> Result<()> {
        let descriptor = SsDeviceLogDescriptor::from(claim.sender.clone());
        ensure!(
            object.0.value == *claim
                && object.0.key.obj_desc == descriptor.clone().to_obj_desc()
                && object.obj_id().fqdn == ArtifactId::from(descriptor).fqdn,
            "LegacyCleanupClaimUnavailable"
        );
        Ok(())
    }

    pub(super) async fn verify_legacy_final_readbacks(
        &self,
        prepared: &GuardedPreparedPass,
        own_claim: &SsDeviceLogObject,
        aggregate: &SsLogObject,
    ) -> Result<()> {
        Self::validate_legacy_claim_object(&prepared.claim, own_claim)?;
        let descriptor = SsLogDescriptor::from(prepared.claim.vault_name.clone());
        ensure!(
            aggregate.0.key.obj_desc == descriptor.clone().to_obj_desc()
                && aggregate.obj_id().fqdn == ArtifactId::from(descriptor).fqdn
                && aggregate.0.value.claims.get(&prepared.claim.id) == Some(&prepared.claim),
            "LegacyCleanupClaimUnavailable"
        );
        // This barrier is AFTER all four writes, in addition to per-save readbacks.
        for packet in &prepared.distributions {
            ensure!(
                self.p_obj.repo.find_one(packet.obj_id()).await?
                    == Some(packet.clone().to_generic()),
                "LegacyCleanupOutputUnavailable"
            );
        }
        ensure!(
            self.p_obj.repo.find_one(own_claim.obj_id()).await?
                == Some(own_claim.clone().to_generic()),
            "LegacyCleanupClaimUnavailable"
        );
        ensure!(
            self.p_obj.repo.find_one(aggregate.obj_id()).await?
                == Some(aggregate.clone().to_generic()),
            "LegacyCleanupClaimUnavailable"
        );
        Ok(())
    }

    pub(super) async fn prepare_legacy_target_cleanup(
        &self,
        vault: &VaultData,
        update: &RemoveMemberEvent,
        transfer: &RemoveMemberShareTransfer,
        prepared: &GuardedPreparedPass,
    ) -> Result<LegacyTargetCleanup> {
        self.legacy_cleanup_context(vault, update)?;
        self.exclude_guarded_legacy_cleanup(update, Some(&prepared.claim))
            .await?;
        ensure!(
            transfer.pass_id == prepared.pass_id
                && transfer.receiver == *self.user_creds.device_id()
                && transfer.claim_id.as_ref() == Some(&prepared.input.claim_id.id),
            "LegacyCleanupInputUnavailable"
        );
        let source = self
            .p_obj
            .repo
            .find_one(prepared.input.distribution_id.clone())
            .await?
            .context("LegacyCleanupInputUnavailable")?
            .shared_secret()?;
        let data = join_source::packet(
            &source,
            &vault.vault_name,
            &prepared.pass_id,
            self.user_creds.device_id(),
        )?;
        ensure!(
            source.obj_id() == prepared.input.distribution_id
                && data.claim_id == prepared.input.claim_id
                && local_body_commitment(&source)? == prepared.input.distribution_commitment,
            "LegacyCleanupInputUnavailable"
        );
        let log = self.get_ss_log_data().await?;
        let original = log
            .claims
            .get(&data.claim_id.id)
            .context("LegacyCleanupOldGenerationUnavailable")?;
        let sender = [
            &update.sender.user_data,
            &update.request.sender.user_data,
            &update.target.user_data,
        ]
        .into_iter()
        .find(|user| user.device.device_id == original.sender)
        .context("LegacyCleanupOldSenderUnavailable")?;
        ensure!(
            data.secret_message.cipher_text().channel.sender() == &sender.device.keys.transport_pk
                && data.secret_message.cipher_text().channel.receiver()
                    == &self.user_creds.device_creds.device.keys.transport_pk,
            "LegacyCleanupInputUnavailable"
        );
        self.capture_legacy_target(vault, update, transfer).await
    }

    fn legacy_cleanup_context(&self, vault: &VaultData, update: &RemoveMemberEvent) -> Result<()> {
        use crate::node::db::events::vault::vault_log_event::RemoveMemberDecision;
        let local = self.user_creds.user();
        let requester = &update.request.sender.user_data;
        let target = &update.target.user_data;
        let members = vault.members();
        ensure!(
            update.decision == RemoveMemberDecision::Accepted
                && update.sender.user_data == local
                && update.target == update.request.target
                && requester.vault_name == vault.vault_name
                && target.vault_name == vault.vault_name
                && local.vault_name == vault.vault_name
                && requester.device.device_id != local.device.device_id
                && target.device.device_id != local.device.device_id
                && target.device.device_id != requester.device.device_id
                && matches!(&update.new_status, UserMembership::Outsider(o) if o.user_data == *target && o.status == UserDataOutsiderStatus::Declined)
                && members.len() == 2
                && members.iter().any(|m| m.user_data == local)
                && members.iter().any(|m| m.user_data == *requester)
                && !vault.is_member(&target.device.device_id),
            "LegacyCleanupContextUnavailable"
        );
        Ok(())
    }

    async fn capture_legacy_target(
        &self,
        vault: &VaultData,
        update: &RemoveMemberEvent,
        transfer: &RemoveMemberShareTransfer,
    ) -> Result<LegacyTargetCleanup> {
        let old = transfer
            .claim_id
            .as_ref()
            .context("LegacyCleanupOldGenerationUnavailable")?;
        let log = self.get_ss_log_data().await?;
        let claim = log
            .claims
            .get(old)
            .context("LegacyCleanupOldGenerationUnavailable")?;
        ensure!(
            claim.id == *old
                && claim.dist_claim_id.id == *old
                && claim.dist_claim_id.pass_id == transfer.pass_id
                && claim.vault_name == vault.vault_name
                && claim.distribution_type == SecretDistributionType::Split,
            "LegacyCleanupOldGenerationUnavailable"
        );
        let target = &update.target.user_data;
        let descriptor = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: transfer.pass_id.clone(),
            receiver: target.device.device_id.clone(),
        });
        let id = ArtifactId::from(descriptor);
        let packet = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await?
            .map(|e| e.shared_secret())
            .transpose()?;
        if let Some(packet) = &packet {
            let data = join_source::packet(
                packet,
                &vault.vault_name,
                &transfer.pass_id,
                &target.device.device_id,
            )?;
            let sender = [
                &update.sender.user_data,
                &update.request.sender.user_data,
                target,
            ]
            .into_iter()
            .find(|user| user.device.device_id == claim.sender)
            .context("LegacyCleanupOldSenderUnavailable")?;
            ensure!(
                data.claim_id == claim.dist_claim_id
                    && data.secret_message.cipher_text().channel.sender()
                        == &sender.device.keys.transport_pk
                    && data.secret_message.cipher_text().channel.receiver()
                        == &target.device.keys.transport_pk,
                "LegacyCleanupTargetConflict"
            );
        }
        Ok(LegacyTargetCleanup { id, packet })
    }

    pub(super) async fn finish_legacy_target_cleanup(
        &self,
        cleanup: LegacyTargetCleanup,
    ) -> Result<()> {
        let current = self.p_obj.repo.find_one(cleanup.id.clone()).await?;
        ensure!(
            current == cleanup.packet.as_ref().map(|p| p.clone().to_generic()),
            "LegacyCleanupTargetConflict"
        );
        if cleanup.packet.is_some() {
            self.p_obj.repo.try_delete(cleanup.id.clone()).await?;
        }
        ensure!(
            self.p_obj.repo.find_one(cleanup.id).await?.is_none(),
            "LegacyCleanupTargetRetained"
        );
        Ok(())
    }

    pub(super) async fn retry_legacy_target_cleanup(
        &self,
        vault: &VaultData,
        expected: Option<&RemoveMemberEvent>,
    ) -> Result<bool> {
        let Some(update) = self.last_legacy_cleanup_update().await? else {
            return Ok(false);
        };
        if expected.is_some_and(|expected| expected != &update) {
            return Ok(false);
        }
        self.legacy_cleanup_context(vault, &update)?;
        let transfers: Vec<_> = update
            .request
            .share_transfers
            .iter()
            .filter(|t| {
                t.receiver == *self.user_creds.device_id()
                    && vault.secrets.contains(&t.pass_id)
                    && !vault.deleted_secrets.contains(&t.pass_id)
            })
            .collect();
        if transfers.is_empty() {
            return Ok(false);
        }
        let log = self.get_ss_log_data().await?;
        let mut pending = vec![];
        for transfer in transfers {
            let old = transfer
                .claim_id
                .as_ref()
                .context("LegacyCleanupOldGenerationUnavailable")?;
            let generation = reshare_generation_claim_id(old, &serde_json::to_string(&update)?);
            let source = self
                .legacy_cleanup_packet(&transfer.pass_id, self.user_creds.device_id())
                .await?
                .context("LegacyCleanupCurrentSourceUnavailable")?;
            let data = source.clone().to_distribution_data()?;
            if data.claim_id.id != generation {
                return Ok(false);
            }
            let claim = log
                .claims
                .get(&generation)
                .context("LegacyCleanupCurrentClaimUnavailable")?;
            let remotes: HashSet<_> = vault
                .members()
                .iter()
                .filter_map(|m| {
                    let id = &m.user_data.device.device_id;
                    (id != self.user_creds.device_id()).then_some(id.clone())
                })
                .collect();
            ensure!(
                claim.id == generation
                    && claim.dist_claim_id == data.claim_id
                    && claim.vault_name == vault.vault_name
                    && claim.dist_claim_id.pass_id == transfer.pass_id
                    && claim.sender == *self.user_creds.device_id()
                    && claim.distribution_type == SecretDistributionType::Split
                    && claim.receivers.len() == remotes.len()
                    && claim.receivers.iter().cloned().collect::<HashSet<_>>() == remotes,
                "LegacyCleanupCurrentClaimUnavailable"
            );
            self.exclude_guarded_legacy_cleanup(&update, Some(claim))
                .await?;
            for member in vault.members() {
                if let Some(packet) = self
                    .legacy_cleanup_packet(&transfer.pass_id, &member.user_data.device.device_id)
                    .await?
                {
                    let data = join_source::packet(
                        &packet,
                        &vault.vault_name,
                        &transfer.pass_id,
                        &member.user_data.device.device_id,
                    )?;
                    ensure!(
                        data.claim_id == claim.dist_claim_id
                            && data.secret_message.cipher_text().channel.sender()
                                == &self.user_creds.device_creds.device.keys.transport_pk
                            && data.secret_message.cipher_text().channel.receiver()
                                == &member.user_data.device.keys.transport_pk,
                        "LegacyCleanupCurrentPacketConflict"
                    );
                }
            }
            pending.push(self.capture_legacy_target(vault, &update, transfer).await?);
        }
        for cleanup in pending {
            self.finish_legacy_target_cleanup(cleanup).await?;
        }
        Ok(true)
    }

    async fn legacy_cleanup_packet(
        &self,
        pass: &MetaPasswordId,
        receiver: &DeviceId,
    ) -> Result<Option<SsWorkflowObject>> {
        let descriptor = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass.clone(),
            receiver: receiver.clone(),
        });
        let packet = self
            .p_obj
            .repo
            .find_one(ArtifactId::from(descriptor))
            .await?
            .map(|e| e.shared_secret())
            .transpose()?;
        if let Some(packet) = &packet {
            join_source::packet(packet, &self.user_creds.vault_name, pass, receiver)?;
        }
        Ok(packet)
    }
}
