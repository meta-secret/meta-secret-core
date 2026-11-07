//! Clean exact admitted sources even when legacy raw history has a prefix gap.
use super::{PersistentSharedSecret, scrub_removal_transfers};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::{
    descriptors::vault_descriptor::{DeviceLogDescriptor, VaultLogDescriptor},
    events::{
        generic_log_event::{ObjIdExtractor, ToGenericEvent},
        object_id::ArtifactId,
        vault::{
            device_log_event::DeviceLogObject,
            producer_metadata::{VerifiedProducerKind, VerifiedProducerSubject},
            vault_log_event::{
                VaultActionEvent, VaultActionEvents, VaultActionRequestEvent,
                VaultActionUpdateEvent,
            },
        },
    },
    objects::{persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault},
    repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub(super) async fn purge_removal_transfer_sources(&self, vault: &VaultData) -> Result<()> {
        for mut event in self.removal_transfer_sources(vault).await? {
            let before = event.clone();
            scrub_removal_transfers(&mut event.0.value, vault);
            if before != event {
                self.p_obj.repo.replace(event.clone()).await?;
                ensure!(
                    self.p_obj.repo.find_one(event.obj_id()).await? == Some(event.to_generic()),
                    "removal cleanup readback failed"
                );
            }
        }
        Ok(())
    }

    /// Direct public gateways cannot repair outside the ordered server owner.
    /// They must withhold retained packets until the existing cleanup is proven.
    pub async fn ensure_deleted_removal_transfers_clean(&self, vault: &VaultData) -> Result<()> {
        for event in self.removal_transfer_sources(vault).await? {
            let mut scrubbed = event.clone();
            scrub_removal_transfers(&mut scrubbed.0.value, vault);
            ensure!(event == scrubbed, "removal cleanup incomplete");
        }
        for log in self
            .p_obj
            .get_object_events_from_beginning(VaultLogDescriptor::from(vault.vault_name.clone()))
            .await?
        {
            let mut scrubbed = log.clone();
            scrub_vault_log(&mut scrubbed.0.value, vault)?;
            ensure!(log == scrubbed, "removal cleanup incomplete");
        }
        Ok(())
    }

    async fn removal_transfer_sources(&self, vault: &VaultData) -> Result<Vec<DeviceLogObject>> {
        let mut events = vec![];
        let transport = PersistentDeviceLog::from(self.p_obj.clone());
        for device in vault.users.values() {
            events.extend(
                transport
                    .transport_slots(
                        &device.user_data().user_id(),
                        ArtifactId::from(DeviceLogDescriptor::from(device.user_data().user_id())),
                    )
                    .await?
                    .into_iter()
                    .filter_map(|slot| slot.business()),
            );
        }
        let log = PersistentVault::from(self.p_obj.clone())
            .vault_log(vault.vault_name.clone())
            .await?;
        if let Some(log) = log {
            for source in &log.0.value.producer_source_refs {
                if !matches!(
                    source.kind,
                    VerifiedProducerKind::RemoveRequest | VerifiedProducerKind::LocalRemoveAccept
                ) {
                    continue;
                }
                log.0.value.producer_source(&source.source_event_id)?;
                if events
                    .iter()
                    .any(|event: &DeviceLogObject| event.obj_id() == source.source_event_id)
                {
                    continue;
                }
                ensure!(
                    source.vault_name == vault.vault_name,
                    "removal cleanup source mismatch"
                );
                let owner = crate::node::common::model::user::common::UserId {
                    vault_name: source.vault_name.clone(),
                    device_id: source.actor_id.clone(),
                };
                let Some(event) = transport
                    .transport_slot(&owner, source.source_event_id.clone())
                    .await?
                    .and_then(|slot| slot.business())
                else {
                    // Missing raw remains unacknowledged; a validated technical
                    // marker has no Business transfer payload to scrub.
                    continue;
                };
                let (kind, actor, target) = match &event.0.value {
                    VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => {
                        (VerifiedProducerKind::RemoveRequest, &r.sender, &r.target)
                    }
                    VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) => (
                        VerifiedProducerKind::LocalRemoveAccept,
                        &u.sender,
                        &u.target,
                    ),
                    _ => anyhow::bail!("removal cleanup source mismatch"),
                };
                let expected =
                    ArtifactId::from(DeviceLogDescriptor::from(actor.user_data.user_id()));
                ensure!(
                    source.kind == kind
                        && source.vault_name == vault.vault_name
                        && actor.user_data.vault_name() == vault.vault_name
                        && source.actor_id == actor.user_data.device.device_id
                        && source.source_event_id.fqdn == expected.fqdn
                        && source.subject
                            == VerifiedProducerSubject::Device(
                                target.user_data.device.device_id.clone()
                            ),
                    "removal cleanup source mismatch"
                );
                events.push(event);
            }
        }
        Ok(events)
    }
}

/// Pure version of the existing tombstone purge, shared by the writer and the
/// direct-read gate. Identity/proof metadata is untouched; nothing is persisted.
pub(super) fn scrub_vault_log(events: &mut VaultActionEvents, vault: &VaultData) -> Result<()> {
    let pending_count = events.requests.len();
    events.requests = std::mem::take(&mut events.requests)
        .into_iter()
        .map(|request| {
            let mut action = VaultActionEvent::Request(request);
            scrub_removal_transfers(&mut action, vault);
            let VaultActionEvent::Request(request) = action else {
                unreachable!()
            };
            request
        })
        .collect();
    ensure!(
        events.requests.len() == pending_count,
        "removal cleanup would collapse distinct pending requests"
    );
    for binding in &mut events.liveboard_removal_pending {
        let mut action = VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(
            binding.request.clone(),
        ));
        scrub_removal_transfers(&mut action, vault);
        if let VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)) = action {
            binding.request = request;
        }
    }
    for receipt in &mut events.liveboard_removal_applications {
        let mut action =
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(receipt.update.clone()));
        scrub_removal_transfers(&mut action, vault);
        if let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = action {
            receipt.update = update;
        }
    }
    events.updates = std::mem::take(&mut events.updates)
        .into_iter()
        .map(|update| {
            let mut action = VaultActionEvent::Update(update);
            scrub_removal_transfers(&mut action, vault);
            let VaultActionEvent::Update(update) = action else {
                unreachable!()
            };
            update
        })
        .collect();
    events.validate_removal_bindings(false)
}
