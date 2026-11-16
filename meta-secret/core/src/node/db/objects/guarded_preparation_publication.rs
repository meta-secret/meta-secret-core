//! Publish only frozen ordinary outputs after the caller proves own canonical Applied.
use super::{
    guarded_claim_publication::same_stable_claim,
    local_guarded_preparation::{PersistentGuardedPreparation, UNAVAILABLE, local_body_commitment},
    persistent_shared_secret::PersistentSharedSecret,
};
use crate::node::common::model::secret::SsDistributionId;
use crate::node::db::{
    descriptors::{
        object_descriptor::ToObjectDescriptor,
        shared_secret_descriptor::{SsDeviceLogDescriptor, SsWorkflowDescriptor},
        vault_descriptor::VaultDescriptor,
    },
    events::{
        generic_log_event::{KeyExtractor, ObjIdExtractor, ToGenericEvent},
        kv_log_event::KvKey,
        local_guarded_preparation::*,
        object_id::ArtifactId,
        shared_secret_event::{SsDeviceLogObject, SsWorkflowObject},
        vault::vault_event::VaultObject,
    },
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo> PersistentGuardedPreparation<R> {
    pub(crate) async fn publish_accepted(
        &self,
        mut object: LocalGuardedPreparationObject,
    ) -> Result<LocalGuardedPreparationObject> {
        if object.0.value.phase == GuardedPreparationPhase::AcceptedPublished {
            return Ok(object);
        }
        let payload = object
            .0
            .value
            .payload
            .clone()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        if object.0.value.phase != GuardedPreparationPhase::AcceptedPromoting {
            ensure!(
                object.0.value.phase == GuardedPreparationPhase::Signed,
                UNAVAILABLE
            );
            let mut value = object.0.value.clone();
            value.phase = GuardedPreparationPhase::AcceptedPromoting;
            object = self.replace(&object, value).await?;
        }
        for pass in &payload.passes {
            for packet in &pass.distributions {
                let index = object
                    .0
                    .value
                    .packets
                    .iter()
                    .position(|p| p.event_id == packet.obj_id())
                    .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
                let compact = &object.0.value.packets[index];
                if compact.handed_off && compact.receiver_id != object.0.value.owner.device_id {
                    self.remove_peer_packet(&object, index).await?;
                    continue;
                }
                self.install_packet(packet, pass).await?;
                if compact.receiver_id == object.0.value.owner.device_id && !compact.handed_off {
                    let mut value = object.0.value.clone();
                    value.packets[index].handed_off = true;
                    object = self.replace(&object, value).await?;
                }
            }
            let index = object
                .0
                .value
                .claims
                .iter()
                .position(|c| c.pass_id == pass.pass_id)
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            if object.0.value.claims[index].handed_off {
                self.verify_claim(&object.0.value.claims[index]).await?;
                continue;
            }
            let (device_id, log_id) = PersistentSharedSecret::from(self.p_obj.clone())
                .publish_guarded_claim(&pass.claim)
                .await?;
            let mut value = object.0.value.clone();
            let claim = &mut value.claims[index];
            ensure!(
                claim
                    .ss_device_event_id
                    .as_ref()
                    .is_none_or(|id| id == &device_id)
                    && claim
                        .ss_log_event_id
                        .as_ref()
                        .is_none_or(|id| id == &log_id),
                UNAVAILABLE
            );
            claim.ss_device_event_id = Some(device_id);
            claim.ss_log_event_id = Some(log_id);
            claim.handed_off = true;
            object = self.replace(&object, value).await?;
        }
        Ok(object)
    }

    async fn install_packet(
        &self,
        intended: &SsWorkflowObject,
        pass: &GuardedPreparedPass,
    ) -> Result<()> {
        let id = intended.obj_id();
        let current = self.p_obj.repo.find_one(id.clone()).await?;
        if current.as_ref() == Some(&intended.clone().to_generic()) {
            return Ok(());
        }
        if let Some(current) = &current {
            let workflow = current.clone().shared_secret()?;
            let SsWorkflowObject::Distribution(event) = &workflow else {
                anyhow::bail!(UNAVAILABLE)
            };
            let data = intended.clone().to_distribution_data()?;
            ensure!(
                workflow.key() == intended.key()
                    && event.value.vault_name == data.vault_name
                    && event.value.claim_id == pass.input.claim_id
                    && event.value.secret_message.cipher_text().channel.receiver()
                        == data.secret_message.cipher_text().channel.receiver(),
                UNAVAILABLE
            );
            if id == pass.input.distribution_id {
                ensure!(
                    local_body_commitment(&workflow)? == pass.input.distribution_commitment,
                    UNAVAILABLE
                );
            }
        }
        let outcome = if current.is_some() {
            self.p_obj.repo.replace(intended.clone()).await
        } else {
            self.p_obj.repo.save(intended.clone()).await
        };
        ensure!(
            self.p_obj.repo.find_one(id.clone()).await? == Some(intended.clone().to_generic()),
            UNAVAILABLE
        );
        if let Ok(saved) = outcome {
            ensure!(saved == id, UNAVAILABLE);
        }
        Ok(())
    }

    pub(crate) async fn acknowledge_packet(
        &self,
        object: &LocalGuardedPreparationObject,
        packet: &SsWorkflowObject,
    ) -> Result<()> {
        let index = object
            .0
            .value
            .packets
            .iter()
            .position(|p| p.event_id == packet.obj_id())
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(
            object.0.value.packets[index].body_commitment == local_body_commitment(packet)?,
            UNAVAILABLE
        );
        let mut value = object.0.value.clone();
        value.packets[index].handed_off = true;
        let updated = self.replace(object, value).await?;
        if updated.0.value.packets[index].receiver_id != updated.0.value.owner.device_id {
            self.remove_peer_packet(&updated, index).await?;
        }
        Ok(())
    }

    async fn remove_peer_packet(
        &self,
        object: &LocalGuardedPreparationObject,
        index: usize,
    ) -> Result<()> {
        let packet = &object.0.value.packets[index];
        if let Some(raw) = self.p_obj.repo.find_one(packet.event_id.clone()).await? {
            ensure!(
                local_body_commitment(&raw.shared_secret()?)? == packet.body_commitment,
                UNAVAILABLE
            );
            self.p_obj.repo.try_delete(packet.event_id.clone()).await?;
        }
        ensure!(
            self.p_obj
                .repo
                .find_one(packet.event_id.clone())
                .await?
                .is_none(),
            UNAVAILABLE
        );
        Ok(())
    }

    async fn verify_owned_metadata(&self, ownership: &GuardedClaimOwnership) -> Result<()> {
        let id = ownership
            .ss_device_event_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let raw = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let event = SsDeviceLogObject::try_from(raw)?;
        let descriptor = SsDeviceLogDescriptor::from(ownership.claim.sender.clone());
        ensure!(
            id.fqdn == ArtifactId::from(descriptor.clone()).fqdn
                && event.key() == KvKey::artifact(descriptor.to_obj_desc(), id)
                && event.0.value == ownership.claim,
            UNAVAILABLE
        );
        Ok(())
    }

    async fn verify_claim(&self, ownership: &GuardedClaimOwnership) -> Result<()> {
        self.verify_owned_metadata(ownership).await?;
        let id = ownership
            .ss_log_event_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let event = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
            .ss_log()?;
        ensure!(
            event.obj_id() == id
                && event
                    .0
                    .value
                    .claims
                    .get(&ownership.claim.id)
                    .is_some_and(|claim| same_stable_claim(claim, &ownership.claim)),
            UNAVAILABLE
        );
        Ok(())
    }

    pub(crate) async fn finish_accepted(
        &self,
        object: &LocalGuardedPreparationObject,
        vault: &VaultObject,
    ) -> Result<()> {
        ensure!(
            matches!(
                object.0.value.phase,
                GuardedPreparationPhase::AcceptedPromoting
                    | GuardedPreparationPhase::AcceptedPublished
            ),
            UNAVAILABLE
        );
        ensure!(
            vault.key().obj_desc
                == VaultDescriptor::from(object.0.value.owner.vault_name.clone()).to_obj_desc()
                && vault.0.value.vault_name == object.0.value.owner.vault_name,
            UNAVAILABLE
        );
        let deleted =
            |pass: &crate::node::common::model::meta_pass::MetaPasswordId| -> Result<bool> {
                let tombstone = vault.0.value.deleted_secrets.contains(pass);
                ensure!(
                    if tombstone {
                        !vault.0.value.secrets.contains(pass)
                            && object.0.value.phase == GuardedPreparationPhase::AcceptedPublished
                            && object.0.value.payload.is_none()
                    } else {
                        vault.0.value.secrets.contains(pass)
                    },
                    UNAVAILABLE
                );
                Ok(tombstone)
            };
        for claim in &object.0.value.claims {
            ensure!(claim.handed_off, UNAVAILABLE);
            if deleted(&claim.pass_id)? {
                self.verify_owned_metadata(claim).await?;
            } else {
                self.verify_claim(claim).await?;
            }
        }
        for packet in &object.0.value.packets {
            ensure!(packet.handed_off, UNAVAILABLE);
            let ownership: Vec<_> = object
                .0
                .value
                .claims
                .iter()
                .filter(|claim| {
                    packet.event_id
                        == ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                            pass_id: claim.pass_id.clone(),
                            receiver: packet.receiver_id.clone(),
                        }))
                })
                .collect();
            ensure!(ownership.len() == 1, UNAVAILABLE);
            let ownership = ownership[0];
            if deleted(&ownership.pass_id)? {
                ensure!(
                    self.p_obj
                        .repo
                        .find_one(packet.event_id.clone())
                        .await?
                        .is_none(),
                    UNAVAILABLE
                );
                continue;
            }
            if packet.receiver_id == object.0.value.owner.device_id {
                let actual = self
                    .p_obj
                    .repo
                    .find_one(packet.event_id.clone())
                    .await?
                    .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
                    .shared_secret()?;
                ensure!(
                    local_body_commitment(&actual)? == packet.body_commitment,
                    UNAVAILABLE
                );
            } else {
                self.remove_peer_packet(
                    object,
                    object
                        .0
                        .value
                        .packets
                        .iter()
                        .position(|p| p.event_id == packet.event_id)
                        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?,
                )
                .await?;
            }
        }
        if object.0.value.phase == GuardedPreparationPhase::AcceptedPublished {
            return Ok(());
        }
        let mut value = object.0.value.clone();
        value.payload = None;
        value.phase = GuardedPreparationPhase::AcceptedPublished;
        self.replace(object, value).await?;
        Ok(())
    }
}
