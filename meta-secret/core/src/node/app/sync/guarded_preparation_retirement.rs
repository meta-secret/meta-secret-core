//! Owned stale Pending retirement; ciphertext cleanup is independent of auth success.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    common::model::{
        liveboard_producer::producer_source_commitment,
        user::common::{UserData, UserDataOutsiderStatus, UserMembership},
        vault::vault_data::VaultData,
    },
    db::{
        descriptors::vault_descriptor::{DeviceLogDescriptor, VaultDescriptor},
        events::{
            local_guarded_preparation::*,
            object_id::ArtifactId,
            vault::liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
            vault::vault_event::VaultObject,
            vault::vault_log_event::{
                RemoveMemberDecision, VaultActionEvent, VaultActionEvents, VaultActionUpdateEvent,
            },
        },
        objects::{
            device_transport::DeviceTransportSlot,
            local_guarded_preparation::{
                CapturedRetirement, PersistentGuardedPreparation, UNAVAILABLE,
            },
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_device_log::PersistentDeviceLog,
            persistent_shared_secret::PersistentSharedSecret,
            persistent_vault::PersistentVault,
        },
        repo::{generic_db::KvLogEventRepo, persistent_credentials::PersistentCredentials},
    },
    device_transport_noop::{DeviceTransportNoop, is_transport_digest, validate_transport_id},
};
use anyhow::{Result, ensure};

fn pending_retirement(
    value: &LocalGuardedPreparation,
    log: &VaultActionEvents,
    vault: &VaultData,
    user: &UserData,
) -> bool {
    if value.transport_noop.is_some() {
        return true;
    }
    let Some(pending) = log
        .liveboard_removal_pending
        .iter()
        .find(|pending| pending.request_id == value.request_id)
    else {
        return false;
    };
    match &value.payload {
        Some(payload) => {
            (pending.request != payload.update.request
                && payload
                    .update
                    .request
                    .share_transfers
                    .iter()
                    .any(|transfer| vault.deleted_secrets.contains(&transfer.pass_id)))
                || (pending.request == payload.update.request
                    && other_removal_won(value, payload, log, vault, user))
        }
        None => value.phase == GuardedPreparationPhase::BlockedRetiredCommand,
    }
}

