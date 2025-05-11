use anyhow::{Context, bail};
use std::sync::Arc;
use tracing::{Instrument, info, instrument};
use wasm_bindgen_futures::spawn_local;

use anyhow::Result;
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
use meta_secret_core::node::common::model::secret::ClaimId;
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

pub struct ApplicationManager<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub meta_client_service: Arc<MetaClientService<Repo, Sync>>,
    pub server: Arc<Sync>,
    pub sync_gateway: Arc<SyncGateway<Repo, Sync>>,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> ApplicationManager<Repo, Sync> {
    pub fn new(
        server: Arc<Sync>,
        sync_gateway: Arc<SyncGateway<Repo, Sync>>,
        meta_client_service: Arc<MetaClientService<Repo, Sync>>,
    ) -> ApplicationManager<Repo, Sync> {
        info!("New. Application State Manager");

        ApplicationManager {
            server,
            sync_gateway,
            meta_client_service,
        }
    }

    pub async fn init(
        client_repo: Arc<Repo>,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
        info!("Initialize application state manager");

        let sync_protocol = Arc::new(HttpSyncProtocol {
            api_url: ApiUrl::prod(),
        });

        let app_manager = Self::client_setup(client_repo, sync_protocol.clone()).await?;

        Ok(app_manager)
    }

    pub async fn generate_user_creds(&self, vault_name: VaultName) {
        info!("Generate user credentials for vault: {}", vault_name);
        let creds = GenericAppStateRequest::GenerateUserCreds(vault_name);
        self.meta_client_service.send_request(creds).await.unwrap();
    }

    #[instrument(skip(self))]
    pub async fn sign_up(&self) -> Result<ApplicationState> {
        info!("Sign Up");
        match self.get_state().await {
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
                info!("Sign Up. Completed");
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

    pub async fn update_membership(&self, candidate: UserData, upd: JoinActionUpdate) -> Result<()> {
        let join_request = JoinClusterEvent { candidate };
        self.meta_client_service.update_membership(join_request, upd).await
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
                VaultFullInfo::Member(_) => {
                    let claim_id = self.find_claim_by_pass_id(&pass_id).await;

                    match claim_id {
                        None => {
                            bail!("Claim id not found");
                        }
                        Some(claim_id) => {
                            let recovery_handler = RecoveryHandler {
                                p_obj: self.sync_gateway.p_obj.clone(),
                            };

                            let pass = recovery_handler
                                .recover(user_creds, claim_id, pass_id)
                                .await?;
                            Ok(pass)
                        }
                    }
                }
            },
        }
    }

    pub async fn find_claim_by_pass_id(&self, pass_id: &MetaPasswordId) -> Option<ClaimId> {
        let ApplicationState::Vault(VaultFullInfo::Member(member)) = &self.get_state().await else {
            return None;
        };

        member.ss_claims.find_recovery_claim(pass_id)
    }

    #[instrument(name = "MetaClientService", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<HttpSyncProtocol>,
    ) -> Result<ApplicationManager<Repo, HttpSyncProtocol>> {
        let p_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };

        //Get or generate device credentials
        let creds_repo = PersistentCredentials::from(p_obj.clone());
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
            device_creds: device_creds.clone(),
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
                device_creds: device_creds.clone(),
            })
        };

        let app_manager =
            ApplicationManager::new(sync_protocol, sync_gateway, meta_client_service.clone());

        spawn_local(async move {
            meta_client_service
                .run()
                .instrument(client_span())
                .await
                .with_context(|| "Meta client error")
                .unwrap();
        });

        Ok(app_manager)
    }
}
