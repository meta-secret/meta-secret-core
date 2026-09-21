use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, instrument};

use crate::crypto::keys::TransportSk;
use crate::node::api::{
    DataEventsResponse, ReadSyncRequest, ServerTailRequest, ServerTailResponse, SsRequest,
    SyncRequest, VaultRequest, WriteSyncRequest,
};
use crate::node::app::orchestrator::MetaOrchestrator;
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SecretDistributionType, SsClaim, SsDistributionStatus};
use crate::node::common::model::user::common::{UserData, UserDataMember, UserId};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::{DeviceLogDescriptor, VaultDescriptor};
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::shared_secret_event::{
    SsDeviceLogObject, SsLogObject, SsWorkflowObject,
};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::JoinClusterEvent;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::security::sign_event;
use anyhow::Result;
use std::collections::HashSet;

pub struct SyncGateway<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub id: String,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub sync: Arc<Sync>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> SyncGateway<Repo, Sync> {
    async fn signed_event(&self, event: GenericKvLogEvent, user: &UserData) -> Result<crate::node::api::SignedAction> {
        let creds = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        }
        .get_user_creds()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Cannot sign sync event without user credentials"))?;
        if creds.device_id() != &user.device.device_id {
            return Err(anyhow::anyhow!("Sync signer does not match local device"));
        }
        let key_manager = creds.device_creds.key_manager()?;
        sign_event(event, user.device.device_id.clone(), &key_manager.dsa)
    }

    fn has_pending_split_receiver(claim: &SsClaim) -> bool {
        claim
            .status
            .statuses
            .values()
            .any(|status| matches!(status, SsDistributionStatus::Pending))
    }

    fn split_workflow_receiver(wf_event: &SsWorkflowObject) -> Option<DeviceId> {
        let SsWorkflowObject::Distribution(event) = wf_event else {
            return None;
        };

        Some(
            event
                .value
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id(),
        )
    }

    fn split_workflow_has_pending_receiver(claim: &SsClaim, wf_event: &SsWorkflowObject) -> bool {
        let Some(receiver) = Self::split_workflow_receiver(wf_event) else {
            return false;
        };

        matches!(
            claim.status.get(&receiver),
            Some(SsDistributionStatus::Pending)
        )
    }

    async fn cleanup_foreign_sender_distributions(
        &self,
        user: &UserData,
        ss_log: &SsLogObject,
    ) -> Result<()> {
        let local_device_id = &user.device.device_id;
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

        for claim in ss_log.as_data().claims.values() {
            if claim.distribution_type != SecretDistributionType::Split
                || claim.sender != *local_device_id
            {
                continue;
            }

            for wf_event in p_ss.get_distributions(claim.clone()).await? {
                let Some(receiver) = Self::split_workflow_receiver(&wf_event) else {
                    continue;
                };
                if receiver == *local_device_id {
                    continue;
                }

                // Pending workflows are still needed for upload. Every terminal or
                // already-sent remote workflow is only a stale local copy.
                let is_pending = matches!(
                    claim.status.get(&receiver),
                    Some(SsDistributionStatus::Pending)
                );
                if !is_pending {
                    self.p_obj.repo.delete(wf_event.obj_id()).await;
                }
            }
        }

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run sync gateway");

        loop {
            let creds_repo = PersistentCredentials {
                p_obj: self.p_obj.clone(),
                master_key: self.master_key.clone(),
            };

            let maybe_user_creds = creds_repo.get_user_creds().await.unwrap();
            let Some(user_creds) = maybe_user_creds else {
                async_std::task::sleep(Duration::from_millis(300)).await;
                continue;
            };

            let result = self.sync(user_creds.user()).await;
            if let Err(err) = result {
                error!("Sync error: {:?}", err);
            }

            async_std::task::sleep(Duration::from_millis(100)).await;
        }
    }

    ///Levels of synchronization:
    ///  - global index, server PK - when user has no account
    ///  - vault, shared secret... - user has been registered, we can sync vault related events
    #[instrument(skip_all)]
    pub async fn sync(&self, user: UserData) -> Result<()> {
        let user_creds = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        }
        .get_user_creds()
        .await?;
        let server_tail = self.get_server_tail(user.clone()).await?;

        self.sync_device_log(&server_tail, user.user_id(), &user).await?;

        let vault_sync_request = self.get_vault_request(user.clone()).await?;
        self.sync_vault(vault_sync_request, user_creds).await?;

        self.sync_shared_secrets(&server_tail, user).await?;

        Ok(())
    }

    async fn get_server_tail(&self, user_data: UserData) -> Result<ServerTailResponse> {
        let server_tail = {
            let server_tail_sync_request = self.get_server_tail_request(user_data).await?;
            self.get_tail(server_tail_sync_request).await?
        };
        Ok(server_tail)
    }

    async fn get_tail(&self, server_tail_sync_request: SyncRequest) -> Result<ServerTailResponse> {
        let server_tail_response = self
            .sync
            .send(server_tail_sync_request)
            .await?
            .to_server_tail()?;

        Ok(server_tail_response)
    }

    async fn get_server_tail_request(&self, user_data: UserData) -> Result<SyncRequest> {
        let sync_request =
            SyncRequest::Read(Box::from(ReadSyncRequest::ServerTail(ServerTailRequest {
                sender: user_data,
            })));
        Ok(sync_request)
    }

    #[instrument(skip(self))]
    async fn sync_vault(
        &self,
        vault_sync_request: SyncRequest,
        user_creds: Option<UserCreds>,
    ) -> Result<()> {
        let previous_members = if let Some(creds) = user_creds.as_ref() {
            self.local_vault_members(creds.vault_name.clone()).await?
        } else {
            Vec::new()
        };

        let DataEventsResponse(data_sync_events) =
            self.sync.send(vault_sync_request).await?.to_data()?;

        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New event from server: {:?}",
                self.id, new_event
            );

            self.p_obj.repo.save(new_event).await?;
        }

        // The approver's local update_membership path only has access to the
        // secrets owned by that approver. Every other existing sender must
        // reconcile its own shares when the canonical acceptance event arrives.
        // Do this once per newly observed member, before sync_shared_secrets uploads
        // the resulting workflows. The server clears VaultLog updates after applying
        // them, so compare the canonical VaultObject membership instead of relying on
        // transient update events. Never run this from get_app_state: that method is
        // called for every UI refresh and would repeatedly re-split the vault.
        if let Some(user_creds) = user_creds {
            let current_members = self
                .local_vault_members(user_creds.vault_name.clone())
                .await?;
            let previous_member_ids: HashSet<DeviceId> = previous_members
                .iter()
                .map(|member| member.user().device.device_id.clone())
                .collect();
            let orchestrator = MetaOrchestrator {
                p_obj: self.p_obj.clone(),
                user_creds,
            };

            for member in current_members {
                let member_id = member.user().device.device_id.clone();
                if previous_member_ids.contains(&member_id) {
                    continue;
                }

                let join_request = JoinClusterEvent {
                    candidate: member.user_data.clone(),
                };
                orchestrator
                    .redistribute_existing_secrets_for_join(&join_request)
                    .await?;
            }
        }

        Ok(())
    }

    async fn local_vault_members(
        &self,
        vault_name: crate::node::common::model::vault::vault::VaultName,
    ) -> Result<Vec<UserDataMember>> {
        let maybe_vault: Option<VaultObject> = self
            .p_obj
            .find_tail_event(VaultDescriptor::from(vault_name))
            .await?;

        Ok(maybe_vault
            .map(|vault| vault.to_data().members())
            .unwrap_or_default())
    }

    async fn get_vault_request(&self, user: UserData) -> Result<SyncRequest> {
        let vault_sync_request = {
            let sender = user.clone();
            let p_vault = PersistentVault::from(self.p_obj.clone());
            let tail = p_vault.vault_tail(user).await?;

            SyncRequest::Read(Box::from(ReadSyncRequest::Vault(VaultRequest {
                sender,
                tail,
            })))
        };
        Ok(vault_sync_request)
    }

    async fn sync_ss_device_log(&self, server_tail: &ServerTailResponse, user: &UserData) -> Result<()> {
        let device_id = user.device.device_id.clone();
        let server_ss_device_log_tail_id = {
            let unit_id = || ArtifactId::from(SsDeviceLogDescriptor::from(device_id));
            server_tail
                .ss_device_log_tail
                .clone()
                .unwrap_or_else(unit_id)
        };

        let ss_device_log_events_to_sync: Vec<SsDeviceLogObject> = self
            .p_obj
            .find_object_events(server_ss_device_log_tail_id)
            .await?;

        for ss_device_log_event in ss_device_log_events_to_sync {
            let signed = self.signed_event(ss_device_log_event.to_generic(), user).await?;
            let sync_request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(signed)));
            self.sync.send(sync_request).await?.ensure_success()?;
        }

        Ok(())
    }

    async fn sync_ss_log(&self, user: UserData) -> Result<()> {
        let vault_name = user.vault_name.clone();

        // Push a local recovery decision before reading the server snapshot. A receiver can
        // create its decision using the same SsLog sequence number as a concurrent decision
        // from another device. Reading that server event first would then overwrite the local
        // terminal decision before the workflow is uploaded.
        self.upload_local_recovery_workflows(&user).await?;

        let ss_sync_request = {
            let ss_log_free_id = {
                let obj_desc = SsLogDescriptor::from(vault_name.clone());
                self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
            };

            SyncRequest::Read(Box::from(ReadSyncRequest::SsRequest(SsRequest {
                sender: user.clone(),
                ss_log: ss_log_free_id,
            })))
        };

        let DataEventsResponse(data_sync_events) =
            self.sync.send(ss_sync_request).await?.to_data()?;

        debug!(
            ss_events_count = data_sync_events.len(),
            "sync_ss_log: events received from server"
        );
        for new_event in data_sync_events {
            debug!(
                "id: {:?}. Sync gateway. New ss event from server: {:?}",
                self.id, new_event
            );
            match &new_event {
                // Distribution objects are stored under a fixed key per (pass_id, receiver)
                // and are intentionally re-issued with fresh content on redistribution (see
                // redistribute_existing_secrets). repo.save() no-ops if a key already exists,
                // so a stale local copy from a previous split would otherwise never be
                // overwritten by the refreshed share pulled down here.
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) => {
                    self.p_obj.repo.delete(new_event.obj_id()).await;
                }
                // SsLog is server-canonical. Local recovery cleanup can allocate the same seq id
                // before the server response arrives; replace the local collision so the client
                // observes the approved claim from the server.
                GenericKvLogEvent::SsLog(_) => {
                    self.p_obj.repo.delete(new_event.obj_id()).await;
                }
                _ => {}
            }
            self.p_obj.repo.save(new_event).await?;
        }

        // Send claims
        let maybe_ss_log = self
            .p_obj
            .find_tail_event(SsLogDescriptor::from(vault_name.clone()))
            .await?;

        if let Some(ss_log) = maybe_ss_log.as_ref() {
            self.cleanup_foreign_sender_distributions(&user, ss_log)
                .await?;
        }

        if let Some(ss_log) = maybe_ss_log {
            for (_, claim) in ss_log.to_data().claims {
                let is_delivered = claim.status.status() == SsDistributionStatus::Delivered;
                if is_delivered {
                    continue;
                }

                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                match claim.distribution_type {
                    SecretDistributionType::Split => {
                        if !claim.sender.eq(&user.device.device_id)
                            || !Self::has_pending_split_receiver(&claim)
                        {
                            continue;
                        }

                        let wf_events = p_ss.get_distributions(claim.clone()).await?;

                        for wf_event in wf_events {
                            // Sent means the server already has this distribution. Only Pending
                            // receivers need an upload from the sender.
                            if !Self::split_workflow_has_pending_receiver(&claim, &wf_event) {
                                continue;
                            }

                            // A sender must retain only its own local Key Share. Remote shares are
                            // outbound payloads: once the server accepts the upload, remove the
                            // local workflow copy so the sender cannot decrypt another member's
                            // share and reconstruct a 3-device secret alone.
                            let obj_id = wf_event.obj_id();
                            let receiver = Self::split_workflow_receiver(&wf_event);
                            let request = {
                                let event = WriteSyncRequest::Event(
                                    self.signed_event(wf_event.to_generic(), &user).await?,
                                );
                                SyncRequest::Write(Box::from(event))
                            };
                            self.sync.send(request).await?.ensure_success()?;
                            if receiver.as_ref() != Some(&user.device.device_id) {
                                self.p_obj.repo.delete(obj_id).await;
                            }
                        }
                    }
                    SecretDistributionType::Recover => {
                        if claim.sender.eq(&user.device.device_id) {
                            continue;
                        }

                        let wf_events = p_ss.get_recoveries(claim.clone()).await?;
                        for wf_event in wf_events {
                            let obj_id = wf_event.obj_id();
                            let request = {
                                let event = WriteSyncRequest::Event(
                                    self.signed_event(wf_event.to_generic(), &user).await?,
                                );
                                SyncRequest::Write(Box::from(event))
                            };
                            self.sync.send(request).await?.ensure_success()?;
                            self.p_obj.repo.delete(obj_id).await;
                        }

                        // A recovery claim is terminal for the whole request as soon as
                        // any receiver declines it. `status()` is an aggregate view and
                        // intentionally remains Pending while another receiver is still
                        // pending, so using it here would leave the Decline workflow in
                        // the local database and let a later approval win the race.
                        let has_declined_receiver = claim
                            .status
                            .statuses
                            .values()
                            .any(|status| matches!(status, SsDistributionStatus::Declined));
                        if has_declined_receiver {
                            info!(
                                claim_id = ?claim.id,
                                "sync_ss_log: uploading recovery decline workflow"
                            );
                            let decline_events = p_ss.get_declines(claim.clone()).await?;
                            for wf_event in decline_events {
                                let obj_id = wf_event.obj_id();
                                let request = {
                                    let event = WriteSyncRequest::Event(
                                        self.signed_event(wf_event.to_generic(), &user).await?,
                                    );
                                    SyncRequest::Write(Box::from(event))
                                };
                                match self.sync.send(request).await {
                                    Ok(_) => {
                                        self.p_obj.repo.delete(obj_id).await;
                                    }
                                    Err(e) => {
                                        debug!(
                                            "Failed to push Decline workflow, will retry: {:?}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                };
            }
        }

        // Supplemental: upload Split distributions from our own ss_device_log.
        // Guards against stale ss_log contamination: when the local ss_log tail has a
        // claim whose sender != us (downloaded from another device), the block above
        // skips our distributions entirely. Reading our own ss_device_log is ground truth.
        {
            let own_claims: Vec<SsDeviceLogObject> = self
                .p_obj
                .get_object_events_from_beginning(SsDeviceLogDescriptor::from(
                    user.device.device_id.clone(),
                ))
                .await?;

            let current_claims = self
                .p_obj
                .find_tail_event(SsLogDescriptor::from(vault_name))
                .await?
                .map(|e: SsLogObject| e.to_data().claims)
                .unwrap_or_default();

            for raw_event in own_claims {
                let claim = raw_event.to_distribution_request();
                if claim.distribution_type != SecretDistributionType::Split {
                    continue;
                }
                let current_claim = current_claims.get(&claim.id).cloned();
                // If not found (ss_log may be contaminated with another device's claim), still
                // attempt upload. If found, upload only while a receiver is still Pending.
                if let Some(c) = current_claim.as_ref()
                    && !Self::has_pending_split_receiver(c)
                {
                    continue;
                }
                let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                let wf_events = p_ss.get_distributions(claim).await?;
                for wf_event in wf_events {
                    if let Some(c) = current_claim.as_ref()
                        && !Self::split_workflow_has_pending_receiver(c, &wf_event)
                    {
                        continue;
                    }

                    let obj_id = wf_event.obj_id();
                    let receiver = Self::split_workflow_receiver(&wf_event);
                    let request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(
                        self.signed_event(wf_event.to_generic(), &user).await?,
                    )));
                    self.sync.send(request).await?.ensure_success()?;
                    if receiver.as_ref() != Some(&user.device.device_id) {
                        self.p_obj.repo.delete(obj_id).await;
                    }
                }
            }
        }

        Ok(())
    }

    async fn upload_local_recovery_workflows(&self, user: &UserData) -> Result<()> {
        let Some(ss_log) = self
            .p_obj
            .find_tail_event(SsLogDescriptor::from(user.vault_name.clone()))
            .await?
        else {
            return Ok(());
        };

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        for (_, claim) in ss_log.to_data().claims {
            if claim.distribution_type != SecretDistributionType::Recover
                || claim.sender == user.device.device_id
            {
                continue;
            }

            let has_declined_receiver = claim
                .status
                .statuses
                .values()
                .any(|status| matches!(status, SsDistributionStatus::Declined));

            if has_declined_receiver {
                for wf_event in p_ss.get_declines(claim.clone()).await? {
                    let SsWorkflowObject::Decline(event) = &wf_event else {
                        continue;
                    };
                    if event.value.receiver_id != user.device.device_id {
                        continue;
                    }

                    let obj_id = wf_event.obj_id();
                    let request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(
                        self.signed_event(wf_event.to_generic(), user).await?,
                    )));
                    info!(claim_id = ?claim.id, "sync_ss_log: uploading local recovery decline first");
                    self.sync.send(request).await?.ensure_success()?;
                    self.p_obj.repo.delete(obj_id).await;
                }
                continue;
            }

            for wf_event in p_ss.get_recoveries(claim.clone()).await? {
                let SsWorkflowObject::Recovery(event) = &wf_event else {
                    continue;
                };
                let receiver_matches = matches!(
                    &event.key.obj_desc,
                    ObjectDescriptor::SharedSecret(SsWorkflowDescriptor::Recovery(recovery_id))
                        if recovery_id.distribution_id.receiver == user.device.device_id
                );
                if !receiver_matches {
                    continue;
                }

                let obj_id = wf_event.obj_id();
                let request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(
                    self.signed_event(wf_event.to_generic(), user).await?,
                )));
                info!(claim_id = ?claim.id, "sync_ss_log: uploading local recovery approval first");
                    self.sync.send(request).await?.ensure_success()?;
                self.p_obj.repo.delete(obj_id).await;
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn sync_device_log(
        &self,
        server_tail: &ServerTailResponse,
        user_id: UserId,
        user: &UserData,
    ) -> Result<()> {
        let device_log_events_to_sync = self.device_log_sync_request(server_tail, user_id).await?;
        for device_log_event in device_log_events_to_sync {
            let signed = self.signed_event(device_log_event, user).await?;
            let request = SyncRequest::Write(Box::from(WriteSyncRequest::Event(signed)));
            self.sync.send(request).await?.ensure_success()?;
        }

        Ok(())
    }

    async fn device_log_sync_request(
        &self,
        server_tail: &ServerTailResponse,
        user_id: UserId,
    ) -> Result<Vec<GenericKvLogEvent>> {
        let tail_to_sync = match &server_tail.device_log_tail {
            None => ArtifactId::from(DeviceLogDescriptor::from(user_id)),
            Some(server_tail_id) => server_tail_id.clone(),
        };

        let device_log_events_to_sync: Vec<GenericKvLogEvent> = self
            .p_obj
            .find_object_events::<DeviceLogObject>(tail_to_sync)
            .await?
            .into_iter()
            .map(|device_log_event| device_log_event.to_generic())
            .collect();
        Ok(device_log_events_to_sync)
    }

    #[instrument(skip(self))]
    async fn sync_shared_secrets(
        &self,
        server_tail: &ServerTailResponse,
        user: UserData,
    ) -> Result<()> {
        let vault_status = {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            p_vault.find(user.clone()).await?
        };

        let VaultStatus::Member(_) = vault_status else {
            return Ok(());
        };

        //sync ss_device_log and ss_log
        self.sync_ss_device_log(server_tail, &user)
            .await?;
        self.sync_ss_log(user).await?;

        Ok(())
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::app::sync::sync_gateway::SyncGateway;
    use crate::node::app::sync::sync_protocol::SyncProtocol;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct SyncGatewayFixture<Sync: SyncProtocol> {
        pub client_gw: Arc<SyncGateway<InMemKvLogEventRepo, Sync>>,
        pub vd_gw: Arc<SyncGateway<InMemKvLogEventRepo, Sync>>,
    }

    impl<Sync: SyncProtocol> SyncGatewayFixture<Sync> {
        pub fn from(state: &EmptyState, server_sync: Arc<Sync>) -> Self {
            let client_gw = Arc::new(SyncGateway {
                id: "client_gw".to_string(),
                p_obj: state.p_obj.client.clone(),
                sync: server_sync.clone(),
                master_key: state.device_creds.client_master_key.clone(),
            });

            let vd_gw = Arc::new(SyncGateway {
                id: "vd_gw".to_string(),
                p_obj: state.p_obj.vd.clone(),
                sync: server_sync,
                master_key: state.device_creds.vd_master_key.clone(),
            });

            Self { client_gw, vd_gw }
        }
    }
}
