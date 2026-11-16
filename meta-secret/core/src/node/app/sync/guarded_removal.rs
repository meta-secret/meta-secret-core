//! Endpoint-confirmed exact decisions and the shared durable envelope selector.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::api::{
    CanonicalDecisionInput, DataSyncResponse, LiveboardCapabilitiesRequest, ReadSyncRequest,
    RemovalDecisionReceipt, SignedAction, SyncRequest,
};
use crate::node::common::model::{
    user::common::{UserData, UserMembership},
    vault::vault::VaultMember,
};
use crate::node::db::{
    actions::sign_up::remove::{RemoveAction, RemoveActionUpdate},
    descriptors::vault_descriptor::DeviceLogDescriptor,
    events::{
        generic_log_event::{ObjIdExtractor, ToGenericEvent},
        local_removal_envelope::RemovalEnvelopeMode,
        object_id::ArtifactId,
        vault::{
            device_log_event::DeviceLogObject,
            vault_log_event::{VaultActionEvent, VaultActionUpdateEvent},
        },
    },
    objects::{
        local_removal_envelope::PersistentRemovalEnvelope,
        persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault,
    },
    repo::{generic_db::KvLogEventRepo, persistent_credentials::PersistentCredentials},
};
use anyhow::{Result, bail, ensure};

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> SyncGateway<Repo, Sync> {
    pub async fn confirm_guarded_removal_support(&self) -> Result<()> {
        let response = self
            .sync
            .send(SyncRequest::Read(Box::new(
                ReadSyncRequest::LiveboardCapabilities(LiveboardCapabilitiesRequest { version: 1 }),
            )))
            .await?;
        ensure!(
            matches!(response, DataSyncResponse::LiveboardCapabilities(cap) if cap.projection_version == 1 && cap.guarded_removal_version == 1 && cap.device_transport_noop_version == 1),
            "UnsupportedLiveboardContract"
        );
        Ok(())
    }
    pub async fn decide_remove_member(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<RemovalDecisionReceipt> {
        self.confirm_guarded_removal_support().await?;
        let _operation = self.sync_guard.lock().await;
        self.decide_guarded_locked(input).await.map_err(|_| {
            anyhow::anyhow!(crate::node::db::objects::local_guarded_preparation::UNAVAILABLE)
        })
    }

    async fn decide_guarded_locked(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<RemovalDecisionReceipt> {
        use crate::node::db::objects::local_guarded_preparation::{
            PersistentGuardedPreparation, UNAVAILABLE, local_body_commitment,
        };
        ensure!(
            crate::node::api::is_liveboard_request_id(&input.request_id),
            UNAVAILABLE
        );
        let credentials = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        }
        .get_user_creds()
        .await?
        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let user = credentials.user();
        let persistent = PersistentVault::from(self.p_obj.clone());
        let base = persistent.get_vault(user.vault_name()).await?;
        let member = match base.0.value.find_user(&user.device.device_id) {
            Some(UserMembership::Member(member)) if member.user_data == user => member.clone(),
            _ => bail!(UNAVAILABLE),
        };
        let log = persistent
            .get_vault_log_artifact(user.vault_name())
            .await?
            .0
            .value;
        log.validate_removal_bindings(true)?;
        let binding = log
            .liveboard_removal_pending
            .iter()
            .find(|b| b.request_id == input.request_id)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let desired = if input.accepted {
            RemoveActionUpdate::Accept
        } else {
            RemoveActionUpdate::Decline
        };
        let update = RemoveAction {
            p_obj: self.p_obj.clone(),
            member: VaultMember {
                member,
                vault: base.0.value.clone(),
            },
        }
        .build_update(binding.request.clone(), desired, vec![])?;
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let object = if let Some(object) = store.load(&user.user_id(), &input.request_id).await? {
            validate_retained_technical(&user, &object.0.value)?;
            if object.0.value.payload.is_some() {
                ensure!(
                    object.0.value.base_vault == base.obj_id()
                        && object
                            .0
                            .value
                            .payload
                            .as_ref()
                            .is_some_and(|p| p.update == update),
                    UNAVAILABLE
                );
                object
            } else {
                use crate::node::db::events::local_guarded_preparation::GuardedPreparationPhase;
                let value = &object.0.value;
                ensure!(
                    value.phase == GuardedPreparationPhase::BlockedRetiredCommand
                        && value.requester_id == update.request.sender.user_data.device.device_id
                        && value.target_id == update.target.user_data.device.device_id,
                    UNAVAILABLE
                );
                let origin = value
                    .origin
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
                validate_settled_technical(&user, origin, value.transport_noop.as_ref())?;
                let registry = PersistentRemovalEnvelope {
                    p_obj: self.p_obj.clone(),
                }
                .load(&user.user_id())
                .await?;
                let entry = registry.as_ref().and_then(|r| {
                    r.0.value
                        .entries
                        .iter()
                        .find(|e| e.event_id == origin.event_id)
                });
                self.validate_guarded_envelope_binding(&user, &origin.event_id, entry)
                    .await?;
                let index = store
                    .index(&user.user_id())
                    .await?
                    .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
                let complete = crate::node::app::orchestrator::MetaOrchestrator {
                    p_obj: self.p_obj.clone(),
                    user_creds: credentials,
                }
                .compute_guarded_preparation(&input.request_id, &update, &base)
                .await?;
                store.rollover(&index, &object, complete).await?
            }
        } else {
            let registry = PersistentRemovalEnvelope {
                p_obj: self.p_obj.clone(),
            }
            .load(&user.user_id())
            .await?;
            ensure!(
                !registry.is_some_and(|r| r
                    .0
                    .value
                    .entries
                    .iter()
                    .any(|e| e.mode == RemovalEnvelopeMode::Guarded
                        && e.request_id.as_deref() == Some(&input.request_id))),
                UNAVAILABLE
            );
            let value = crate::node::app::orchestrator::MetaOrchestrator {
                p_obj: self.p_obj.clone(),
                user_creds: credentials,
            }
            .compute_guarded_preparation(&input.request_id, &update, &base)
            .await?;
            store.insert(value).await?
        };
        // A reopened FULL retry has not passed the ordinary-sync technical verifier yet.
        validate_retained_technical(&user, &object.0.value)?;
        let payload = object
            .0
            .value
            .payload
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        for pass in &payload.passes {
            let raw = self
                .p_obj
                .repo
                .find_one(pass.input.distribution_id.clone())
                .await?
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            let workflow = raw.shared_secret()?;
            ensure!(
                workflow.obj_id() == pass.input.distribution_id
                    && workflow.clone().to_distribution_data()?.claim_id == pass.input.claim_id
                    && local_body_commitment(&workflow)? == pass.input.distribution_commitment,
                UNAVAILABLE
            );
        }
        PersistentDeviceLog::from(self.p_obj.clone())
            .append_prepared_guarded(&user, object)
            .await
    }
    pub async fn sign_queued_device_event(
        &self,
        event: DeviceLogObject,
        user: &UserData,
    ) -> Result<SignedAction> {
        let _owner = self.sync_guard.lock().await;
        self.sign_queued_device_event_owned(event, user).await
    }

    /// The production sync transaction already holds the shared owner.
    pub(super) async fn sign_queued_device_event_owned(
        &self,
        event: DeviceLogObject,
        user: &UserData,
    ) -> Result<SignedAction> {
        let id = event.obj_id();
        let expected = ArtifactId::from(DeviceLogDescriptor::from(user.user_id()));
        ensure!(
            id.fqdn == expected.fqdn,
            "queued event uses another device stream"
        );
        ensure!(
            self.p_obj.repo.find_one(id.clone()).await? == Some(event.clone().to_generic()),
            "queued event does not match durable body"
        );
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = &event.0.value
        else {
            return self.signed_event(event.to_generic(), user).await;
        };
        ensure!(
            update.sender.user_data == *user
                && update.request.sender.user_data.vault_name() == user.vault_name(),
            "queued removal actor or Vault mismatch"
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
                .find(|entry| entry.event_id == id)
        });
        self.validate_guarded_envelope_binding(user, &id, entry)
            .await?;
        if let Some(entry) = entry {
            match entry.mode {
                RemovalEnvelopeMode::Legacy => {
                    return self.signed_event(event.to_generic(), user).await;
                }
                RemovalEnvelopeMode::Guarded => {
                    self.confirm_guarded_removal_support().await?;
                    let request_id = entry
                        .request_id
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("missing guarded removal reference"))?;
                    return self
                        .freeze_guarded_source(event, user, request_id)
                        .await
                        .map_err(|_| {
                            anyhow::anyhow!(
                                crate::node::db::objects::local_guarded_preparation::UNAVAILABLE
                            )
                        });
                }
            }
        }
        ensure!(
            registry.is_none_or(
                |registry| (id.id.curr as u64) < registry.0.value.first_managed_removal_nonce
            ),
            "managed removal is missing durable envelope ownership"
        );
        self.signed_event(event.to_generic(), user).await
    }
}

