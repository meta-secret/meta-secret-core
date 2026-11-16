#[path = "app_manager_recovery_attempt.rs"]
mod recovery_attempt;
#[path = "app_manager_reveal.rs"]
mod reveal;
use crate::log_timestamp;
use anyhow::Result;
use anyhow::bail;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::api::{ReadSyncRequest, SsRecoveryCompletion, SyncRequest};
use meta_secret_core::node::app::app_manager_shared::{
    build_client_components, resolve_signup_vault_name,
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
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::join_events::JoinResultSubscription;
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
        if let Ok(user_creds) = self.meta_client_service.find_user_creds().await
            && let Err(original) = self.sync_gateway.sync(user_creds.user()).await
        {
            if let Ok(Some(local @ ApplicationState::Local(_))) = self
                .meta_client_service
                .try_removed_state_after_failed_sync(&user_creds)
                .await
            {
                return Ok(local);
            }
            return Err(original);
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

    pub async fn join_result_auth_token(&self) -> Result<String> {
        let user_creds = self.meta_client_service.find_user_creds().await?;
        let request = PersistentDeviceLog::from(self.meta_client_service.p_obj.clone())
            .latest_join_request(&user_creds.user())
            .await?
            .ok_or_else(|| anyhow::anyhow!("No local JOIN request exists for this device"))?;
        let key_manager = user_creds.device_creds.key_manager()?;
        JoinResultSubscription::sign(&request, &key_manager.dsa)?.bearer_token()
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
        let join_request = JoinClusterEvent {
            candidate,
            request_id: String::new(),
        };
        self.meta_client_service
            .update_membership(join_request, upd)
            .await
    }

    pub async fn remove_member(&self, target_device_id: String) -> Result<()> {
        self.meta_client_service
            .remove_member(target_device_id)
            .await
    }

    pub async fn resolve_secret(&self, identifier: &str) -> Result<MetaPasswordId> {
        self.get_state().await?;
        self.meta_client_service.resolve_secret(identifier).await
    }

    pub async fn delete_secret(&self, identifier: String) -> Result<()> {
        let pass_id = self.resolve_secret(&identifier).await?;
        self.meta_client_service.delete_secret(pass_id).await
    }

    pub async fn decide_delete_secret(&self, request_id: String, accepted: bool) -> Result<()> {
        self.meta_client_service
            .decide_delete_secret(request_id, accepted)
            .await
    }

    pub async fn approve_remove_member(&self, target_device_id: String) -> Result<()> {
        self.meta_client_service
            .approve_remove_member(target_device_id)
            .await
    }

    pub async fn decline_remove_member(&self, target_device_id: String) -> Result<()> {
        self.meta_client_service
            .decline_remove_member(target_device_id)
            .await
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
        meta_secret_core::node::db::objects::persistent_vault::PersistentVault::from(
            self.sync_gateway.p_obj.clone(),
        )
        .get_vault(user_creds.vault_name.clone())
        .await?
        .to_data()
        .ensure_local_secret_available(user_creds.device_id(), &pass_id)?;
        use meta_secret_core::recover_from_shares;

        let data = meta_secret_core::secret::local_share::load_local_distribution(
            &self.sync_gateway.p_obj,
            &user_creds,
            &pass_id,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("Distribution not found for local Secret"))?;
        let share = meta_secret_core::secret::local_share::decode_local_share(
            &data,
            &user_creds,
            meta_secret_core::secret::local_share::LocalShareProfile::WholeCopy,
        )?;

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
