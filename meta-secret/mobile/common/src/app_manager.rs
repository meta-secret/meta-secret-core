use crate::log_timestamp;
use anyhow::Result;
use anyhow::bail;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::api::{ReadSyncRequest, SsRecoveryCompletion, SyncRequest};
use meta_secret_core::node::app::app_manager_shared::{
    build_client_components, recover_plain_text_by_claim, resolve_signup_vault_name,
};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::MetaClientService;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
use meta_secret_core::node::common::meta_tracing::client_span;
use meta_secret_core::node::common::model::device::common::{DeviceName, DeviceType};
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::{
    ClaimId, RecoveryClientStatus, SecretDistributionType, SsClaim, SsDistributionId,
    SsDistributionStatus, SsRecoveryId,
};
use meta_secret_core::node::security::sign_recovery_completion;
use meta_secret_core::node::state_events::StateEventsSubscription;

use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::user::user_creds::UserCreds;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::secret::shared_secret::PlainText;
use std::sync::Arc;
use std::thread;
use tracing::{Instrument, info, instrument, warn};

pub struct ApplicationManager<Repo: KvLogEventRepo + Send + Sync, SyncP: SyncProtocol + Send + Sync>
{
    pub meta_client_service: Arc<MetaClientService<Repo, SyncP>>,
    pub server: Arc<SyncP>,
    pub sync_gateway: Arc<SyncGateway<Repo, SyncP>>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo + Send + Sync + 'static, SyncP: SyncProtocol + Send + Sync + 'static>
    ApplicationManager<Repo, SyncP>
{
    pub fn new(
        server: Arc<SyncP>,
        sync_gateway: Arc<SyncGateway<Repo, SyncP>>,
        meta_client_service: Arc<MetaClientService<Repo, SyncP>>,
        master_key: TransportSk,
    ) -> ApplicationManager<Repo, SyncP> {
        println!("🦀Mobile App Manager: New. Application State Manager");

        ApplicationManager {
            server,
            sync_gateway,
            meta_client_service,
            master_key,
        }
    }

    pub async fn init(
        client_repo: Arc<Repo>,
        master_key: TransportSk,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
        println!("🦀Mobile App Manager: Initialize application state manager");

        let sync_protocol = Arc::new(HttpSyncProtocol {
            api_url: ApiUrl::selected(),
        });

        let app_manager = Self::client_setup(
            client_repo,
            sync_protocol.clone(),
            master_key,
            DeviceName::client(),
            DeviceType::other(),
        )
        .await?;

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
                    println!(
                        "🦀❌ Mobile App Manager: Meta client service error: {:?}",
                        e
                    );
                }
            });
        });

        Ok(())
    }

    pub async fn generate_user_creds(&self, vault_name: VaultName) -> Result<ApplicationState> {
        println!(
            "🦀 Mobile App Manager: Generate user credentials for vault: {}",
            vault_name
        );
        let creds = GenericAppStateRequest::GenerateUserCreds(vault_name);
        let app_state = self.meta_client_service.send_request(creds).await?;
        Ok(app_state)
    }

    #[instrument(skip(self))]
    pub async fn sign_up(&self) -> Result<ApplicationState> {
        info!("Sign Up");

        let state = self.get_state().await?;
        let vault_name = resolve_signup_vault_name(&state)?;
        let sign_up = GenericAppStateRequest::SignUp(vault_name);
        let new_state = self.meta_client_service.send_request(sign_up).await?;
        println!("🦀 Mobile App Manager: Sign Up. Completed");

        Ok(new_state)
    }

    pub async fn cluster_distribution(&self, plain_pass_info: PlainPassInfo) {
        let request = GenericAppStateRequest::ClusterDistribution(plain_pass_info);
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap();
    }

    pub async fn find_meta_password_id_by_secret_id(
        &self,
        secret_id: &str,
    ) -> Result<Option<MetaPasswordId>> {
        println!("🦀 Mobile App Manager: Find meta password id by secret id");
        let state = self.get_state().await?;

        let ApplicationState::Vault(vault_info) = state else {
            return Ok(None);
        };

        let VaultFullInfo::Member(member) = vault_info else {
            return Ok(None);
        };

        let found_secret = member
            .member
            .vault
            .secrets
            .iter()
            .find(|secret| secret.id.text.base64_str() == secret_id)
            .cloned();

        println!("🦀 Mobile App Manager: Secret lookup completed");

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
        if let Ok(user_creds) = self.meta_client_service.find_user_creds().await {
            self.sync_gateway.sync(user_creds.user()).await?;
        }

        let request = GenericAppStateRequest::GetState;
        self.meta_client_service.send_request(request).await
    }

    pub async fn state_events_auth_token(&self, vault_name: VaultName) -> Result<String> {
        let user_creds = self.meta_client_service.find_user_creds().await?;
        if user_creds.vault_name != vault_name {
            bail!("state events Vault does not match local credentials");
        }
        let key_manager = user_creds.device_creds.key_manager()?;
        StateEventsSubscription::sign(vault_name, user_creds.device_id().clone(), &key_manager.dsa)?
            .bearer_token()
    }

    pub async fn accept_recover_mobile(&self, claim_id: ClaimId) -> Result<()> {
        info!(claim_id = ?claim_id, "accept_recover_mobile: started");

        // Force sync before checking claims to ensure we have latest distribution events
        let user_creds = self.meta_client_service.find_user_creds().await?;
        info!(claim_id = ?claim_id, "accept_recover_mobile: syncing before claim lookup");
        self.sync_gateway.sync(user_creds.user()).await?;
        info!(claim_id = ?claim_id, "accept_recover_mobile: pre-accept sync completed");

        let state = self.get_state().await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state");
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member");
        };

        let claim = member
            .ss_claims
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();

        info!(
            claim_id = ?claim_id,
            sender = ?claim.sender,
            client_status = ?claim.client_status,
            "accept_recover_mobile: claim found, dispatching approval"
        );

        let result = self.accept_recover(claim_id.clone()).await;
        match &result {
            Ok(()) => {
                info!(claim_id = ?claim_id, "accept_recover_mobile: approval event created");
                // The UI must not finish the action before the approval is visible
                // to the server. Otherwise a later decline can overtake a local
                // approval (or vice versa) simply because the background sync
                // runs after the alert has already disappeared.
                self.sync_gateway.sync(user_creds.user()).await?;
                info!(claim_id = ?claim_id, "accept_recover_mobile: approval synced");
            }
            Err(error) => {
                warn!(claim_id = ?claim_id, error = %error, "accept_recover_mobile: approval failed")
            }
        }
        result
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

        let _ = member
            .ss_claims
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();

        self.meta_client_service
            .decline_recover(claim_id.clone())
            .await?;
        // Do not hide a failed upload behind a successful UI dismissal. The
        // decline is the terminal decision for the claim and must reach the
        // server before the alert is reported as processed.
        self.sync_gateway.sync(user_creds.user()).await?;
        println!(
            "🦀 Mobile App Manager: ✅ Decline recover completed for claim: {:?}",
            claim_id
        );
        Ok(())
    }

    pub async fn send_decline_completion(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Mobile App Manager: Send decline completion");
        let user_creds = self.meta_client_service.find_user_creds().await?;
        let state = self
            .meta_client_service
            .send_request(GenericAppStateRequest::GetState)
            .await?;
        let ApplicationState::Vault(vault_info) = state else {
            bail!("Not in vault state");
        };
        let VaultFullInfo::Member(member) = vault_info else {
            bail!("Not a member");
        };
        let claim = member
            .ss_claims
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("Claim not found: {:?}", claim_id))?
            .clone();
        let receiver_id = claim
            .receivers
            .iter()
            .find(|r| claim.status.get(r) == Some(&SsDistributionStatus::Declined))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("No declined receiver found for claim: {:?}", claim_id)
            })?;
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
        let key_manager = user_creds.device_creds.key_manager()?;
        let signed =
            sign_recovery_completion(completion, user_creds.device_id().clone(), &key_manager.dsa)?;
        let sync_request =
            SyncRequest::Read(Box::from(ReadSyncRequest::SsRecoveryCompletion(signed)));
        if let Err(e) = self.server.send(sync_request).await {
            println!(
                "🦀 Mobile App Manager: ❌ Failed to send recovery completion: {}",
                e
            );
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

    /// Show a recovered secret for the exact claim selected by Core.
    ///
    /// The Pass ID is derived from that claim, so a client cannot combine a
    /// Claim ID belonging to one secret with another secret identifier.
    pub async fn show_recovered(&self, claim_id: ClaimId) -> Result<PlainText> {
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
                    if vault_members_count <= 2 {
                        bail!("Recovery claim is not required for a replicated vault");
                    }

                    let claim = member
                        .ss_claims
                        .claims
                        .get(&claim_id)
                        .ok_or_else(|| anyhow::anyhow!("Claim id not found"))?;
                    if claim.distribution_type != SecretDistributionType::Recover {
                        bail!("Claim is not a recovery claim");
                    }
                    if claim.sender != *user_creds.device_id() {
                        bail!("Recovery claim belongs to another device");
                    }
                    if claim.client_status != Some(RecoveryClientStatus::Accepted) {
                        bail!("Recovery claim is not accepted");
                    }
                    let pass_id = claim.dist_claim_id.pass_id.clone();

                    info!(
                        pass_id = %pass_id.name,
                        sender = ?user_creds.device_id(),
                        claim_id = ?claim_id,
                        "mobile selected recovery claim for show_recovered"
                    );

                    let pass = recover_plain_text_by_claim(
                        self.sync_gateway.as_ref(),
                        user_creds.clone(),
                        claim_id.clone(),
                    )
                    .await?;

                    // Send recovery completion to mark claim as Delivered
                    let ts = log_timestamp::log_timestamp_utc();
                    println!(
                        "[{ts}] 🦀 App Manager: Send recovery completion to mark claim as Delivered"
                    );
                    let receiver = claim.approved_recovery_receiver()?;
                    let vault_name = user_creds.vault_name.clone();

                    let recovery_id = SsRecoveryId {
                        claim_id: claim.dist_claim_id.clone(),
                        sender: claim.sender.clone(),
                        distribution_id: SsDistributionId {
                            pass_id: pass_id.clone(),
                            receiver,
                        },
                    };

                    let completion = SsRecoveryCompletion {
                        vault_name,
                        recovery_id,
                        receiver_status: SsDistributionStatus::Sent,
                    };

                    let key_manager = user_creds.device_creds.key_manager()?;
                    let signed = sign_recovery_completion(
                        completion,
                        user_creds.device_id().clone(),
                        &key_manager.dsa,
                    )?;
                    let sync_request =
                        SyncRequest::Read(Box::from(ReadSyncRequest::SsRecoveryCompletion(signed)));

                    self.server.send(sync_request).await?;

                    // The sender's local claim is marked Delivered while
                    // recovering the secret.  Flush that terminal state before
                    // returning to the UI so the next Recover action cannot see
                    // the previous claim as still Accepted and reuse it.
                    self.sync_gateway.sync(user_creds.user()).await?;
                    info!(
                        pass_id = %pass_id.name,
                        claim_id = ?claim_id,
                        "mobile recovery completion synchronized"
                    );

                    Ok(pass)
                }
            },
        }
    }

    /// Show a locally replicated secret (one or two devices, no recovery claim).
    pub async fn show_local_secret(&self, pass_id: MetaPasswordId) -> Result<PlainText> {
        let user_creds = self.meta_client_service.find_user_creds().await?;
        self.show_local_secret_with_creds(user_creds, pass_id).await
    }

    async fn show_local_secret_with_creds(
        &self,
        user_creds: UserCreds,
        pass_id: MetaPasswordId,
    ) -> Result<PlainText> {
        use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
        use meta_secret_core::recover_from_shares;
        use meta_secret_core::secret::shared_secret::UserShareDto;

        let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: user_creds.device_id().clone(),
        });

        let dist = self
            .sync_gateway
            .p_obj
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
        if self.meta_client_service.find_user_creds().await.is_err() {
            return None;
        }
        let state = match self.get_state().await {
            Ok(state) => state,
            Err(_) => return None,
        };

        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            return None;
        };
        let user_creds = self.meta_client_service.find_user_creds().await.ok()?;
        member
            .ss_claims
            .find_unique_active_recovery_claim_id(user_creds.device_id(), pass_id)
            .ok()
            .flatten()
    }

    pub async fn find_claim_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<SsClaim> {
        if self.meta_client_service.find_user_creds().await.is_err() {
            return None;
        }
        let state = match self.get_state().await {
            Ok(state) => state,
            Err(_) => return None,
        };

        let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
            return None;
        };
        let user_creds = self.meta_client_service.find_user_creds().await.ok()?;
        let claim_id = member
            .ss_claims
            .find_unique_active_recovery_claim_id(user_creds.device_id(), pass_id)
            .ok()
            .flatten()?;
        member.ss_claims.claims.get(&claim_id).cloned()
    }

    #[instrument(name = "MetaClientService", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<HttpSyncProtocol>,
        master_key: TransportSk,
        device_name: DeviceName,
        device_type: DeviceType,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>>
    where
        HttpSyncProtocol: Send + Sync + 'static,
    {
        let (sync_gateway, meta_client_service) = build_client_components(
            client_repo,
            sync_protocol.clone(),
            master_key.clone(),
            device_name,
            device_type,
        )
        .await?;

        let app_manager = ApplicationManager::new(
            sync_protocol,
            sync_gateway,
            meta_client_service.clone(),
            master_key,
        );

        app_manager.run_service()?;

        Ok(app_manager)
    }
}
