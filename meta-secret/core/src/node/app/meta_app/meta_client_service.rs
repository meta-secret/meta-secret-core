use anyhow::bail;
use flume::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, instrument};

use crate::crypto::keys::TransportSk;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::orchestrator::MetaOrchestrator;
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::{DeviceData, DeviceId};
use crate::node::common::model::meta_pass::{MetaPasswordId, SecurePassInfo};
use crate::node::common::model::secret::ClaimId;
use crate::node::common::model::secret_deletion::deletion_states;
use crate::node::common::model::user::common::{
    UserData, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::common::model::{ApplicationState, UserMemberFullInfo, VaultFullInfo};
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::actions::secret_deletion::SecretDeletionAction;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use crate::node::db::actions::sign_up::join::JoinActionUpdate;
use crate::node::db::events::vault::vault_log_event::{
    JoinClusterEvent, VaultActionEvents, VaultActionUpdateEvent, VaultLogObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::secret::MetaDistributor;
use anyhow::Result;
use log::error;

pub struct MetaClientService<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo, Sync>>,
    pub state_provider: Arc<MetaClientStateProvider>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub device_data: DeviceData,
    pub master_key: TransportSk,
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

fn vault_contains_member(vault: &VaultData, device_id: &DeviceId) -> bool {
    matches!(vault.find_user(device_id), Some(UserMembership::Member(_)))
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> MetaClientService<Repo, Sync> {
    #[instrument(skip_all)]
    pub async fn run(&self) -> Result<()> {
        info!("Run meta_app service");

        let mut service_state = self.build_service_state().await?;

        loop {
            let request = self.data_transfer.dt.service_receive().await?;
            let new_app_state_result = self
                .handle_client_request(service_state.app_state.clone(), request)
                .await;

            let new_app_state = match new_app_state_result {
                Ok(new_state) => new_state,
                Err(err) => {
                    error!("Error while handling request: {:?}", err);
                    self.data_transfer
                        .dt
                        .send_to_client(GenericAppStateResponse::Error(err.to_string()))
                        .await;
                    continue;
                }
            };

            service_state.app_state = new_app_state;
            //self.state_provider.push(&service_state.app_state).await?;
            let response_state = GenericAppStateResponse::AppState(service_state.app_state.clone());
            self.data_transfer.dt.send_to_client(response_state).await;

            async_std::task::sleep(Duration::from_millis(100)).await;
        }
    }

    #[instrument(skip_all)]
    pub async fn handle_client_request(
        &self,
        app_state: ApplicationState,
        request: GenericAppStateRequest,
    ) -> Result<ApplicationState> {
        debug!(action = request.kind(), "Action execution");

        match &request {
            GenericAppStateRequest::DeleteSecret(_)
            | GenericAppStateRequest::DecideDeleteSecret { .. } => {
                let creds = self.find_user_creds().await?;
                self.sync_gateway.sync(creds.user()).await?;
                let deletion = SecretDeletionAction {
                    p_obj: self.p_obj(),
                    user_creds: creds.clone(),
                };
                match &request {
                    GenericAppStateRequest::DeleteSecret(pass_id) => {
                        Box::pin(deletion.request(pass_id.clone())).await?
                    }
                    GenericAppStateRequest::DecideDeleteSecret {
                        request_id,
                        accepted,
                    } => Box::pin(deletion.decide(request_id, *accepted)).await?,
                    _ => unreachable!(),
                }
                self.sync_gateway.sync(creds.user()).await?;
            }
            GenericAppStateRequest::GetState => {
                return self.get_app_state().await;
            }
            GenericAppStateRequest::GenerateUserCreds(_) => {
                self.get_user_creds(&request).await?;
            }
            GenericAppStateRequest::SignUp(_) => {
                info!("Handle sign up request");

                let user_creds = self.get_user_creds(&request).await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

                match &app_state {
                    ApplicationState::Local(_) => {
                        self.sign_up(&user_creds).await?;
                    }
                    ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                        let vault = &member_info.member.vault;
                        bail!("You are already a vault member: {:?}", vault);
                    }
                    ApplicationState::Vault(VaultFullInfo::NotExists(_)) => {
                        self.sign_up(&user_creds).await?;
                    }
                    ApplicationState::Vault(VaultFullInfo::Outsider(outsider)) => {
                        info!("Handle outsider sign up request");

                        match outsider.status {
                            UserDataOutsiderStatus::NonMember => {
                                info!("Handle outsider NON_MEMBER sign up request");
                                self.sign_up(&user_creds).await?;
                            }
                            UserDataOutsiderStatus::Pending => {
                                bail!("Your request is already in pending status")
                            }
                            UserDataOutsiderStatus::Declined => {
                                // Removal leaves a Declined marker, but a later
                                // explicit Sign Up starts a fresh Join request
                                // that still requires approval by a member.
                                self.sign_up(&user_creds).await?;
                            }
                        }
                    }
                }

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }

            GenericAppStateRequest::ClusterDistribution(plain_request) => {
                let user_creds = self.get_user_creds(&request).await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

                let vault_status = {
                    let vault_repo = PersistentVault::from(self.p_obj.clone());
                    vault_repo.find(user_creds.user()).await?
                };

                match vault_status {
                    VaultStatus::NotExists(_) => {
                        bail!("Vault doesn't exists")
                    }
                    VaultStatus::Outsider(_) => {
                        bail!("Outsider user can't manage a vault")
                    }
                    VaultStatus::Member(member) => {
                        let p_vault = PersistentVault::from(self.p_obj());
                        let vault = p_vault
                            .get_vault(member.user().vault_name())
                            .await?
                            .to_data();
                        let vault_member = VaultMember { member, vault };
                        let distributor = MetaDistributor {
                            p_obj: self.p_obj.clone(),
                            vault_member: vault_member.clone(),
                            user_creds: Arc::new(user_creds.clone()),
                        };

                        let secure_request = SecurePassInfo::from(plain_request.clone());
                        distributor.distribute(vault_member, secure_request).await?;
                    }
                }

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }

            GenericAppStateRequest::Recover(meta_pass_id) => {
                let user_creds = self.get_user_creds(&request).await?;

                info!(
                    pass_id = %meta_pass_id.name,
                    sender = ?user_creds.device_id(),
                    "recovery request received"
                );

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

                // A sender may close the reveal dialog and request the same secret again.
                // Reuse its still-active accepted claim instead of creating a second claim.
                let state = self.get_app_state().await?;
                if let ApplicationState::Vault(VaultFullInfo::Member(member)) = &state {
                    let existing_claim = member
                        .ss_claims
                        .find_unique_accepted_recovery_claim_id_for_sender(
                            user_creds.device_id(),
                            meta_pass_id,
                        )?;
                    info!(
                        pass_id = %meta_pass_id.name,
                        sender = ?user_creds.device_id(),
                        claim_id = ?existing_claim,
                        "checked for reusable accepted recovery claim"
                    );
                    if existing_claim.is_some() {
                        info!(
                            pass_id = %meta_pass_id.name,
                            sender = ?user_creds.device_id(),
                            "reusing accepted recovery claim; no new claim will be created"
                        );
                        // Do not return the state captured before the concurrent
                        // request from the other sender is persisted.  The caller
                        // must receive a fresh state so its socket/UI pipeline can
                        // observe the other sender's claim as well.
                        self.sync_gateway.sync(user_creds.user()).await?;
                    } else {
                        info!(pass_id = %meta_pass_id.name, sender = ?user_creds.device_id(), "creating new recovery claim");
                        let recovery_action = RecoveryAction::from(self.p_obj.clone());
                        recovery_action
                            .recovery_request(user_creds.clone(), meta_pass_id.clone())
                            .await?;

                        self.sync_gateway.sync(user_creds.user()).await?;
                        self.sync_gateway.sync(user_creds.user()).await?;
                    }
                } else {
                    info!(pass_id = %meta_pass_id.name, sender = ?user_creds.device_id(), "creating new recovery claim");
                    let recovery_action = RecoveryAction::from(self.p_obj.clone());
                    recovery_action
                        .recovery_request(user_creds.clone(), meta_pass_id.clone())
                        .await?;

                    self.sync_gateway.sync(user_creds.user()).await?;
                    self.sync_gateway.sync(user_creds.user()).await?;
                }
            }
            GenericAppStateRequest::RemoveMember(target_device_id) => {
                let user_creds = self.get_user_creds(&request).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                let member = match PersistentVault::from(self.p_obj.clone())
                    .find(user_creds.user())
                    .await?
                {
                    VaultStatus::Member(member) => member,
                    VaultStatus::Outsider(_) => anyhow::bail!("User is not a vault member"),
                    VaultStatus::NotExists(_) => anyhow::bail!("Vault does not exist"),
                };
                let vault = PersistentVault::from(self.p_obj.clone())
                    .get_vault(member.user().vault_name())
                    .await?
                    .to_data();
                let target = vault
                    .members()
                    .into_iter()
                    .find(|member| member.user().device.device_id.to_string() == *target_device_id)
                    .ok_or_else(|| anyhow::anyhow!("Removal target is not a vault member"))?;
                let orchestrator = MetaOrchestrator {
                    p_obj: self.p_obj.clone(),
                    user_creds: user_creds.clone(),
                };
                orchestrator.request_remove_member(target).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }
            GenericAppStateRequest::ApproveRemoveMember(target_device_id) => {
                let user_creds = self.get_user_creds(&request).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                let orchestrator = MetaOrchestrator {
                    p_obj: self.p_obj.clone(),
                    user_creds: user_creds.clone(),
                };
                orchestrator
                    .accept_remove_member(target_device_id.clone())
                    .await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }
            GenericAppStateRequest::DeclineRemoveMember(target_device_id) => {
                let user_creds = self.get_user_creds(&request).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                let orchestrator = MetaOrchestrator {
                    p_obj: self.p_obj.clone(),
                    user_creds: user_creds.clone(),
                };
                orchestrator
                    .decline_remove_member(target_device_id.clone())
                    .await?;
                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }
        }

        //Update app state
        let app_state = self.get_app_state().await?;
        Ok(app_state)
    }

    async fn sign_up(&self, user_creds: &UserCreds) -> Result<()> {
        let user_data = UserData {
            vault_name: user_creds.vault_name.clone(),
            device: user_creds.device(),
        };

        let sign_up_claim = SignUpClaim::from(self.p_obj.clone());
        sign_up_claim.sign_up(user_data.clone()).await?;

        Ok(())
    }

    async fn get_user_creds(&self, request: &GenericAppStateRequest) -> Result<UserCreds> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        };

        let user_creds = match &request {
            GenericAppStateRequest::GetState => {
                bail!("Invalid state. GetState request!")
            }
            GenericAppStateRequest::GenerateUserCreds(vault_name) => {
                let device_name = self.device_data.device_name.clone();
                let device_type = self.device_data.device_type.clone();
                creds_repo
                    .get_or_generate_user_creds_with_type(
                        device_name,
                        device_type,
                        vault_name.clone(),
                    )
                    .await?
            }
            GenericAppStateRequest::SignUp(vault_name) => {
                let device_name = self.device_data.device_name.clone();
                let device_type = self.device_data.device_type.clone();
                creds_repo
                    .get_or_generate_user_creds_with_type(
                        device_name,
                        device_type,
                        vault_name.clone(),
                    )
                    .await?
            }
            GenericAppStateRequest::ClusterDistribution(_) => self.find_user_creds().await?,
            GenericAppStateRequest::Recover(_) => self.find_user_creds().await?,
            GenericAppStateRequest::DeleteSecret(_)
            | GenericAppStateRequest::DecideDeleteSecret { .. } => self.find_user_creds().await?,
            GenericAppStateRequest::RemoveMember(_) => self.find_user_creds().await?,
            GenericAppStateRequest::ApproveRemoveMember(_) => self.find_user_creds().await?,
            GenericAppStateRequest::DeclineRemoveMember(_) => self.find_user_creds().await?,
        };
        Ok(user_creds)
    }

    pub async fn find_user_creds(&self) -> Result<UserCreds> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        };
        let user_creds = {
            let maybe_user_creds = creds_repo.get_user_creds().await?;

            let Some(user_creds) = maybe_user_creds else {
                bail!("Invalid state. UserCredentials must be present")
            };

            user_creds
        };

        Ok(user_creds)
    }

    pub async fn build_service_state(&self) -> Result<ServiceState<ApplicationState>> {
        let app_state = self.get_app_state().await?;
        let service_state = ServiceState { app_state };

        //self.state_provider.push(&service_state.app_state).await?;
        Ok(service_state)
    }

    pub async fn get_app_state(&self) -> Result<ApplicationState> {
        let creds_repo = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        };
        let maybe_user_creds = creds_repo.get_user_creds().await?;

        let app_state = match maybe_user_creds {
            None => ApplicationState::Local(self.device_data.clone()),
            Some(user_creds) => {
                self.sync_gateway.sync(user_creds.user()).await?;

                // A removal approval may carry Encrypted Key Shares for the
                // initiator. Reconcile them after sync, before exposing the
                // refreshed state to the client UI.
                let orchestrator = MetaOrchestrator {
                    p_obj: self.p_obj.clone(),
                    user_creds: user_creds.clone(),
                };
                orchestrator.finalize_pending_removal_resharing().await?;

                let p_vault = PersistentVault::from(self.p_obj());
                let vault_status = p_vault.find(user_creds.user()).await?;

                match vault_status {
                    VaultStatus::NotExists(user) => {
                        ApplicationState::Vault(VaultFullInfo::NotExists(user))
                    }
                    VaultStatus::Outsider(outsider) => {
                        if matches!(outsider.status, UserDataOutsiderStatus::Declined)
                            && self.was_removed_from_vault(&outsider).await?
                        {
                            // A removed device must not keep the old vault
                            // credentials or shares. Clear only this local
                            // database after the signed removal update has
                            // been synchronized; a future join starts with
                            // fresh credentials and requires approval again.
                            self.p_obj.repo.db_clean_up().await;
                            ApplicationState::Local(self.device_data.clone())
                        } else {
                            ApplicationState::Vault(VaultFullInfo::Outsider(outsider))
                        }
                    }
                    VaultStatus::Member(member_user) => {
                        let vault = p_vault
                            .get_vault(member_user.user_data.vault_name())
                            .await?;

                        Box::pin(
                            PersistentSharedSecret::from(self.p_obj())
                                .purge_deleted_secrets(&vault.clone().to_data()),
                        )
                        .await?;
                        let ss_claims = {
                            let p_ss = PersistentSharedSecret::from(self.p_obj());
                            let mut log = p_ss.get_ss_log_obj(user_creds.vault_name).await?;
                            let deleted = &vault.0.value.deleted_secrets;
                            log.claims.retain(|_, claim| {
                                !deleted
                                    .iter()
                                    .any(|pass| pass.id == claim.dist_claim_id.pass_id.id)
                            });
                            let current_device_id = &member_user.user_data.device.device_id;
                            let result = log.with_client_status(current_device_id);
                            for (id, claim) in &result.claims {
                                debug!(
                                    claim_id = ?id,
                                    distribution_type = ?claim.distribution_type,
                                    client_status = ?claim.client_status,
                                    "get_app_state: ss_claim client_status"
                                );
                            }
                            result
                        };

                        let maybe_vault_log_event = {
                            let vault_name = member_user.user_data.vault_name();
                            p_vault.vault_log(vault_name).await?
                        };

                        let vault_action_events = maybe_vault_log_event
                            .map(|obj| obj.0.value)
                            .unwrap_or(VaultActionEvents::default());

                        let deletion_states = deletion_states(
                            &vault.clone().to_data(),
                            &vault_action_events,
                            &member_user.user_data.device.device_id,
                        );
                        let user_full_info = UserMemberFullInfo {
                            deletion_states,
                            member: VaultMember {
                                member: member_user,
                                vault: vault.to_data().client_view(),
                            },
                            ss_claims,
                            vault_events: vault_action_events,
                        };
                        ApplicationState::Vault(VaultFullInfo::Member(Box::from(user_full_info)))
                    }
                }
            }
        };
        Ok(app_state)
    }

    async fn was_removed_from_vault(&self, outsider: &UserDataOutsider) -> Result<bool> {
        let p_vault = PersistentVault::from(self.p_obj());
        let device_id = &outsider.user_data.device.device_id;
        if let Ok(vault) = p_vault.get_vault(outsider.user_data.vault_name()).await
            && vault_contains_member(&vault.to_data(), device_id)
        {
            return Ok(true);
        }

        let Some(VaultLogObject(log)) = p_vault.vault_log(outsider.user_data.vault_name()).await?
        else {
            return Ok(false);
        };
        Ok(log.value.updates.iter().any(|update| {
            matches!(
                update,
                VaultActionUpdateEvent::RemoveMember(event)
                    if event.target.user_data.device.device_id == *device_id
                        && matches!(
                            event.decision,
                            crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted
                        )
            )
        }))
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) -> Result<ApplicationState> {
        let resp = self
            .data_transfer
            .dt
            .send_to_service_and_get(request)
            .await?;
        match resp {
            GenericAppStateResponse::AppState(app_state) => Ok(app_state),
            GenericAppStateResponse::Error(message) => anyhow::bail!(message),
        }
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        info!(claim_id = ?claim_id, "accept_recover: started");

        // The mobile caller has already synchronized and validated the claim.
        // Do not perform another full get_app_state/sync here: that re-enters
        // the shared sync gateway while UI refreshes may be in flight and was
        // the source of a parallel-approval stall. The orchestrator validates
        // membership and the claim against the same local snapshot before
        // writing the recovery workflow.
        let user_creds = self.find_user_creds().await?;
        let orchestrator = MetaOrchestrator {
            p_obj: self.sync_gateway.p_obj.clone(),
            user_creds,
        };

        orchestrator.accept_recover(claim_id.clone()).await?;
        info!(claim_id = ?claim_id, "accept_recover: recovery workflow created");
        Ok(())
    }

    pub async fn decline_recover(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Meta Client Service: decline claim_id: {:?}", claim_id);
        match &self.get_app_state().await? {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist")
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider")
                }
                VaultFullInfo::Member(_) => {
                    let user_creds = self.find_user_creds().await?;

                    let orchestrator = MetaOrchestrator {
                        p_obj: self.sync_gateway.p_obj.clone(),
                        user_creds,
                    };

                    orchestrator.decline_recover(claim_id).await?;
                    Ok(())
                }
            },
        }
    }

    pub async fn update_membership(
        &self,
        join_request: JoinClusterEvent,
        upd: JoinActionUpdate,
    ) -> Result<()> {
        match self.get_app_state().await? {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist")
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider")
                }
                VaultFullInfo::Member(_) => {
                    let user_creds = self.find_user_creds().await?;

                    let orchestrator = MetaOrchestrator {
                        p_obj: self.sync_gateway.p_obj.clone(),
                        user_creds,
                    };

                    orchestrator.update_membership(join_request, upd).await?;
                    Ok(())
                }
            },
        }
    }

    pub async fn remove_member(&self, target_device_id: String) -> Result<()> {
        self.send_request(GenericAppStateRequest::RemoveMember(target_device_id))
            .await
            .map(|_| ())
    }

    pub async fn delete_secret(&self, pass_id: MetaPasswordId) -> Result<()> {
        self.send_request(GenericAppStateRequest::DeleteSecret(pass_id))
            .await
            .map(|_| ())
    }

    pub async fn resolve_secret(&self, identifier: &str) -> Result<MetaPasswordId> {
        if identifier.is_empty() || identifier.len() > 512 {
            bail!("Invalid Secret identifier");
        }
        let creds = self.find_user_creds().await?;
        let vault = PersistentVault::from(self.p_obj())
            .get_vault(creds.vault_name.clone())
            .await?
            .to_data();
        if !vault.is_member(creds.device_id()) {
            bail!("User is not a Vault member");
        }
        let matches: Vec<_> = vault
            .secrets
            .iter()
            .filter(|pass| pass.name == identifier || pass.id_str() == identifier)
            .cloned()
            .collect();
        match matches.as_slice() {
            [pass] => Ok(pass.clone()),
            [] => bail!("Secret is deleted or does not exist"),
            _ => bail!("Secret name is ambiguous; use its ID"),
        }
    }

    pub async fn decide_delete_secret(&self, request_id: String, accepted: bool) -> Result<()> {
        self.send_request(GenericAppStateRequest::DecideDeleteSecret {
            request_id,
            accepted,
        })
        .await
        .map(|_| ())
    }

    pub async fn approve_remove_member(&self, target_device_id: String) -> Result<()> {
        self.send_request(GenericAppStateRequest::ApproveRemoveMember(
            target_device_id,
        ))
        .await
        .map(|_| ())
    }

    pub async fn decline_remove_member(&self, target_device_id: String) -> Result<()> {
        self.send_request(GenericAppStateRequest::DeclineRemoveMember(
            target_device_id,
        ))
        .await
        .map(|_| ())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.sync_gateway.p_obj.clone()
    }
}

