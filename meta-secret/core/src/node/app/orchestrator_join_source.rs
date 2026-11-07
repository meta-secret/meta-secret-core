//! Exact current source binding; queued creation is deferred, not sender authority.
use super::MetaOrchestrator;
use crate::node::{
    common::model::{
        device::common::DeviceId,
        meta_pass::MetaPasswordId,
        secret::{
            SecretDistributionData, SecretDistributionType, SsClaim, SsClaimId, SsDistributionId,
            SsLogData,
        },
        vault::vault_data::VaultData,
    },
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor,
            shared_secret_descriptor::{SsDeviceLogDescriptor, SsWorkflowDescriptor},
        },
        events::{
            generic_log_event::{KeyExtractor, ObjIdExtractor},
            local_guarded_preparation::GuardedPreparationPhase,
            object_id::{ArtifactId, Next},
            shared_secret_event::SsWorkflowObject,
        },
        objects::{
            local_guarded_preparation::{PersistentGuardedPreparation, local_body_commitment},
            persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use crate::secret::local_share::{LocalShareProfile, decode_local_share};
use anyhow::{Result, ensure};
use std::collections::HashSet;
const UNAVAILABLE: &str = "JoinResharingUnavailable";
pub(super) struct OwnedJoinSource {
    pub source: SsWorkflowObject,
    pub claim: SsClaim,
    pub missing: Vec<DeviceId>,
}
impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(super) async fn audit_join_source(
        &self,
        vault: &VaultData,
        pass: &MetaPasswordId,
        log: &SsLogData,
    ) -> Result<Option<OwnedJoinSource>> {
        let local = self.user_creds.device_id();
        let source_descriptor = descriptor(pass, local);
        let Some(source) = self.p_obj.find_tail_event(source_descriptor).await? else {
            return Ok(None);
        };
        let data = packet(&source, &vault.vault_name, pass, local)?;
        let local_pk = &self.user_creds.device_creds.device.keys.transport_pk;
        let canonical = log.claims.get(&data.claim_id.id);
        let foreign_origin = data.secret_message.cipher_text().channel.sender() != local_pk;
        if foreign_origin && canonical.is_none_or(|claim| claim.sender != *local) {
            return Ok(None); // Incoming receiver-only material never confers ownership.
        }
        let queued;
        let (claim, deferred) = match canonical {
            Some(claim) => (claim, false),
            None => {
                queued = self.exact_queued_join_claim(&data.claim_id).await?;
                (&queued, true)
            }
        };
        ensure!(
            claim.id == data.claim_id.id
                && claim.dist_claim_id == data.claim_id
                && claim.vault_name == vault.vault_name
                && claim.dist_claim_id.pass_id == *pass
                && claim.distribution_type == SecretDistributionType::Split
                && claim.sender == *local
                && vault
                    .members()
                    .iter()
                    .any(|member| member.user_data == self.user_creds.user())
                && data.secret_message.cipher_text().channel.receiver() == local_pk,
            UNAVAILABLE
        );
        if self
            .defer_prospective_guarded_join(vault, &source, claim, !deferred)
            .await?
        {
            return Ok(None);
        }
        if foreign_origin {
            // Existing2→1 canonicalization adopts an exact retained k1 copy while
            // retaining its ciphertext origin. It never adopts k2/receiver-only input.
            ensure!(
                !deferred && claim.receivers.is_empty() && claim.status.statuses.is_empty(),
                UNAVAILABLE
            );
            decode_local_share(data, &self.user_creds, LocalShareProfile::WholeCopy)?;
        }
        let receivers: HashSet<_> = claim.receivers.iter().cloned().collect();
        ensure!(
            receivers.len() == claim.receivers.len() && !receivers.contains(local),
            UNAVAILABLE
        );
        let current = self.join_remotes(vault);
        // Inspect every PRESENT old/new-target packet before replacing any generation.
        for receiver in receivers.union(&current) {
            if let Some(peer) = self
                .p_obj
                .find_tail_event(descriptor(pass, receiver))
                .await?
            {
                let peer_data = packet(&peer, &vault.vault_name, pass, receiver)?;
                ensure!(
                    peer_data.claim_id == claim.dist_claim_id
                        && peer_data.secret_message.cipher_text().channel.sender() == local_pk,
                    UNAVAILABLE
                );
            }
            // An absent historical Pending packet can follow real ACK/deletion. No
            // completion, loss, handoff or material readiness is inferred from it.
        }
        if deferred {
            // Fresh Create persists the exact own claim in SsDeviceLog, not SsLog.
            // Existing normal SS sync must publish/import it; do not compute from it.
            return Ok(None);
        }
        let mut missing: Vec<_> = current.difference(&receivers).cloned().collect();
        missing.sort_by_key(ToString::to_string);
        Ok(Some(OwnedJoinSource {
            source,
            claim: claim.clone(),
            missing,
        }))
    }

    // Exclusion only: a prospective approval never authorizes private cleanup or compute.
    async fn defer_prospective_guarded_join(
        &self,
        prospective: &VaultData,
        source: &SsWorkflowObject,
        claim: &SsClaim,
        canonical_claim: bool,
    ) -> Result<bool> {
        let records = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        }
        .load_all(&self.user_creds.user_id())
        .await?;
        let commitment = local_body_commitment(source)?;
        let mut owned = None;
        for object in &records {
            let value = &object.0.value;
            let claims: Vec<_> = value
                .claims
                .iter()
                .filter(|owned| owned.claim.id == claim.id)
                .collect();
            if claims.is_empty() {
                continue;
            }
            ensure!(claims.len() == 1, UNAVAILABLE);
            let ownership = claims[0];
            ensure!(
                ownership.pass_id == claim.dist_claim_id.pass_id
                    && ownership.claim.dist_claim_id == claim.dist_claim_id
                    && ownership.claim.vault_name == claim.vault_name
                    && ownership.claim.sender == claim.sender
                    && ownership.claim.distribution_type == claim.distribution_type,
                UNAVAILABLE
            );
            let packets: Vec<_> = value
                .packets
                .iter()
                .filter(|p| p.event_id == source.obj_id())
                .collect();
            ensure!(
                packets.len() == 1
                    && packets[0].body_commitment == commitment
                    && packets[0].receiver_id == *self.user_creds.device_id(),
                UNAVAILABLE
            );
            match value.phase {
                GuardedPreparationPhase::AcceptedPromoting
                | GuardedPreparationPhase::AcceptedPublished => {
                    ensure!(
                        ownership.claim.receivers == claim.receivers
                            && owned.is_none()
                            && value.survivors.contains(&self.user_creds.user()),
                        UNAVAILABLE
                    );
                    owned = Some(value);
                }
                GuardedPreparationPhase::BlockedRetiredCommand => {
                    ensure!(value.payload.is_none(), UNAVAILABLE);
                    if ownership.claim.receivers != claim.receivers {
                        self.require_normalized_terminal_input(
                            prospective,
                            source,
                            claim,
                            canonical_claim,
                        )
                        .await?;
                    }
                }
                GuardedPreparationPhase::Staged
                | GuardedPreparationPhase::Queued
                | GuardedPreparationPhase::Signed
                | GuardedPreparationPhase::RetiredUnqueued => anyhow::bail!(UNAVAILABLE),
            }
        }
        let Some(owned) = owned else {
            return Ok(false);
        };
        let canonical = PersistentVault::from(self.p_obj.clone())
            .get_vault(self.user_creds.vault_name.clone())
            .await?
            .to_data();
        let current = canonical.members();
        let future = prospective.members();
        if canonical == *prospective {
            ensure!(
                owned.survivors.len() == 2
                    && owned
                        .survivors
                        .iter()
                        .all(|u| current.iter().any(|m| m.user_data == *u)),
                UNAVAILABLE
            );
            if current.len() == 3 {
                // Another sync may have imported Join3 but not yet finished its
                // private lifetime readback. Equality is never compute authority.
                return Ok(true);
            }
            ensure!(current.len() == 2, UNAVAILABLE);
            return Ok(false);
        }
        ensure!(
            canonical.vault_name == prospective.vault_name
                && current.len() == 2
                && owned.survivors.len() == 2
                && future.len() == 3
                && current
                    .iter()
                    .all(|m| owned.survivors.contains(&m.user_data))
                && current.iter().all(|m| future.contains(m)),
            UNAVAILABLE
        );
        Ok(true)
    }

    // Compatibility of one ended generation's metadata, never publication authority.
    async fn require_normalized_terminal_input(
        &self,
        prospective: &VaultData,
        source: &SsWorkflowObject,
        claim: &SsClaim,
        canonical_claim: bool,
    ) -> Result<()> {
        ensure!(
            canonical_claim
                && claim.sender == *self.user_creds.device_id()
                && claim.distribution_type == SecretDistributionType::Split
                && claim.receivers.is_empty()
                && claim.status.statuses.is_empty()
                && claim.client_status.is_none(),
            UNAVAILABLE
        );
        let actual = self.get_ss_log_data().await?;
        ensure!(actual.claims.get(&claim.id) == Some(claim), UNAVAILABLE);
        let canonical = PersistentVault::from(self.p_obj.clone())
            .get_vault(self.user_creds.vault_name.clone())
            .await?
            .to_data();
        ensure!(
            canonical.vault_name == prospective.vault_name
                && canonical.vault_name == claim.vault_name
                && canonical
                    .members()
                    .iter()
                    .any(|m| m.user_data == self.user_creds.user()),
            UNAVAILABLE
        );
        let data = packet(
            source,
            &claim.vault_name,
            &claim.dist_claim_id.pass_id,
            self.user_creds.device_id(),
        )?;
        let local_pk = &self.user_creds.device_creds.device.keys.transport_pk;
        ensure!(
            data.claim_id == claim.dist_claim_id
                && data.secret_message.cipher_text().channel.sender() == local_pk
                && data.secret_message.cipher_text().channel.receiver() == local_pk,
            UNAVAILABLE
        );
        decode_local_share(data, &self.user_creds, LocalShareProfile::WholeCopy)?;
        Ok(())
    }

    async fn exact_queued_join_claim(&self, source: &SsClaimId) -> Result<SsClaim> {
        let descriptor = SsDeviceLogDescriptor::from(self.user_creds.device_id().clone());
        let mut expected = ArtifactId::from(descriptor.clone());
        let mut matches = vec![];
        for object in self
            .p_obj
            .get_object_events_from_beginning(descriptor.clone())
            .await?
        {
            ensure!(
                object.obj_id() == expected
                    && object.key().obj_desc == descriptor.clone().to_obj_desc(),
                UNAVAILABLE
            );
            expected = expected.next();
            let claim = object.to_distribution_request();
            if claim.id == source.id {
                ensure!(
                    claim.dist_claim_id == *source
                        && claim.sender == *self.user_creds.device_id()
                        && claim.vault_name == self.user_creds.vault_name
                        && claim.distribution_type == SecretDistributionType::Split,
                    UNAVAILABLE
                );
                matches.push(claim);
            }
        }
        ensure!(matches.len() == 1, UNAVAILABLE);
        Ok(matches.remove(0))
    }
}

fn descriptor(pass: &MetaPasswordId, receiver: &DeviceId) -> SsWorkflowDescriptor {
    SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: receiver.clone(),
    })
}

pub(super) fn packet<'a>(
    event: &'a SsWorkflowObject,
    vault: &crate::node::common::model::vault::vault::VaultName,
    pass: &MetaPasswordId,
    receiver: &DeviceId,
) -> Result<&'a SecretDistributionData> {
    let expected = descriptor(pass, receiver);
    ensure!(
        event.obj_id() == ArtifactId::from(expected.clone())
            && event.key().obj_desc == expected.to_obj_desc(),
        UNAVAILABLE
    );
    let SsWorkflowObject::Distribution(value) = event else {
        anyhow::bail!(UNAVAILABLE)
    };
    ensure!(
        value.value.vault_name == *vault
            && value.value.claim_id.pass_id == *pass
            && value
                .value
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id()
                == *receiver,
        UNAVAILABLE
    );
    Ok(&value.value)
}
