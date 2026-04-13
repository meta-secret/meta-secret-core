use anyhow::{bail};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, instrument, Instrument};
use anyhow::Result;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::api::{ReadSyncRequest, SsRecoveryCompletion, SyncRequest};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::client_span;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::{ClaimId, SecretDistributionType, SsClaim, SsDistributionId, SsDistributionStatus, SsRecoveryId};
use meta_secret_core::node::common::model::user::common::{UserData, UserDataOutsiderStatus};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::actions::recover::RecoveryHandler;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::secret::shared_secret::PlainText;
use std::thread;
use meta_secret_core::node::common::model::user::user_creds::UserCreds;
use crate::log_timestamp;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RecoveryStage {
    Stale,
    OneSent,
    Waiting,
    InProgress,
}

fn recovery_stage_from_claim(claim: &SsClaim) -> RecoveryStage {
    let has_pending = claim
        .status
        .statuses
        .values()
        .any(|s| matches!(s, SsDistributionStatus::Pending));
    let has_sent = claim
        .status
        .statuses
        .values()
        .any(|s| matches!(s, SsDistributionStatus::Sent));
    match (has_pending, has_sent) {
        (true, true) => RecoveryStage::InProgress,
        (true, false) => RecoveryStage::Waiting,
        (false, true) => RecoveryStage::OneSent,
        (false, false) => RecoveryStage::Stale,
    }
}

fn claim_selection_key(
    claim: &SsClaim,
    my_device_id: &meta_secret_core::node::common::model::device::common::DeviceId,
) -> (bool, RecoveryStage, String) {
    let sender_status = claim.status.get(my_device_id);
    let is_active = sender_status != Some(&SsDistributionStatus::Delivered);
    let stage = recovery_stage_from_claim(claim);
    let tie_breaker = claim.id.0.text.clone();
    (is_active, stage, tie_breaker)
}

