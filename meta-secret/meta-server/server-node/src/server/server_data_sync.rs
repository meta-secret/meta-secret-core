use std::cmp::PartialEq;
use std::sync::Arc;

use crate::server::state_invalidation::{
    NoopStateInvalidationPublisher, StateInvalidation, StateInvalidationPublisher,
    StateInvalidationScope,
};
use anyhow::Result;
use anyhow::bail;
use meta_secret_core::node::api::{SignedAction, SignedActionPayload, SsRequest, VaultRequest};
use meta_secret_core::node::common::model::device::common::{DeviceData, DeviceId};
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::state_events::{StateEventsSubscription, unix_time_secs};
use meta_secret_core::node::common::model::secret::{
    SecretDistributionType, SsDistributionId, SsDistributionStatus,
};
use meta_secret_core::node::common::model::user::common::{UserData, UserMembership};
use meta_secret_core::node::common::model::vault::vault::{VaultName, VaultStatus};
use meta_secret_core::node::common::model::vault::vault_data::VaultData;
use meta_secret_core::node::db::actions::vault::vault_action::ServerVaultAction;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::{
    SsLogDescriptor, SsWorkflowDescriptor,
};
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::shared_secret_event::{SsLogObject, SsWorkflowObject};
use meta_secret_core::node::db::events::vault::vault_log_event::{
    VaultActionEvent, VaultActionInitEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
};
use meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use tracing::{debug, instrument, warn};