pub struct MetaClientAccessProxy {
    pub dt: Arc<MetaClientDataTransfer>,
}

pub struct MetaClientStateProvider {
    sender: Sender<ApplicationState>,
    _receiver: Receiver<ApplicationState>,
}

impl MetaClientStateProvider {
    pub fn new() -> Self {
        let (sender, receiver) = flume::bounded(1);
        Self {
            sender,
            _receiver: receiver,
        }
    }

    pub async fn push(&self, state: &ApplicationState) -> Result<()> {
        self.sender.send_async(state.clone()).await?;
        Ok(())
    }
}

impl Default for MetaClientStateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::app::meta_app::meta_client_service::{
        MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
    };
    use crate::node::app::sync::sync_gateway::fixture::SyncGatewayFixture;
    use crate::node::app::sync::sync_protocol::SyncProtocol;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct MetaClientServiceFixture<Sync: SyncProtocol> {
        pub client: Arc<MetaClientService<InMemKvLogEventRepo, Sync>>,
        pub vd: Arc<MetaClientService<InMemKvLogEventRepo, Sync>>,

        pub state_provider: MetaClientStateProviderFixture,
        pub data_transfer: MetaClientDataTransferFixture,

        pub sync_gateway: SyncGatewayFixture<Sync>,
    }

    impl<Sync: SyncProtocol> MetaClientServiceFixture<Sync> {
        pub fn from(base: &BaseState, sync: Arc<Sync>) -> Self {
            let state_provider = MetaClientStateProviderFixture::generate();
            let dt_fxr = MetaClientDataTransferFixture::generate();

            let sync_gateway = SyncGatewayFixture::from(&base.empty, sync);

            let client = Arc::new(MetaClientService {
                data_transfer: dt_fxr.client.clone(),
                sync_gateway: sync_gateway.client_gw.clone(),
                state_provider: state_provider.client.clone(),
                p_obj: sync_gateway.client_gw.p_obj.clone(),
                device_data: base.empty.device_creds.client.device.clone(),
                master_key: base.p_creds.client_p_creds.master_key.clone(),
            });

            let vd = Arc::new(MetaClientService {
                data_transfer: dt_fxr.vd.clone(),
                sync_gateway: sync_gateway.vd_gw.clone(),
                state_provider: state_provider.vd.clone(),
                p_obj: sync_gateway.vd_gw.p_obj.clone(),
                device_data: base.empty.device_creds.vd.device.clone(),
                master_key: base.p_creds.vd_p_creds.master_key.clone(),
            });

            Self {
                client,
                vd,
                state_provider,
                data_transfer: dt_fxr,
                sync_gateway,
            }
        }
    }

    pub struct MetaClientStateProviderFixture {
        pub client: Arc<MetaClientStateProvider>,
        pub vd: Arc<MetaClientStateProvider>,
    }

    impl MetaClientStateProviderFixture {
        pub fn generate() -> Self {
            Self {
                client: Arc::new(MetaClientStateProvider::new()),
                vd: Arc::new(MetaClientStateProvider::new()),
            }
        }
    }

    pub struct MetaClientDataTransferFixture {
        client: Arc<MetaClientDataTransfer>,
        vd: Arc<MetaClientDataTransfer>,
    }

    impl MetaClientDataTransferFixture {
        pub fn generate() -> Self {
            Self {
                client: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                vd: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::vault_contains_member;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{
        UserDataOutsider, UserDataOutsiderStatus, UserMembership,
    };

    #[test]
    fn local_vault_distinguishes_removed_member_from_declined_join_candidate() {
        let registry = FixtureRegistry::empty();
        let current_user = registry.state.user_creds.client.user();
        let current_device_id = current_user.device.device_id.clone();
        let vault = registry.state.vault_data.full_membership;

        assert!(vault_contains_member(&vault, &current_device_id));

        let vault_after_removal =
            vault.update_membership(UserMembership::Outsider(UserDataOutsider {
                user_data: current_user,
                status: UserDataOutsiderStatus::Declined,
            }));
        assert!(!vault_contains_member(
            &vault_after_removal,
            &current_device_id
        ));
    }
}