fn validate_retained_technical(
    user: &UserData,
    value: &crate::node::db::events::local_guarded_preparation::LocalGuardedPreparation,
) -> Result<()> {
    if let Some(technical) = &value.transport_noop {
        let origin = value.origin.as_ref().ok_or_else(|| {
            anyhow::anyhow!(crate::node::db::objects::local_guarded_preparation::UNAVAILABLE)
        })?;
        super::guarded_transport_noop::validate_technical(user, origin, technical)?;
    }
    for attempt in &value.previous_attempts {
        validate_settled_technical(user, &attempt.origin, attempt.transport_noop.as_ref())?;
    }
    Ok(())
}

fn validate_settled_technical(
    user: &UserData,
    origin: &crate::node::db::events::local_guarded_preparation::GuardedCommandOrigin,
    technical: Option<
        &crate::node::db::events::local_guarded_preparation::LocalGuardedTransportNoop,
    >,
) -> Result<()> {
    let technical = technical.ok_or_else(|| {
        anyhow::anyhow!(crate::node::db::objects::local_guarded_preparation::UNAVAILABLE)
    })?;
    ensure!(
        technical.receipt.is_some() && technical.confirmed_next_tail.is_some(),
        crate::node::db::objects::local_guarded_preparation::UNAVAILABLE
    );
    super::guarded_transport_noop::validate_technical(user, origin, technical)
}