pub struct ServerSyncGateway<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub invalidation_publisher: Arc<dyn StateInvalidationPublisher>,
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
    pub fn new(
        p_obj: Arc<PersistentObject<Repo>>,
        invalidation_publisher: Arc<dyn StateInvalidationPublisher>,
    ) -> Self {
        Self {
            p_obj,
            invalidation_publisher,
        }
    }

    /// Verify that a state invalidation subscriber is a current member of the
    /// signed Vault. The HTTP handler must call this before creating an SSE
    /// stream; the stream itself never trusts a client-provided device ID.
    pub async fn verify_state_events_subscription(
        &self,
        subscription: &StateEventsSubscription,
    ) -> Result<()> {
        let p_vault = PersistentVault::from(self.p_obj.clone());
        let vault = p_vault.get_vault(subscription.vault_name.clone()).await?;
        verify_state_events_subscription_data(&vault.to_data(), subscription, unix_time_secs()?)
    }

    fn publish_invalidation(
        &self,
        vault_name: meta_secret_core::node::common::model::vault::vault::VaultName,
        scope: StateInvalidationScope,
    ) {
        self.invalidation_publisher
            .publish(StateInvalidation::new(vault_name, scope));
    }

    #[instrument(skip(self))]
    pub async fn vault_replication(&self, request: VaultRequest) -> Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];

        let p_vault = PersistentVault::from(self.p_obj.clone());

        let vault_status = p_vault
            .update_vault_membership_info_for_user(request.sender.clone())
            .await?;

        //sync vault status (available to any user - just by definition)
        {
            let vault_status_events = self
                .p_obj
                .find_object_events::<GenericKvLogEvent>(request.tail.vault_status.clone())
                .await?;

            commit_log.extend(vault_status_events);
        }

        // guarding vault from sending event to outsiders
        match vault_status {
            VaultStatus::NotExists(_) => {
                //ignore
            }
            VaultStatus::Outsider(_) => {
                //ignore
            }
            VaultStatus::Member(_) => {
                //sync VaultLog
                {
                    let vault_log_events = self
                        .p_obj
                        .find_object_events::<GenericKvLogEvent>(request.tail.vault_log.clone())
                        .await?;
                    commit_log.extend(vault_log_events);
                }

                //sync Vault
                {
                    let vault_events = self
                        .p_obj
                        .find_object_events::<GenericKvLogEvent>(request.tail.vault.clone())
                        .await?;
                    commit_log.extend(vault_events);
                }
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled
    /// and the actions will be executed accordingly
    pub async fn handle_write(&self, server_device: DeviceData, action: SignedAction) -> Result<()> {
        let generic_event = self.verify_event_action(&action).await?;
        self.server_write_processing(server_device, generic_event)
            .await
    }

    /// Verify the authenticated envelope before any event is persisted.
    pub async fn verify_event_action(&self, action: &SignedAction) -> Result<GenericKvLogEvent> {
        let event = match &action.payload {
            SignedActionPayload::Event(event) => event.clone(),
            SignedActionPayload::RecoveryCompletion(_) => {
                bail!("recovery completion cannot be sent as a write event")
            }
        };
        let object_id = event.obj_id();
        if action.stream != object_id.fqdn.clone().id_str() {
            bail!("signed action stream does not match event object")
        }
        if action.nonce != object_id.id.curr as u64 {
            bail!("signed action nonce does not match event sequence")
        }

        let vault_name = event_vault_name(&event)?;
        let p_vault = PersistentVault::from(self.p_obj.clone());
        let signer_user = match p_vault.get_vault(vault_name.clone()).await {
            Ok(vault) => vault
                .to_data()
                .find_user(&action.signer)
                .and_then(|membership| match membership {
                    UserMembership::Member(member) => Some(member.user_data.clone()),
                    UserMembership::Outsider(_) => None,
                })
                .or_else(|| event_actor_user(&event, &action.signer)),
            Err(_) => event_actor_user(&event, &action.signer),
        }
        .ok_or_else(|| anyhow::anyhow!("signer is not authorized for this Vault"))?;

        if signer_user.device.device_id != action.signer {
            bail!("signer device id does not match action actor")
        }
        if DeviceId::from(&signer_user.device.keys) != action.signer {
            bail!("signer device id does not match transport public key")
        }
        action.verify(&signer_user.device.keys.dsa_pk)?;
        validate_event_actor(&event, &action.signer)?;

        // The event's ArtifactId is the durable per-stream monotonic sequence.
        // Reject the same or an older sequence before the event can be applied.
        if let Some(tail) = self.p_obj.find_tail_id(object_id.clone().first()).await? {
            if object_id.id.curr <= tail.id.curr {
                bail!("replayed or out-of-order signed action")
            }
        }
        Ok(event)
    }

    pub async fn verify_recovery_completion(&self, action: &SignedAction) -> Result<()> {
        let completion = action.completion()?;
        if action.stream != completion.recovery_id.clone().id_str() || action.nonce != 1 {
            bail!("invalid recovery completion sequence")
        }
        let p_vault = PersistentVault::from(self.p_obj.clone());
        let vault = p_vault.get_vault(completion.vault_name.clone()).await?;
        let member = vault
            .to_data()
            .find_user(&action.signer)
            .and_then(|membership| match membership {
                UserMembership::Member(member) => Some(member.user_data.clone()),
                UserMembership::Outsider(_) => None,
            })
            .ok_or_else(|| anyhow::anyhow!("completion signer is not a Vault member"))?;
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let ss_log = p_ss
            .get_ss_log_obj(completion.vault_name.clone())
            .await?;
        let claim = ss_log
            .claims
            .get(&completion.recovery_id.claim_id.id)
            .ok_or_else(|| anyhow::anyhow!("completion references an unknown claim"))?;
        let receiver = &completion.recovery_id.distribution_id.receiver;
        if claim.sender != completion.recovery_id.sender || !claim.receivers.contains(receiver) {
            bail!("completion receiver is not a target of this claim")
        }
        match &completion.receiver_status {
            SsDistributionStatus::Sent => {
                let receiver_has_approved =
                    claim.status.get(receiver) == Some(&SsDistributionStatus::Sent);
                let receiver_is_signer = action.signer == *receiver;
                let sender_is_signer = action.signer == claim.sender;
                if !receiver_is_signer && !(sender_is_signer && receiver_has_approved) {
                    bail!("completion signer is not authorized for this claim")
                }
            }
            SsDistributionStatus::Declined => {
                if action.signer != *receiver {
                    bail!("only the declining receiver can complete the claim")
                }
            }
            SsDistributionStatus::Pending | SsDistributionStatus::Delivered => {
                bail!("invalid recovery completion status")
            }
        }
        action.verify(&member.device.keys.dsa_pk)
    }
}

fn verify_state_events_subscription_data(
    vault: &VaultData,
    subscription: &StateEventsSubscription,
    now: u64,
) -> Result<()> {
    debug!(
        vault_name = %subscription.vault_name,
        subscriber_device_id = %subscription.signer,
        member_count = vault.members().len(),
        member_device_ids = ?vault
            .members()
            .iter()
            .map(|member| member.user_data.device.device_id.to_string())
            .collect::<Vec<_>>(),
        "verifying state events subscription"
    );
    let member = vault
        .find_user(&subscription.signer)
        .and_then(|membership| match membership {
            UserMembership::Member(member) => Some(member.user_data.clone()),
            UserMembership::Outsider(_) => None,
        })
        .ok_or_else(|| {
            warn!(
                vault_name = %subscription.vault_name,
                subscriber_device_id = %subscription.signer,
                "state events subscriber is not a Vault member"
            );
            anyhow::anyhow!("state events subscriber is not a Vault member")
        })?;

    if member.device.device_id != subscription.signer {
        bail!("state events signer device id does not match membership");
    }
    if DeviceId::from(&member.device.keys) != subscription.signer {
        bail!("state events signer device id does not match transport public key");
    }

    let result = subscription.verify(&member.device.keys.dsa_pk, now);
    if result.is_ok() {
        debug!(
            vault_name = %subscription.vault_name,
            subscriber_device_id = %subscription.signer,
            "state events subscription authorized"
        );
    }
    result
}

#[cfg(test)]
mod state_events_tests {
    use super::verify_state_events_subscription_data;
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::state_events::StateEventsSubscription;

    #[test]
    fn accepts_signed_subscription_for_current_member() {
        let registry = FixtureRegistry::empty();
        let member = &registry.state.user_creds.client;
        let key_manager = member.device_creds.key_manager().unwrap();
        let subscription = StateEventsSubscription::sign_at(
            member.vault_name.clone(),
            member.device_id().clone(),
            &key_manager.dsa,
            1_000,
        )
        .unwrap();

        verify_state_events_subscription_data(
            &registry.state.vault_data.full_membership,
            &subscription,
            1_001,
        )
        .unwrap();
    }

    #[test]
    fn rejects_signed_subscription_for_non_member() {
        let registry = FixtureRegistry::empty();
        let outsider_key = meta_secret_core::crypto::key_pair::DsaKeyPair::generate();
        let outsider_device = meta_secret_core::node::common::model::device::common::DeviceId(
            meta_secret_core::crypto::utils::U64IdUrlEnc::from("outside-state-events".to_string()),
        );
        let subscription = StateEventsSubscription::sign_at(
            registry.state.user_creds.client.vault_name.clone(),
            outsider_device,
            &outsider_key,
            1_000,
        )
        .unwrap();

        assert!(verify_state_events_subscription_data(
            &registry.state.vault_data.full_membership,
            &subscription,
            1_001,
        )
        .is_err());
    }

    #[test]
    fn rejects_subscription_signed_by_a_different_key() {
        let registry = FixtureRegistry::empty();
        let member = &registry.state.user_creds.client;
        let wrong_key_manager = registry.state.user_creds.client_b.device_creds.key_manager().unwrap();
        let subscription = StateEventsSubscription::sign_at(
            member.vault_name.clone(),
            member.device_id().clone(),
            &wrong_key_manager.dsa,
            1_000,
        )
        .unwrap();

        assert!(verify_state_events_subscription_data(
            &registry.state.vault_data.full_membership,
            &subscription,
            1_001,
        ).is_err());
    }
}

fn event_vault_name(event: &GenericKvLogEvent) -> Result<VaultName> {
    Ok(match event {
        GenericKvLogEvent::DeviceLog(object) => object.0.value.vault_name(),
        GenericKvLogEvent::SsDeviceLog(object) => object.0.value.vault_name.clone(),
        GenericKvLogEvent::SsWorkflow(object) => match object {
            SsWorkflowObject::Recovery(event) | SsWorkflowObject::Distribution(event) => {
                event.value.vault_name.clone()
            }
            SsWorkflowObject::Decline(event) => event.value.vault_name.clone(),
        },
        _ => bail!("unsupported unsigned state-changing event"),
    })
}

fn event_actor_user(event: &GenericKvLogEvent, signer: &DeviceId) -> Option<UserData> {
    let GenericKvLogEvent::DeviceLog(object) = event else {
        return None;
    };
    let user = match &object.0.value {
        VaultActionEvent::Init(VaultActionInitEvent::CreateVault(event)) => {
            event.owner.user_data.clone()
        }
        VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(event)) => {
            event.candidate.clone()
        }
        VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(event)) => {
            event.sender.user_data.clone()
        }
        VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(event)) => {
            event.sender.user_data.clone()
        }
        VaultActionEvent::Update(VaultActionUpdateEvent::AddMetaPass(event)) => {
            event.sender.user_data.clone()
        }
        VaultActionEvent::Update(VaultActionUpdateEvent::AddToPending { candidate }) => {
            candidate.clone()
        }
    };
    (user.device.device_id == *signer).then_some(user)
}

