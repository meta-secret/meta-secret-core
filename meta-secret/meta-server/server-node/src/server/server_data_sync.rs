use std::cmp::PartialEq;
use std::sync::Arc;

use anyhow::Result;
use anyhow::{bail, Ok};
use derive_more::From;
use meta_secret_core::node::api::{SsRequest, VaultRequest};
use meta_secret_core::node::common::model::device::common::{DeviceData, DeviceId};
use meta_secret_core::node::common::model::secret::SecretDistributionType;
use meta_secret_core::node::common::model::vault::vault::VaultStatus;
use meta_secret_core::node::db::actions::vault::vault_action::ServerVaultAction;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::shared_secret_event::{SsLogObject, SsWorkflowObject};
use meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use tracing::{debug, info, instrument};

#[derive(From)]
pub struct ServerSyncGateway<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> ServerSyncGateway<Repo> {
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
                info!(
                    "Shared Secret Device Log message processing: {:?}",
                    &ss_device_log_obj
                );

                self.p_obj.repo.save(ss_device_log_obj.clone()).await?;

                let ss_claim = ss_device_log_obj.to_distribution_request();

                let p_ss_log = PersistentSharedSecret::from(self.p_obj.clone());
                p_ss_log.save_ss_log_event(ss_claim).await?;
            }
            GenericKvLogEvent::SsWorkflow(ss_object) => {
                self.p_obj.repo.save(ss_object.clone()).await?;

                if let SsWorkflowObject::Decline(decline_event) = &ss_object {
                    let decline_data = decline_event.value.clone();
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
                        .create_new_ss_log_object(
                            new_ss_log_data,
                            decline_data.vault_name,
                        )
                        .await?;
                    self.p_obj.repo.save(new_ss_log_event).await?;
                } else {
                    let wf = ss_object.to_distribution_data()?;
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
                                        let new_ss_log_data =
                                            ss_log_data.sent(wf.claim_id.id, device_id);
                                        // TODO: k-of-N — call only when count(Sent) >= threshold
                                        let new_ss_log_data = match distribution_type {
                                            SecretDistributionType::Recover => {
                                                new_ss_log_data.decline_remaining_pending(claim_id)
                                            }
                                            SecretDistributionType::Split => new_ss_log_data,
                                        };
                                        let new_ss_log_event = p_ss_log
                                            .create_new_ss_log_object(new_ss_log_data, wf.vault_name)
                                            .await?;
                                        self.p_obj.repo.save(new_ss_log_event).await?;
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

        let action = ServerVaultAction {
            p_obj: self.p_obj.clone(),
            server_device,
        };

        action.do_processing(vault_action).await?;
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
        let maybe_latest_ss_log_state = ss_log_events.last();
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

        for (_, claim) in ss_log_data.claims.iter() {
            // Distribute shares
            for dist_id in claim.recovery_db_ids() {
                if claim.sender.eq(&server_device) {
                    bail!("Invalid state. Server can't manage encrypted shares");
                };

                let request_sender_device = request.sender.device.device_id.clone();

                let for_delivery = match claim.distribution_type {
                    SecretDistributionType::Split => {
                        // If the device is a receiver of the share (vise versa to recovery)
                        dist_id.distribution_id.receiver.eq(&request_sender_device)
                    }
                    SecretDistributionType::Recover => {
                        // The device that sent recovery claim request is a receiver of the share
                        dist_id.sender.eq(&request_sender_device)
                    }
                };

                if !for_delivery {
                    continue;
                }

                let desc = match claim.distribution_type {
                    SecretDistributionType::Split => {
                        SsWorkflowDescriptor::Distribution(dist_id.distribution_id.clone())
                    }
                    SecretDistributionType::Recover => {
                        SsWorkflowDescriptor::Recovery(dist_id.clone())
                    }
                };

                let dist_obj = self.p_obj.find_tail_event(desc).await?;

                if let Some(dist_event) = dist_obj {
                    let ss_dist_obj_id = dist_event.obj_id();
                    commit_log.push(dist_event.to_generic());
                    self.p_obj.repo.delete(ss_dist_obj_id).await;

                    match claim.distribution_type {
                        SecretDistributionType::Split => {
                            let claim_id = dist_id.claim_id.id;
                            updated_ss_log_data = updated_ss_log_data
                                .complete(claim_id, dist_id.distribution_id.receiver);
                        }
                        SecretDistributionType::Recover => {
                            //skip: we can't complete the claim, otherwise we won't know
                            //on the sender device that the claim exists.
                            //Completion event needs to be sent by the recovery claim creator
                        }
                    }

                    updated_state = true;
                }
            }
        }

        if updated_state {
            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
            let new_ss_log_obj = p_ss
                .create_new_ss_log_object(updated_ss_log_data, request.sender.vault_name.clone())
                .await?;
            self.p_obj
                .repo
                .save(new_ss_log_obj.clone().to_generic())
                .await?;
            commit_log.push(new_ss_log_obj.to_generic());
        }

        debug!(
            commit_log_len = commit_log.len(),
            "ss_replication: returning commit_log to client"
        );
        Ok(commit_log)
    }
}