pub struct ApplicationManager<Repo: KvLogEventRepo + Send + Sync, SyncP: SyncProtocol + Send + Sync> {
    pub meta_client_service: Arc<MetaClientService<Repo, SyncP>>,
    pub server: Arc<SyncP>,
    pub sync_gateway: Arc<SyncGateway<Repo, SyncP>>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo + Send + Sync + 'static, SyncP: SyncProtocol + Send + Sync + 'static> ApplicationManager<Repo, SyncP> {
    pub fn new(
        server: Arc<SyncP>,
        sync_gateway: Arc<SyncGateway<Repo, SyncP>>,
        meta_client_service: Arc<MetaClientService<Repo, SyncP>>,
        master_key: TransportSk
    ) -> ApplicationManager<Repo, SyncP> {
        println!("🦀Mobile App Manager: New. Application State Manager");

        ApplicationManager {
            server,
            sync_gateway,
            meta_client_service,
            master_key
        }
    }

    pub async fn init(
        client_repo: Arc<Repo>,
        master_key: TransportSk
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
        println!("🦀Mobile App Manager: Initialize application state manager");

        let sync_protocol = Arc::new(HttpSyncProtocol::new(ApiUrl::prod()));

        let app_manager = Self::client_setup(client_repo, sync_protocol.clone(), master_key).await?;

        Ok(app_manager)
    }
    
    pub fn run_service(&self) -> Result<()> {
        let meta_client_service_clone = self.meta_client_service.clone();
        
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                if let Err(e) = meta_client_service_clone
                    .run()
                    .instrument(client_span())
                    .await 
                {
                    println!("🦀❌ Mobile App Manager: Meta client service error: {:?}", e);
                }
            });
        });
        
        Ok(())
    }

    pub async fn generate_user_creds(&self, vault_name: VaultName) -> Result<ApplicationState> {
        println!("🦀 Mobile App Manager: Generate user credentials for vault: {}", vault_name);
        let creds = GenericAppStateRequest::GenerateUserCreds(vault_name);
        let app_state = self.meta_client_service.send_request(creds).await?;
        Ok(app_state)
    }

    #[instrument(skip(self))]
    pub async fn sign_up(&self) -> Result<ApplicationState> {
        info!("Sign Up");
        
        let state = self.get_state().await?;
        
        match state {
            ApplicationState::Local(_) => {
                bail!("Sign up is not allowed in local state");
            }
            ApplicationState::Vault(vault_info) => {
                let vault_name = match vault_info {
                    VaultFullInfo::NotExists(user) => user.vault_name,
                    VaultFullInfo::Outsider(outsider) => match outsider.status {
                        UserDataOutsiderStatus::NonMember => outsider.user_data.vault_name,
                        UserDataOutsiderStatus::Pending => {
                            bail!("Sign up is not allowed in pending state");
                        }
                        UserDataOutsiderStatus::Declined => {
                            bail!("Sign up is not allowed in declined state");
                        }
                    },
                    VaultFullInfo::Member(_) => {
                        bail!("Sign up is not allowed in vault state");
                    }
                };
                
                let sign_up = GenericAppStateRequest::SignUp(vault_name);
                let new_state = self.meta_client_service.send_request(sign_up).await?;
                println!("🦀 Mobile App Manager: Sign Up. Completed");
                
                Ok(new_state)
            }
        }
    }

    pub async fn cluster_distribution(&self, plain_pass_info: PlainPassInfo) {
        let request = GenericAppStateRequest::ClusterDistribution(plain_pass_info);
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap();
    }

    pub async fn find_meta_password_id_by_secret_id(&self, secret_id: &str) -> Result<Option<MetaPasswordId>> {
        println!("🦀 Mobile App Manager: Find meta password id by secret id");
        let state = self.get_state().await?;

        let ApplicationState::Vault(vault_info) = state else {
            return Ok(None);
        };

        let VaultFullInfo::Member(member) = vault_info else {
            return Ok(None);
        };

        let found_secret = member.member.vault.secrets
            .iter()
            .find(|secret| secret.id.text.base64_str() == secret_id)
            .cloned();

        println!("🦀 Mobile App Manager: Looking for secret with id: {}, found: {:?}", secret_id, found_secret);

        Ok(found_secret)
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        println!("🦀 Mobile App Manager: recover");
        let request = GenericAppStateRequest::Recover(meta_pass_id);
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap();
    }

    pub async fn get_state(&self) -> Result<ApplicationState> {
        let get_state_started_at = Instant::now();
        let request = GenericAppStateRequest::GetState;
        let state = self
            .meta_client_service
            .send_request(request)
            .await?;
        info!(
            get_state_elapsed_ms = get_state_started_at.elapsed().as_millis(),
            "mobile_app_manager: get_state completed"
        );
        Ok(
            state
        )
    }

    pub async fn accept_recover_mobile(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Mobile App Manager: Accept recover mobile");
        
        // Force sync before checking claims to ensure we have latest distribution events
        if let Ok(user_creds) = self.meta_client_service.find_user_creds().await {
            println!("🦀 Mobile App Manager: Force sync before accept_recover");
            self.sync_gateway.sync(user_creds.user()).await?;
            println!("🦀 Mobile App Manager: Force sync completed");
        }
        
        let state = self.get_state().await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state");
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member");
        };

        let _ = member.ss_claims.claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();
        
        self.accept_recover(claim_id).await
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        self.meta_client_service.accept_recover(claim_id).await
    }

    pub async fn decline_recover_mobile(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Mobile App Manager: Decline recover mobile");
        let user_creds = self.meta_client_service.find_user_creds().await?;
        println!("🦀 Mobile App Manager: Force sync before decline_recover");
        self.sync_gateway.sync(user_creds.user()).await?;

        let state = self.get_state().await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state");
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member");
        };

        let _ = member.ss_claims.claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();

        self.meta_client_service.decline_recover(claim_id.clone()).await?;
        if let Err(e) = self.sync_gateway.sync(user_creds.user()).await {
            println!("🦀 Mobile App Manager: ⚠️ Sync after decline failed (will retry): {}", e);
        }
        println!("🦀 Mobile App Manager: ✅ Decline recover completed for claim: {:?}", claim_id);
        Ok(())
    }

    pub async fn send_decline_completion(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Mobile App Manager: Send decline completion");
        let user_creds = self.meta_client_service.find_user_creds().await?;
        let state = self.meta_client_service.send_request(GenericAppStateRequest::GetState).await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state");
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member");
        };
        let claim = member.ss_claims.claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();
        let receiver_id = claim
            .receivers
            .iter()
            .find(|r| claim.status.get(r) == Some(&SsDistributionStatus::Declined))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No declined receiver found for claim: {:?}", claim_id))?;
        let vault_name = user_creds.vault_name.clone();
        let pass_id = claim.dist_claim_id.pass_id.clone();
        let recovery_id = SsRecoveryId {
            claim_id: claim.dist_claim_id.clone(),
            sender: claim.sender.clone(),
            distribution_id: SsDistributionId {
                pass_id,
                receiver: receiver_id.clone(),
            },
        };
        let completion = SsRecoveryCompletion {
            vault_name,
            recovery_id: recovery_id.clone(),
            receiver_status: SsDistributionStatus::Declined,
        };
        println!(
            "🦀 Mobile App Manager: Sending recovery completion for decline, sender: {:?}, receiver: {:?}",
            recovery_id.sender, recovery_id.distribution_id.receiver
        );
        let sync_request = SyncRequest::Read(Box::from(
            ReadSyncRequest::SsRecoveryCompletion(completion)
        ));
        if let Err(e) = self.server.send(sync_request).await {
            println!("🦀 Mobile App Manager: ❌ Failed to send recovery completion: {}", e);
            return Err(e);
        }
        println!("🦀 Mobile App Manager: ✅ Recovery completion sent successfully");
        Ok(())
    }

    pub async fn update_membership(
        &self,
        candidate: UserData,
        upd: JoinActionUpdate,
    ) -> Result<()> {
        let join_request = JoinClusterEvent { candidate };
        self.meta_client_service
            .update_membership(join_request, upd)
            .await
    }

    pub async fn show_recovered(&self, pass_id: MetaPasswordId) -> Result<PlainText> {
        let user_creds = self.meta_client_service.find_user_creds().await?;
        let state = self.get_state().await?;

        match state {
            ApplicationState::Local(_) => {
                bail!("Show recovered is not allowed in local state");
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Show recovered is not allowed in not exists state");
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Show recovered is not allowed in outsider state");
                }
                VaultFullInfo::Member(member) => {
                    let vault_members_count = member.member.vault.members().len();
                    
                    if vault_members_count == 1 {
                        println!("🦀 Mobile App Manager: Single device mode, showing local secret");
                        return self.show_local_secret(user_creds, pass_id).await;
                    }
                    
                    let claim_id = self.find_claim_id_by_pass_id(&pass_id).await;

                    match claim_id {
                        None => {
                            bail!("Claim id not found");
                        }
                        Some(claim_id) => {
                            let recovery_handler = RecoveryHandler {
                                p_obj: self.sync_gateway.p_obj.clone(),
                            };

                            let pass = recovery_handler
                                .recover(user_creds.clone(), claim_id.clone(), pass_id.clone())
                                .await?;
                            
                            // Send recovery completion to mark claim as Delivered
                            let ts = log_timestamp::log_timestamp_utc();
                            println!("[{ts}] 🦀 App Manager: Send recovery completion to mark claim as Delivered");
                            if let Some(claim) = member.ss_claims.claims.get(&claim_id) {
                                let vault_name = user_creds.vault_name.clone();
                                let device_id = user_creds.device_id();
                                
                                let recovery_id = SsRecoveryId {
                                    claim_id: claim.dist_claim_id.clone(),
                                    sender: claim.sender.clone(),
                                    distribution_id: SsDistributionId {
                                        pass_id: pass_id.clone(),
                                        receiver: device_id.clone(),
                                    },
                                };
                                
                                let completion = SsRecoveryCompletion {
                                    vault_name,
                                    recovery_id,
                                    receiver_status: SsDistributionStatus::Sent,
                                };
                                
                                let sync_request = SyncRequest::Read(Box::from(
                                    ReadSyncRequest::SsRecoveryCompletion(completion)
                                ));
                                
                                if let Err(e) = self.server.send(sync_request).await {
                                    println!("🦀 Mobile App Manager: ❌ Failed to send recovery completion: {}", e);
                                } else {
                                    println!("🦀 Mobile App Manager: ✅ Recovery completion sent successfully");
                                }
                            }
                            
                            Ok(pass)
                        }
                    }
                }
            },
        }
    }
    
    async fn show_local_secret(&self, user_creds: UserCreds, pass_id: MetaPasswordId) -> Result<PlainText> {
        use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
        use meta_secret_core::secret::shared_secret::UserShareDto;
        use meta_secret_core::recover_from_shares;
        
        let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: user_creds.device_id().clone(),
        });
        
        let dist = self.sync_gateway.p_obj
            .find_tail_event(desc)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Distribution not found for single device"))?
            .to_distribution_data()?;
        
        let transport_sk = &user_creds.device_creds.secret_box.transport.sk;
        let decrypted = dist.secret_message.cipher_text().decrypt(transport_sk)?;
        let share = UserShareDto::try_from(&decrypted.msg)?;
        
        let plain_text = recover_from_shares(vec![share])?;
        
        println!("🦀 Mobile App Manager: ✅ Local secret recovered successfully");
        Ok(plain_text)
    }

    pub async fn clean_up_database(&self) {
        self.sync_gateway.p_obj.repo.db_clean_up().await
    }

    pub async fn find_claim_id_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<ClaimId> {
        let user_creds = match self.meta_client_service.find_user_creds().await {
            Ok(user_creds) => user_creds,
            Err(_) => return None,
        };
        let state = match self.get_state().await {
            Ok(state) => state,
            Err(_) => return None,
        };
        
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            return None;
        };
        println!("🦀 Find claim id by pass id. State is Member");
        let my_device_id = user_creds.device_id();
        let selected = member
            .ss_claims
            .claims
            .values()
            .filter(|claim| {
                matches!(claim.distribution_type, SecretDistributionType::Recover)
                    && claim.dist_claim_id.pass_id.eq(pass_id)
                    && claim.sender.eq(my_device_id)
            })
            .max_by_key(|claim| claim_selection_key(claim, my_device_id));

        selected.map(|claim| claim.id.clone())
    }

    pub async fn find_claim_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<SsClaim> {
        let user_creds = match self.meta_client_service.find_user_creds().await {
            Ok(user_creds) => user_creds,
            Err(_) => return None,
        };
        let state = match self.get_state().await {
            Ok(state) => state,
            Err(_) => return None,
        };

        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            return None;
        };
        println!("🦀 Find claim by pass id. State is Member");
        let my_device_id = user_creds.device_id();
        member
            .ss_claims
            .claims
            .values()
            .filter(|claim| {
                matches!(claim.distribution_type, SecretDistributionType::Recover)
                    && claim.dist_claim_id.pass_id.eq(pass_id)
                    && claim.sender.eq(my_device_id)
            })
            .max_by_key(|claim| claim_selection_key(claim, my_device_id))
            .cloned()
    }

    #[instrument(name = "MetaClientService", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<HttpSyncProtocol>,
        master_key: TransportSk
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>>
    where
        HttpSyncProtocol: Send + Sync + 'static
    {
        let p_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };
        
        let creds_repo = PersistentCredentials {
            p_obj: p_obj.clone(),
            master_key: master_key.clone(),
        };
        let device_creds = {
            let creds = creds_repo
                .get_or_generate_device_creds(DeviceName::client())
                .await?;
            Arc::new(creds)
        };

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            p_obj: p_obj.clone(),
            sync: sync_protocol.clone(),
            master_key: master_key.clone()
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        let meta_client_service = {
            Arc::new(MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                sync_gateway: sync_gateway.clone(),
                state_provider,
                p_obj: p_obj.clone(),
                device_data: device_creds.device.clone(),
                master_key: master_key.clone()
            })
        };

        let app_manager = ApplicationManager::new(
            sync_protocol, 
            sync_gateway, 
            meta_client_service.clone(), 
            master_key
        );
        
        app_manager.run_service()?;

        Ok(app_manager)
    }
}