fn validate_event_actor(event: &GenericKvLogEvent, signer: &DeviceId) -> Result<()> {
    match event {
        GenericKvLogEvent::SsDeviceLog(object) => {
            if object.0.value.sender != *signer {
                bail!("claim sender does not match action signer")
            }
        }
        GenericKvLogEvent::SsWorkflow(object) => match object {
            SsWorkflowObject::Decline(event) => {
                if event.value.receiver_id != *signer {
                    bail!("decline receiver does not match action signer")
                }
            }
            SsWorkflowObject::Recovery(event) | SsWorkflowObject::Distribution(event) => {
                let channel_sender = event
                    .value
                    .secret_message
                    .cipher_text()
                    .channel
                    .sender()
                    .to_device_id();
                if channel_sender != *signer {
                    bail!("workflow channel sender does not match action signer")
                }
            }
        },
        GenericKvLogEvent::DeviceLog(_) => {}
        _ => bail!("unsupported state-changing event"),
    }
    Ok(())
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
    #[instrument(skip(self))]
    async fn server_write_processing(
        &self,
        server_device: DeviceData,
        generic_event: GenericKvLogEvent,
    ) -> Result<()> {
        match generic_event {
            GenericKvLogEvent::DeviceLog(device_log_obj) => {
                self.handle_device_log_request(server_device, device_log_obj)
                    .await?;
            }
            GenericKvLogEvent::SsDeviceLog(ss_device_log_obj) => {
                let claim_preview = ss_device_log_obj.clone().to_distribution_request();
                let vault_name = claim_preview.vault_name.clone();
                debug!(
                    claim_id = ?claim_preview.id,
                    claim_sender = ?claim_preview.sender,
                    pass_id_name = %claim_preview.dist_claim_id.pass_id.name,
                    receivers_count = claim_preview.receivers.len(),
                    dist_type = ?claim_preview.distribution_type,
                    "SsDeviceLog received"
                );
                if claim_preview.distribution_type == SecretDistributionType::Recover {
                    let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                    let server_log = p_ss_log.get_ss_log_obj(vault_name.clone()).await?;
                    let server_log_with_status = server_log
                        .clone()
                        .with_client_status(&claim_preview.sender);
                    if let Some(active_claim_id) = server_log_with_status
                        .find_unique_active_recovery_claim_id(
                        &claim_preview.sender,
                        &claim_preview.dist_claim_id.pass_id,
                    )? {
                        if active_claim_id != claim_preview.id {
                            debug!(
                                ?active_claim_id,
                                rejected_claim_id = ?claim_preview.id,
                                "recovery claim already active; keeping authoritative claim"
                            );
                            return Ok(());
                        }
                    }

                    // Device logs contain a complete local snapshot. Merge it with the
                    // server's current claim instead of inserting it wholesale: a stale
                    // snapshot from another receiver must not resurrect Pending after a
                    // terminal recovery decision has already been accepted.
                    self.p_obj.repo.save(ss_device_log_obj.clone()).await?;
                    let merged_log = server_log.merge_claim_update(claim_preview);
                    let new_ss_log_event = p_ss_log
                        .create_new_ss_log_object(merged_log, vault_name.clone())
                        .await?;
                    self.p_obj.repo.save(new_ss_log_event).await?;
                    self.publish_invalidation(vault_name, StateInvalidationScope::SsClaims);
                    return Ok(());
                }
                self.p_obj.repo.save(ss_device_log_obj.clone()).await?;

                let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                p_ss_log.save_ss_log_event(claim_preview).await?;
                self.publish_invalidation(vault_name, StateInvalidationScope::SsClaims);
            }
            GenericKvLogEvent::SsWorkflow(ss_object) => {
                if let SsWorkflowObject::Decline(decline_event) = &ss_object {
                    self.p_obj.repo.save(ss_object.clone()).await?;
                    let decline_data = decline_event.value.clone();
                    let vault_name = decline_data.vault_name.clone();
                    let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                    let maybe_ss_log_event = p_ss_log
                        .find_ss_log_tail_event(decline_data.vault_name.clone())
                        .await?;
                    let Some(ss_event) = maybe_ss_log_event else {
                        bail!("No claim found for decline: {:?}", decline_data)
                    };
                    let ss_log_data = ss_event.to_data();
                    let Some(current_claim) = ss_log_data.claims.get(&decline_data.claim_id) else {
                        bail!("No claim found for decline: {:?}", decline_data)
                    };
                    let has_prior_approval = current_claim.status.statuses.values().any(|status| {
                        matches!(
                            status,
                            SsDistributionStatus::Sent | SsDistributionStatus::Delivered
                        )
                    });
                    let new_ss_log_data = if has_prior_approval {
                        debug!(
                            claim_id = ?decline_data.claim_id,
                            receiver = ?decline_data.receiver_id,
                            "ignoring late recovery decline after approval"
                        );
                        ss_log_data
                    } else {
                        // The first decline is terminal for this claim: retire every
                        // still-pending receiver so a later approval cannot reveal the secret.
                        ss_log_data.decline_remaining_pending(decline_data.claim_id)
                    };
                    let new_ss_log_event = p_ss_log
                        .create_new_ss_log_object(new_ss_log_data, decline_data.vault_name)
                        .await?;
                    self.p_obj.repo.save(new_ss_log_event).await?;
                    self.publish_invalidation(vault_name, StateInvalidationScope::SsClaims);
                } else {
                    let ss_object_to_save = ss_object.clone();
                    let wf = ss_object.to_distribution_data()?;
                    let vault_name = wf.vault_name.clone();
                    let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                    let maybe_ss_log_event = p_ss_log
                        .find_ss_log_tail_event(wf.vault_name.clone())
                        .await?;
                    match maybe_ss_log_event {
                        None => {
                            bail!("No claim found for distribution: {:?}", wf)
                        }
                        Some(ss_event) => {
                            let ss_log_data = ss_event.to_data();
                            let maybe_claim = ss_log_data.claims.get(&wf.claim_id.id);

                            match maybe_claim {
                                None => {
                                    bail!("Invalid! No claim found for distribution: {:?}", wf)
                                }
                                Some(claim) => {
                                    let distribution_type = claim.distribution_type;
                                    let device_id = match distribution_type {
                                        SecretDistributionType::Split => wf
                                            .secret_message
                                            .cipher_text()
                                            .channel
                                            .receiver()
                                            .to_device_id(),
                                        SecretDistributionType::Recover => wf
                                            .secret_message
                                            .cipher_text()
                                            .channel
                                            .sender()
                                            .to_device_id(),
                                    };

                                    let claim_id = wf.claim_id.id.clone();
                                    if distribution_type == SecretDistributionType::Recover
                                        && claim.status.statuses.values().any(|status| {
                                            matches!(
                                                status,
                                                SsDistributionStatus::Declined
                                                    | SsDistributionStatus::Sent
                                                    | SsDistributionStatus::Delivered
                                            )
                                        })
                                    {
                                        debug!(
                                            claim_id = ?claim_id,
                                            receiver = ?device_id,
                                            "ignoring late recovery response after terminal decision"
                                        );
                                        return Ok(());
                                    }

                                    if distribution_type == SecretDistributionType::Split
                                        && matches!(
                                            claim.status.get(&device_id),
                                            Some(
                                                SsDistributionStatus::Sent
                                                    | SsDistributionStatus::Delivered
                                            )
                                        )
                                    {
                                        debug!(
                                            claim_id = ?claim_id,
                                            receiver = ?device_id,
                                            "duplicate split distribution workflow ignored"
                                        );
                                        return Ok(());
                                    }

                                    self.p_obj.repo.save(ss_object_to_save).await?;
                                    let new_ss_log_data =
                                        ss_log_data.sent(wf.claim_id.id, device_id);
                                    let new_ss_log_event = p_ss_log
                                        .create_new_ss_log_object(new_ss_log_data, wf.vault_name)
                                        .await?;
                                    self.p_obj.repo.save(new_ss_log_event).await?;
                                    self.publish_invalidation(
                                        vault_name,
                                        StateInvalidationScope::SsClaims,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            GenericKvLogEvent::DeviceCreds(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::UserCreds(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::VaultLog(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::Vault(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::VaultStatus(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::SsLog(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
            GenericKvLogEvent::DbError(_) => {
                bail!("Invalid event type: {:?}", generic_event);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_device_log_request(
        &self,
        server_device: DeviceData,
        device_log_obj: DeviceLogObject,
    ) -> Result<()> {
        self.p_obj.repo.save(device_log_obj.clone()).await?;

        let vault_action_event = device_log_obj.0;
        let vault_action = vault_action_event.value;
        let vault_name = vault_action.vault_name();
        let scope = state_scope_for_vault_action(&vault_action);
        let accepted_new_member = matches!(
            &vault_action,
            VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(update))
                if matches!(&update.update, UserMembership::Member(_))
        );

        let action = ServerVaultAction {
            p_obj: self.p_obj.clone(),
            server_device,
        };

        action.do_processing(vault_action).await?;
        if accepted_new_member && self.decline_pending_recovery_claims(vault_name.clone()).await? {
            self.publish_invalidation(vault_name.clone(), StateInvalidationScope::SsClaims);
        }
        self.publish_invalidation(vault_name, scope);
        Ok(())
    }

    /// A membership change invalidates recovery requests created against the
    /// previous member set. The server is the canonical log, so terminalize
    /// pending receivers here and let every client consume the resulting
    /// `Declined` claim through normal SsLog replication.
    async fn decline_pending_recovery_claims(&self, vault_name: VaultName) -> Result<bool> {
        let Some(ss_log_event) = self
            .p_obj
            .find_tail_event(SsLogDescriptor::from(vault_name.clone()))
            .await?
        else {
            return Ok(false);
        };

        let ss_log_data = ss_log_event.to_data();
        let declined_ss_log_data = ss_log_data.clone().decline_pending_recovery_claims();
        if declined_ss_log_data == ss_log_data {
            return Ok(false);
        }

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let new_ss_log_event = p_ss
            .create_new_ss_log_object(declined_ss_log_data, vault_name)
            .await?;
        self.p_obj.repo.save(new_ss_log_event).await?;
        Ok(true)
    }

    pub async fn ss_replication(
        &self,
        request: SsRequest,
        server_device: DeviceId,
    ) -> Result<Vec<GenericKvLogEvent>> {
        //sync SsLog
        let ss_log_events = self
            .p_obj
            .find_object_events::<SsLogObject>(request.ss_log.clone())
            .await?;
        debug!(
            ss_log_events_count = ss_log_events.len(),
            "ss_replication: events from find_object_events for request.ss_log"
        );
        let maybe_latest_ss_log_state = match ss_log_events.last() {
            Some(latest_ss_log_state) => Some(latest_ss_log_state.clone()),
            None => {
                self.p_obj
                    .find_tail_event(SsLogDescriptor::from(request.sender.vault_name.clone()))
                    .await?
            }
        };
        let Some(latest_ss_log_state) = maybe_latest_ss_log_state else {
            return Ok(vec![]);
        };

        let mut commit_log = vec![];
        for ss_log_event in ss_log_events.clone() {
            commit_log.push(ss_log_event.to_generic())
        }

        let ss_log_data = latest_ss_log_state.as_data();
        let mut updated_ss_log_data = ss_log_data.clone();
        let mut updated_state = false;

        debug!(
            claims_count = ss_log_data.claims.len(),
            request_sender = ?request.sender.device.device_id,
            "ss_replication: iterating claims"
        );

        for (_, claim) in ss_log_data.claims.iter() {
            if claim.sender.eq(&server_device) {
                bail!("Invalid state. Server can't manage Encrypted Key Shares");
            };

            let request_sender_device = request.sender.device.device_id.clone();

            debug!(
                claim_id = ?claim.id,
                dist_type = ?claim.distribution_type,
                pass_id_name = %claim.dist_claim_id.pass_id.name,
                "ss_replication: processing claim"
            );

            match claim.distribution_type {
                SecretDistributionType::Split => {
                    // Directly look up the distribution by (pass_id, requesting_device).
                    // Bypasses claim.receivers, which may be stale/empty when distributions
                    // arrive at the server before the ss_log is updated with the full receiver list.
                    let dist_id = SsDistributionId {
                        pass_id: claim.dist_claim_id.pass_id.clone(),
                        receiver: request_sender_device.clone(),
                    };
                    let desc = SsWorkflowDescriptor::Distribution(dist_id.clone());
                    let dist_obj = self.p_obj.find_tail_event(desc).await?;
                    debug!(
                        found = dist_obj.is_some(),
                        dist_id = ?dist_id,
                        "ss_replication: Split — find_tail_event result"
                    );

                    if let Some(dist_event) = dist_obj {
                        let ss_dist_obj_id = dist_event.obj_id();
                        commit_log.push(dist_event.to_generic());
                        updated_ss_log_data = updated_ss_log_data
                            .complete(claim.id.clone(), request_sender_device.clone());
                        updated_state = true;
                        self.p_obj.repo.delete(ss_dist_obj_id).await;
                    }
                }
                SecretDistributionType::Recover => {
                    for dist_id in claim.recovery_db_ids() {
                        if !dist_id.sender.eq(&request_sender_device) {
                            continue;
                        }
                        let desc = SsWorkflowDescriptor::Recovery(dist_id.clone());
                        let dist_obj = self.p_obj.find_tail_event(desc).await?;
                        if let Some(dist_event) = dist_obj {
                            let ss_dist_obj_id = dist_event.obj_id();
                            commit_log.push(dist_event.to_generic());
                            //skip: we can't complete the claim, otherwise we won't know
                            //on the sender device that the claim exists.
                            //Completion event needs to be sent by the recovery claim creator
                            updated_state = true;
                            self.p_obj.repo.delete(ss_dist_obj_id).await;
                        }
                    }
                }
            }
        }

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        if updated_state {
            let new_ss_log_obj = p_ss
                .create_new_ss_log_object(updated_ss_log_data, request.sender.vault_name.clone())
                .await?;
            self.p_obj
                .repo
                .save(new_ss_log_obj.clone().to_generic())
                .await?;
            commit_log.push(new_ss_log_obj.to_generic());
            self.publish_invalidation(request.sender.vault_name, StateInvalidationScope::SsClaims);
        }

        debug!(
            commit_log_len = commit_log.len(),
            "ss_replication: returning commit_log to client"
        );
        Ok(commit_log)
    }
}

impl<Repo: KvLogEventRepo> From<Arc<PersistentObject<Repo>>> for ServerSyncGateway<Repo> {
    fn from(p_obj: Arc<PersistentObject<Repo>>) -> Self {
        Self::new(p_obj, Arc::new(NoopStateInvalidationPublisher))
    }
}

fn state_scope_for_vault_action(action: &VaultActionEvent) -> StateInvalidationScope {
    match action {
        VaultActionEvent::Init(_) => StateInvalidationScope::All,
        VaultActionEvent::Request(request) => match request {
            VaultActionRequestEvent::JoinCluster(_) => StateInvalidationScope::Devices,
            VaultActionRequestEvent::AddMetaPass(_) => StateInvalidationScope::Vault,
        },
        VaultActionEvent::Update(update) => match update {
            VaultActionUpdateEvent::AddToPending { .. }
            | VaultActionUpdateEvent::UpdateMembership(_) => StateInvalidationScope::Devices,
            VaultActionUpdateEvent::AddMetaPass(_) => StateInvalidationScope::Vault,
        },
    }
}