fn other_removal_won(
    value: &LocalGuardedPreparation,
    payload: &GuardedPreparationPayload,
    log: &VaultActionEvents,
    vault: &VaultData,
    user: &UserData,
) -> bool {
    let Some(origin) = &value.origin else {
        return false;
    };
    // Own Applied still owns frozen publication; Prepared is not a losing result.
    if log
        .liveboard_removal_results
        .iter()
        .any(|result| result.request_id == value.request_id)
        || value.owner != user.user_id()
        || vault.vault_name != user.vault_name()
        || origin.actor_id != user.device.device_id
        || origin.vault_name != user.vault_name()
        || origin.request_id != value.request_id
        || origin.target_id != value.target_id
        || !matches!(vault.find_user(&user.device.device_id), Some(UserMembership::Member(member)) if member.user_data == *user)
        || !matches!(vault.find_user(&value.target_id), Some(UserMembership::Outsider(target))
            if target.status == UserDataOutsiderStatus::Declined
                && target.user_data == payload.update.target.user_data)
    {
        return false;
    }
    let members = vault.members();
    if members.len() != 2
        || value.survivors.len() != 2
        || !members
            .iter()
            .all(|member| value.survivors.contains(&member.user_data))
    {
        return false;
    }
    let Some(other) = value.survivors.iter().find(|survivor| {
        survivor.device.device_id == value.requester_id
            && survivor.device.device_id != user.device.device_id
            && survivor.vault_name() == user.vault_name()
    }) else {
        return false;
    };
    let source_stream = ArtifactId::from(DeviceLogDescriptor::from(other.user_id())).fqdn;
    log.liveboard_removal_results.iter().any(|result| {
        result.request_id != value.request_id
            && result.target_id == value.target_id
            && result.requester_id == user.device.device_id
            && result.responder_id == other.device.device_id
            && result.outcome == DecisionOutcome::Accepted
            && result.application_state == RemovalApplicationState::Applied
            && result.source.as_ref().is_some_and(|source| {
                source.signer == result.responder_id
                    && source.event_id.fqdn == source_stream
                    && source.event_id != origin.event_id
                    && validate_transport_id(&source.event_id).is_ok()
                    && is_transport_digest(&source.signed_action_hash)
            })
    })
}

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(super) async fn retire_guarded_preparations(&self, user: &UserData) -> Result<()> {
        let persistent = PersistentVault::from(self.p_obj.clone());
        let local: Option<VaultObject> = self
            .p_obj
            .find_tail_event(VaultDescriptor::from(user.vault_name()))
            .await?;
        let Some(local) = local else {
            ensure!(
                PersistentGuardedPreparation {
                    p_obj: self.p_obj.clone()
                }
                .load_all(&user.user_id())
                .await?
                .is_empty(),
                UNAVAILABLE
            );
            return Ok(());
        };
        let vault = local.to_data();
        let log = persistent
            .get_vault_log_artifact(user.vault_name())
            .await?
            .0
            .value;
        log.validate_removal_bindings(true)?;
        let preparation = self.retire_pending_owned(user, &log, &vault).await;
        // Never retain ordinary transfers merely because auth/readback was uncertain.
        let ordinary = Box::pin(
            PersistentSharedSecret::from(self.p_obj.clone()).purge_deleted_secrets(&vault),
        )
        .await;
        preparation?;
        ordinary
    }

    async fn retire_pending_owned(
        &self,
        user: &UserData,
        log: &VaultActionEvents,
        vault: &VaultData,
    ) -> Result<()> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let Some(index) = store.index(&user.user_id()).await? else {
            return Ok(());
        };
        let mut failure = None;
        for object in store.load_all(&user.user_id()).await? {
            if !pending_retirement(&object.0.value, log, vault, user) {
                continue;
            }
            let captured = store.capture_retirement(&index, &object)?;
            let mut intended = object.0.value.clone();
            let authorization = match self
                .build_retirement_authorization(&object, user, vault)
                .await
            {
                Ok(Some(candidate)) => {
                    // Retain the exact signature before any uncertain write/readback.
                    intended.transport_noop = Some(candidate);
                    store.replace(&object, intended.clone()).await.map(|_| ())
                }
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            // This attempt is mandatory even after an authorization save/readback error.
            let scrub = self.scrub_guarded_retirement(&captured, intended).await;
            for result in [authorization, scrub] {
                if let Err(error) = result
                    && failure.is_none()
                {
                    failure = Some(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn build_retirement_authorization(
        &self,
        object: &LocalGuardedPreparationObject,
        user: &UserData,
        vault: &VaultData,
    ) -> Result<Option<LocalGuardedTransportNoop>> {
        let value = &object.0.value;
        let Some(origin) = &value.origin else {
            return Ok(None);
        };
        if let Some(technical) = &value.transport_noop {
            super::guarded_transport_noop::validate_technical(user, origin, technical)?;
            return Ok(None);
        }
        let Some(slot) = PersistentDeviceLog::from(self.p_obj.clone())
            .transport_slot(&user.user_id(), origin.event_id.clone())
            .await?
        else {
            return Ok(None); // An actual absent body cannot become queued from its origin.
        };
        let DeviceTransportSlot::Business(body) = slot else {
            anyhow::bail!(UNAVAILABLE)
        };
        let body = *body;
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = &body.0.value
        else {
            anyhow::bail!(UNAVAILABLE)
        };
        ensure!(
            value.owner == user.user_id()
                && origin.actor_id == user.device.device_id
                && origin.vault_name == user.vault_name()
                && origin.request_id == value.request_id
                && update.sender.user_data == *user
                && update.request.sender.user_data.device.device_id == value.requester_id
                && update.target.user_data.device.device_id == origin.target_id
                && update.target == update.request.target
                && (update.decision == RemoveMemberDecision::Accepted) == origin.accepted
                && matches!(vault.find_user(&user.device.device_id), Some(UserMembership::Member(member)) if member.user_data == *user),
            UNAVAILABLE
        );
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        }
        .load(&user.user_id())
        .await?;
        let entry = registry.as_ref().and_then(|registry| {
            registry
                .0
                .value
                .entries
                .iter()
                .find(|entry| entry.event_id == origin.event_id)
        });
        self.validate_guarded_envelope_binding(user, &origin.event_id, entry)
            .await?;
        let original = producer_source_commitment(&body)? == origin.body_commitment;
        if value.payload.is_some() {
            super::guarded_preparation_source::validate_complete_source(
                value,
                original.then_some(&body),
            )?;
        }
        if !original {
            let log = PersistentVault::from(self.p_obj.clone())
                .get_vault_log_artifact(user.vault_name())
                .await?
                .0
                .value;
            ensure!(
                log.liveboard_removal_pending
                    .iter()
                    .any(|pending| pending.request_id == value.request_id
                        && pending.request == update.request)
                    && update
                        .request
                        .share_transfers
                        .iter()
                        .chain(&update.share_transfers)
                        .all(|transfer| !vault.deleted_secrets.contains(&transfer.pass_id)),
                UNAVAILABLE
            );
        }
        let credentials = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        }
        .get_user_creds()
        .await?
        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(credentials.user() == *user, UNAVAILABLE);
        let authorization = DeviceTransportNoop::sign(
            &user.user_id(),
            origin.event_id.clone(),
            origin.body_commitment.clone(),
            &credentials.device_creds.key_manager()?.dsa,
        )?;
        Ok(Some(LocalGuardedTransportNoop {
            authorization,
            receipt: None,
            confirmed_next_tail: None,
        }))
    }

    async fn scrub_guarded_retirement(
        &self,
        captured: &CapturedRetirement,
        mut value: LocalGuardedPreparation,
    ) -> Result<()> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        if value.payload.is_none() {
            return Ok(());
        }
        let body_exists = match &value.origin {
            Some(origin) => self
                .p_obj
                .repo
                .find_one(origin.event_id.clone())
                .await?
                .is_some(),
            None => false,
        };
        value.payload = None;
        value.phase = if body_exists || value.transport_noop.is_some() {
            GuardedPreparationPhase::BlockedRetiredCommand
        } else {
            GuardedPreparationPhase::RetiredUnqueued
        };
        store.redact_retirement(captured, value).await
    }
}
