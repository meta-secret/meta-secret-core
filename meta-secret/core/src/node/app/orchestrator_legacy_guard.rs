//! Negative Guarded ownership checks never supply positive Legacy authority.
use super::*;
use crate::node::db::{
    events::{
        local_removal_envelope::RemovalEnvelopeMode,
        vault::{
            device_log_event::DeviceLogObject,
            vault_log_event::{RemoveMemberDecision, VaultActionEvent},
        },
    },
    objects::{
        device_transport::DeviceTransportSlot,
        local_guarded_preparation::PersistentGuardedPreparation,
        local_removal_envelope::PersistentRemovalEnvelope,
        persistent_device_log::PersistentDeviceLog,
    },
};
use anyhow::ensure;

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(super) async fn last_legacy_cleanup_update(&self) -> Result<Option<RemoveMemberEvent>> {
        let Some(body) = self.last_cleanup_body().await? else {
            return Ok(None);
        };
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = &body.0.value
        else {
            return Ok(None);
        };
        if update.decision != RemoveMemberDecision::Accepted {
            return Ok(None);
        }
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        }
        .load(&self.user_creds.user_id())
        .await?;
        let entry = registry.as_ref().and_then(|r| {
            r.0.value
                .entries
                .iter()
                .find(|e| e.event_id == body.obj_id())
        });
        let Some(entry) = entry else { return Ok(None) };
        if entry.mode != RemovalEnvelopeMode::Legacy {
            return Ok(None);
        }
        self.exclude_guarded_legacy_cleanup(update, None).await?;
        Ok(Some(update.clone()))
    }

    async fn last_cleanup_body(&self) -> Result<Option<DeviceLogObject>> {
        let log = PersistentDeviceLog::from(self.p_obj.clone());
        let owner = self.user_creds.user_id();
        let Some(id) = log.find_tail_id(&owner).await? else {
            return Ok(None);
        };
        Ok(match log.transport_slot(&owner, id).await? {
            Some(DeviceTransportSlot::Business(body)) => Some(*body),
            _ => None, // No reverse search past a technical or unrelated tail.
        })
    }

    pub(super) async fn exclude_guarded_legacy_cleanup(
        &self,
        update: &RemoveMemberEvent,
        claim: Option<&SsClaim>,
    ) -> Result<()> {
        let owner = self.user_creds.user_id();
        let body = self.last_cleanup_body().await?.filter(|body| {
            matches!(&body.0.value, VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) if u == update)
        });
        let records = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        }
        .load_all(&owner)
        .await?;
        for object in records {
            let v = &object.0.value;
            ensure!(
                !v.payload.as_ref().is_some_and(|p| p.update == *update),
                "LegacyCleanupGuardedOwnership"
            );
            for (origin, claims) in std::iter::once((v.origin.as_ref(), &v.claims)).chain(
                v.previous_attempts
                    .iter()
                    .map(|a| (Some(&a.origin), &a.claims)),
            ) {
                ensure!(
                    !body
                        .as_ref()
                        .is_some_and(|b| origin.is_some_and(|o| o.event_id == b.obj_id())),
                    "LegacyCleanupGuardedOwnership"
                );
                ensure!(
                    !claim.is_some_and(|expected| claims
                        .iter()
                        .any(|owned| same_identity(&owned.claim, expected))),
                    "LegacyCleanupGuardedOwnership"
                );
            }
        }
        if let Some(body) = body {
            let registry = PersistentRemovalEnvelope {
                p_obj: self.p_obj.clone(),
            }
            .load(&owner)
            .await?;
            if let Some(registry) = registry {
                if let Some(entry) = registry
                    .0
                    .value
                    .entries
                    .iter()
                    .find(|e| e.event_id == body.obj_id())
                {
                    ensure!(
                        entry.mode == RemovalEnvelopeMode::Legacy,
                        "LegacyCleanupGuardedOwnership"
                    );
                } else {
                    ensure!(
                        (body.obj_id().id.curr as u64)
                            < registry.0.value.first_managed_removal_nonce,
                        "LegacyCleanupUnknownOwnership"
                    );
                }
            }
        }
        Ok(())
    }
}

fn same_identity(left: &SsClaim, right: &SsClaim) -> bool {
    left.id == right.id
        && left.dist_claim_id == right.dist_claim_id
        && left.vault_name == right.vault_name
        && left.sender == right.sender
        && left.distribution_type == right.distribution_type
        && left.receivers == right.receivers
    // Mutable status/clientStatus/handedOff changes cannot make Guarded become Legacy.
}
