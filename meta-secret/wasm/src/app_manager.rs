use anyhow::{bail, Context};
use std::sync::Arc;
use tracing::{info, instrument, Instrument};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;

use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::{SyncProtocol};
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, vd_span};
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::wasm_repo::WasmSyncProtocol;
use anyhow::Result;
use meta_secret_core::node::app::meta_app::messaging::{ClusterDistributionRequest, GenericAppStateRequest};
use meta_secret_core::node::app::orchestrator::MetaOrchestrator;
use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo, WasmApplicationState};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::user::user_creds::UserCredentials;
use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
use meta_server_node::server::server_app::ServerApp;

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
        cfg: ApplicationManagerConfigurator<Repo>,
    ) -> Result<ApplicationManager<Repo, WasmSyncProtocol<Repo>>> {
        info!("Initialize application state manager");
        
        let sync_protocol = {
            let server = Arc::new(ServerApp::new(cfg.server_repo)?);
            Arc::new(WasmSyncProtocol { server })
        };

        Self::virtual_device_setup(cfg.device_repo, sync_protocol.clone()).await?;

        let app_manager = Self::client_setup(cfg.client_repo, sync_protocol.clone()).await?;

        Ok(app_manager)
    }

    pub async fn sign_up(&self, vault_name: VaultName) {
        info!("Sign Up");
        let sign_up = GenericAppStateRequest::SignUp(vault_name);
        self.meta_client_service.send_request(sign_up).await;
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        let request = GenericAppStateRequest::ClusterDistribution(ClusterDistributionRequest {
            pass_id: MetaPasswordId::build(pass_id),
            pass: pass.to_string(),
        });

        self.meta_client_service.send_request(request).await;
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        let request = GenericAppStateRequest::Recover(meta_pass_id);
        self.meta_client_service.send_request(request).await;
    }

    pub async fn get_state(&self) -> ApplicationState {
        self
            .meta_client_service
            .state_provider
            .get()
            .await
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        match &self.get_state().await {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => {
                match vault_info {
                    VaultFullInfo::NotExists(_) => {
                        bail!("Invalid state. Vault doesn't exist")
                    }
                    VaultFullInfo::Outsider(_) => {
                        bail!("Invalid state. User is outsider")
                    }
                    VaultFullInfo::Member(_) => {
                        let user_creds = self.get_user_creds().await?;

                        let orchestrator = MetaOrchestrator {
                            p_obj: self.sync_gateway.p_obj.clone(),
                            user_creds
                        };

                        orchestrator.accept_recover(claim_id).await?;
                        Ok(())
                    }
                }
            }
        }
    }

    async fn get_user_creds(&self) -> Result<UserCredentials> {
        let user_creds = {
            let creds_repo = PersistentCredentials::from(self.sync_gateway.p_obj.clone());
            let maybe_user_creds = creds_repo.get_user_creds().await?;

            let Some(user_creds) = maybe_user_creds else {
                bail!("Invalid state. UserCredentials must be present")
            };

            user_creds
        };
        Ok(user_creds)
    }

    pub async fn accept_join(&self, join_request: JoinClusterEvent) -> Result<()> {
        match &self.get_state().await {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => {
                match vault_info {
                    VaultFullInfo::NotExists(_) => {
                        bail!("Invalid state. Vault doesn't exist")
                    }
                    VaultFullInfo::Outsider(_) => {
                        bail!("Invalid state. User is outsider")
                    }
                    VaultFullInfo::Member(_) => {
                        let user_creds = self.get_user_creds().await?;
                        
                        let orchestrator = MetaOrchestrator {
                            p_obj: self.sync_gateway.p_obj.clone(),
                            user_creds
                        };

                        orchestrator.accept_join(join_request).await?;
                        Ok(())
                    }
                }
            }
        }
    }

    #[instrument(name = "MetaClient", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<WasmSyncProtocol<Repo>>,
    ) -> Result<ApplicationManager<Repo, WasmSyncProtocol<Repo>>> {
        let persistent_obj = {
            let obj = PersistentObject::new(client_repo.clone());
            Arc::new(obj)
        };

        let sync_gateway = Arc::new(SyncGateway {
            id: String::from("client-gateway"),
            p_obj: persistent_obj.clone(),
            sync: sync_protocol.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());

        let meta_client_service = {
            Arc::new(MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
                sync_gateway: sync_gateway.clone(),
                state_provider,
                p_obj: persistent_obj.clone(),
            })
        };

        let app_manager = ApplicationManager::new(sync_protocol, sync_gateway, meta_client_service.clone());

        spawn_local(async move {
            meta_client_service
                .run()
                .instrument(client_span())
                .await
                .with_context(|| "Meta client error")
                .unwrap();
        });

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        spawn_local(async move { sync_gateway_rc.run().instrument(client_span()).await });

        Ok(app_manager)
    }

    #[instrument(name = "Vd", skip_all)]
    pub async fn virtual_device_setup(
        device_repo: Arc<Repo>,
        sync_protocol: Arc<WasmSyncProtocol<Repo>>
    ) -> Result<()> {
        info!("virtual device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

        let creds_repo = PersistentCredentials {
            p_obj: persistent_object.clone(),
        };

        let _user_creds = creds_repo
            .get_or_generate_user_creds(DeviceName::virtual_device(), VaultName::test())
            .await?;

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            p_obj: persistent_object.clone(),
            sync: sync_protocol.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());
        let meta_client_service = MetaClientService {
            data_transfer: dt_meta_client.clone(),
            sync_gateway: gateway.clone(),
            state_provider,
            p_obj: persistent_object.clone(),
        };

        spawn_local(async move {
            meta_client_service
                .run()
                .instrument(vd_span())
                .await
                .unwrap();
        });

        let meta_client_access_proxy = Arc::new(MetaClientAccessProxy { dt: dt_meta_client });
        let vd = VirtualDevice::init(persistent_object, meta_client_access_proxy, gateway).await?;
        let vd = Arc::new(vd);
        spawn_local(async move { vd.run().instrument(vd_span()).await.unwrap() });

        Ok(())
    }
}
