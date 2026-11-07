//! One explicit checked append; no background signing, promotion or Noop dispatch.
use super::{
    local_guarded_preparation::{PersistentGuardedPreparation, UNAVAILABLE},
    local_removal_envelope::PersistentRemovalEnvelope,
    persistent_device_log::PersistentDeviceLog,
};
use crate::node::{
    api::RemovalDecisionReceipt,
    common::model::{
        liveboard::LiveboardOperationPhase, liveboard_producer::producer_source_commitment,
        user::common::UserData,
    },
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
        },
        events::{
            generic_log_event::{ObjIdExtractor, ToGenericEvent},
            kv_log_event::{KvKey, KvLogEvent},
            local_guarded_preparation::*,
            local_removal_envelope::RemovalEnvelopeMode,
            vault::{device_log_event::DeviceLogObject, vault_log_event::*},
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo> PersistentDeviceLog<R> {
    pub(crate) async fn append_prepared_guarded(
        &self,
        sender: &UserData,
        mut witness: LocalGuardedPreparationObject,
    ) -> Result<RemovalDecisionReceipt> {
        let payload = witness
            .0
            .value
            .payload
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let action =
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(payload.update.clone()));
        let _append = self.p_obj.device_log_append_guard.lock().await;
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let event = if let Some(origin) = &witness.0.value.origin {
            let registry = PersistentRemovalEnvelope {
                p_obj: self.p_obj.clone(),
            }
            .load(&sender.user_id())
            .await?;
            ensure!(
                registry
                    .as_ref()
                    .is_some_and(|r| r.0.value.entries.iter().any(|entry| entry.event_id
                        == origin.event_id
                        && entry.mode == RemovalEnvelopeMode::Guarded
                        && entry.request_id.as_deref()
                            == Some(witness.0.value.request_id.as_str()))),
                UNAVAILABLE
            );
            let event = DeviceLogObject(KvLogEvent {
                key: KvKey::artifact(
                    DeviceLogDescriptor::from(sender.user_id()).to_obj_desc(),
                    origin.event_id.clone(),
                ),
                value: action.clone(),
            });
            ensure!(
                producer_source_commitment(&event)? == origin.body_commitment,
                UNAVAILABLE
            );
            let existing = self.p_obj.repo.find_one(origin.event_id.clone()).await?;
            if let Some(existing) = existing {
                ensure!(existing == event.clone().to_generic(), UNAVAILABLE);
            } else {
                ensure!(
                    self.free_transport_id(&sender.user_id()).await? == origin.event_id,
                    UNAVAILABLE
                );
                let reserved = self
                    .reserve_owned_device_event(
                        sender,
                        action,
                        Some(witness.0.value.request_id.clone()),
                    )
                    .await?;
                ensure!(reserved == event, UNAVAILABLE);
            }
            event
        } else {
            let event = self
                .reserve_owned_device_event(
                    sender,
                    action,
                    Some(witness.0.value.request_id.clone()),
                )
                .await?;
            let mut value = witness.0.value.clone();
            value.origin = Some(GuardedCommandOrigin {
                event_id: event.obj_id(),
                actor_id: sender.device.device_id.clone(),
                vault_name: sender.vault_name(),
                request_id: value.request_id.clone(),
                target_id: value.target_id.clone(),
                accepted: matches!(&event.0.value, VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) if u.decision == RemoveMemberDecision::Accepted),
                body_commitment: producer_source_commitment(&event)?,
                business_signature: None,
            });
            witness = store.replace(&witness, value).await?;
            event
        };
        let id = event.obj_id();
        if self
            .save_reserved_device_event(event.clone())
            .await
            .is_err()
        {
            match self.p_obj.repo.find_one(id.clone()).await {
                Ok(None) => anyhow::bail!(UNAVAILABLE),
                Ok(Some(actual)) if actual != event.to_generic() => anyhow::bail!(UNAVAILABLE),
                _ => return Ok(receipt(&witness, id, true)),
            }
        }
        if witness.0.value.phase != GuardedPreparationPhase::Queued {
            ensure!(
                witness.0.value.phase == GuardedPreparationPhase::Staged,
                UNAVAILABLE
            );
            let mut value = witness.0.value.clone();
            value.phase = GuardedPreparationPhase::Queued;
            if store.replace(&witness, value).await.is_err() {
                return Ok(receipt(&witness, id, true));
            }
        }
        Ok(receipt(&witness, id, false))
    }
}

fn receipt(
    witness: &LocalGuardedPreparationObject,
    id: crate::node::db::events::object_id::ArtifactId,
    unknown: bool,
) -> RemovalDecisionReceipt {
    RemovalDecisionReceipt {
        request_id: witness.0.value.request_id.clone(),
        event_id: id,
        phase: if unknown {
            LiveboardOperationPhase::Unknown
        } else {
            LiveboardOperationPhase::Queued
        },
        error_code: unknown.then(|| UNAVAILABLE.into()),
    }
}
