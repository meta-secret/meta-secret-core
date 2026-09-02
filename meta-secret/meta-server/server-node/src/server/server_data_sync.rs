use std::cmp::PartialEq;
use std::sync::Arc;

use crate::server::state_invalidation::{
    NoopStateInvalidationPublisher, StateInvalidation, StateInvalidationPublisher,
    StateInvalidationScope,
};
use anyhow::Result;
use anyhow::{Ok, bail};
use meta_secret_core::node::api::{SsRequest, VaultRequest};
use meta_secret_core::node::common::model::device::common::{DeviceData, DeviceId};
use meta_secret_core::node::common::model::secret::{
    SecretDistributionType, SsDistributionId, SsDistributionStatus,
};
use meta_secret_core::node::common::model::vault::vault::VaultStatus;
use meta_secret_core::node::db::actions::vault::vault_action::ServerVaultAction;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::{
    SsLogDescriptor, SsWorkflowDescriptor,
};
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::shared_secret_event::{SsLogObject, SsWorkflowObject};
use meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject;
use meta_secret_core::node::db::events::vault::vault_log_event::{
    VaultActionEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use tracing::{debug, instrument};

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
    pub async fn handle_write(
        &self,
        server_device: DeviceData,
        generic_event: GenericKvLogEvent,
    ) -> Result<()> {
        self.server_write_processing(server_device, generic_event)
            .await
    }
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
                    let server_log = p_ss_log
                        .get_ss_log_obj(vault_name.clone())
                        .await?
                        .with_client_status(&claim_preview.sender);
                    if let Some(active_claim_id) = server_log.find_unique_active_recovery_claim_id(
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
                    let new_ss_log_data =
                        ss_log_data.decline(decline_data.claim_id, decline_data.receiver_id);
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

        let action = ServerVaultAction {
            p_obj: self.p_obj.clone(),
            server_device,
        };

        action.do_processing(vault_action).await?;
        self.publish_invalidation(vault_name, scope);
        Ok(())
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
                bail!("Invalid state. Server can't manage encrypted shares");
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
