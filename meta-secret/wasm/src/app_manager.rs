use anyhow::{Result, bail};
use std::sync::Arc;
use tracing::{Instrument, error, info, instrument};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::app::app_manager_shared::{
    build_client_components, find_recovery_claim_id_from_state, recover_plain_text,
    resolve_signup_vault_name,
};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::MetaClientService;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
use meta_secret_core::node::common::meta_tracing::client_span;
use meta_secret_core::node::common::model::device::common::{DeviceName, DeviceType};
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::secret::SsDistributionId;
use meta_secret_core::node::common::model::user::common::UserData;
use meta_secret_core::node::common::model::user::user_creds::UserCreds;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::secret::shared_secret::PlainText;
use meta_secret_core::secret::shared_secret::UserShareDto;
use meta_secret_core::recover_from_shares;

pub struct ApplicationManager<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub meta_client_service: Arc<MetaClientService<Repo, Sync>>,
    pub server: Arc<Sync>,
    pub sync_gateway: Arc<SyncGateway<Repo, Sync>>,
    pub master_key: TransportSk,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> ApplicationManager<Repo, Sync> {
    pub fn new(
        server: Arc<Sync>,
        sync_gateway: Arc<SyncGateway<Repo, Sync>>,
        meta_client_service: Arc<MetaClientService<Repo, Sync>>,
        master_key: TransportSk,
    ) -> ApplicationManager<Repo, Sync> {
        info!("New. Application State Manager");

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
        Self::init_with_device(
            client_repo,
            master_key,
            DeviceName::from("web_device"),
            DeviceType::web(),
        )
        .await
    }

    pub async fn init_with_device(
        client_repo: Arc<Repo>,
        master_key: TransportSk,
        device_name: DeviceName,
        device_type: DeviceType,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
        info!("Initialize application state manager");

        let sync_protocol = Arc::new(HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        });

        Self::client_setup(
            client_repo,
            sync_protocol,
            master_key,
            device_name,
            device_type,
        )
        .await
    }

    pub async fn generate_user_creds(&self, vault_name: VaultName) {
        info!("Generate user credentials for vault: {}", vault_name);
        let creds = GenericAppStateRequest::GenerateUserCreds(vault_name);
        self.meta_client_service.send_request(creds).await.unwrap();
    }

    #[instrument(skip(self))]
    pub async fn sign_up(&self) -> Result<ApplicationState> {
        info!("Sign Up");
        let state = self.get_state().await;
        let vault_name = resolve_signup_vault_name(&state)?;
        let sign_up = GenericAppStateRequest::SignUp(vault_name);
        let new_state = self.meta_client_service.send_request(sign_up).await?;
        info!("Sign Up. Completed");
        Ok(new_state)
    }

    pub async fn cluster_distribution(&self, plain_pass_info: PlainPassInfo) {
        let request = GenericAppStateRequest::ClusterDistribution(plain_pass_info);
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap();
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        let request = GenericAppStateRequest::Recover(meta_pass_id);
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap();
    }

    pub async fn get_state(&self) -> ApplicationState {
        let request = GenericAppStateRequest::GetState;
        self.meta_client_service
            .send_request(request)
            .await
            .unwrap()
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        self.meta_client_service.accept_recover(claim_id).await
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
        match &self.get_state().await {
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
                        return self.show_local_secret(user_creds, pass_id).await;
                    }

                    let claim_id = self.find_claim_by_pass_id(&pass_id).await;
                    match claim_id {
                        None => bail!("Claim id not found"),
                        Some(claim_id) => {
                            recover_plain_text(
                                self.sync_gateway.as_ref(),
                                user_creds,
                                claim_id,
                                pass_id,
                            )
                            .await
                        }
                    }
                }
            },
        }
    }

    async fn show_local_secret(
        &self,
        user_creds: UserCreds,
        pass_id: MetaPasswordId,
    ) -> Result<PlainText> {
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

        Ok(recover_from_shares(vec![share])?)
    }

    pub async fn clean_up_database(&self) {
        self.sync_gateway.p_obj.repo.db_clean_up().await
    }

    pub async fn find_claim_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<ClaimId> {
        let state = self.get_state().await;
        find_recovery_claim_id_from_state(&state, pass_id)
    }

    #[instrument(name = "MetaClientService", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<HttpSyncProtocol>,
        master_key: TransportSk,
        device_name: DeviceName,
        device_type: DeviceType,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
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

        spawn_local(async move {
            if let Err(e) = meta_client_service.run().instrument(client_span()).await {
                error!(error = %e, "Meta client background task failed");
            }
        });

        Ok(app_manager)
    }
}
