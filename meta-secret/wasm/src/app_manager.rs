use anyhow::{bail, Context};
use std::sync::Arc;
use tracing::{Instrument, info, instrument};
use wasm_bindgen_futures::spawn_local;

use meta_secret_core::node::app::app_state_update_manager::ApplicationManagerConfigurator;

use crate::wasm_repo::WasmSyncProtocol;
use anyhow::Result;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::SyncProtocol;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::{client_span, vd_span};
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::ClaimId;
use meta_secret_core::node::common::model::user::common::UserDataOutsiderStatus;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
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

    pub async fn generate_user_creds(&self, vault_name: VaultName) {
        info!("Sign Up");
        let creds = GenericAppStateRequest::GenerateUserCreds(vault_name);
        self.meta_client_service.send_request(creds).await;
    }

    pub async fn sign_up(&self) -> Result<()> {
        info!("Sign Up");
        match self.get_state().await {
            ApplicationState::Local(_) => {
                bail!("Sign up is not allowed in local state");
            }
            ApplicationState::Vault(vault_info) => {
                let vault_name = match vault_info {
                    VaultFullInfo::NotExists(user) => user.vault_name,
                    VaultFullInfo::Outsider(outsider) => {
                        match outsider.status {
                            UserDataOutsiderStatus::NonMember => {
                                outsider.user_data.vault_name
                            }
                            UserDataOutsiderStatus::Pending => {
                                bail!("Sign up is not allowed in pending state");
                            }
                            UserDataOutsiderStatus::Declined => {
                                bail!("Sign up is not allowed in declined state");
                            }
                        }
                    }
                    VaultFullInfo::Member(_) => {
                        bail!("Sign up is not allowed in vault state");
                    }
                };
                let sign_up = GenericAppStateRequest::SignUp(vault_name);
                self.meta_client_service.send_request(sign_up).await;
                Ok(())
            }
        }
    }

    pub async fn cluster_distribution(&self, plain_pass_info: PlainPassInfo) {
        let request = GenericAppStateRequest::ClusterDistribution(plain_pass_info);
        self.meta_client_service.send_request(request).await;
    }

    pub async fn recover_js(&self, meta_pass_id: MetaPasswordId) {
        let request = GenericAppStateRequest::Recover(meta_pass_id);
        self.meta_client_service.send_request(request).await;
    }

    pub async fn get_state(&self) -> ApplicationState {
        self.meta_client_service.get_app_state().await.unwrap()
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        self.meta_client_service.accept_recover(claim_id).await
    }

    pub async fn accept_join(&self, join_request: JoinClusterEvent) -> Result<()> {
        self.meta_client_service.accept_join(join_request).await
    }

    #[instrument(name = "MetaClient", skip_all)]
    pub async fn client_setup(
        client_repo: Arc<Repo>,
        sync_protocol: Arc<WasmSyncProtocol<Repo>>,
    ) -> Result<ApplicationManager<Repo, WasmSyncProtocol<Repo>>> {
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

        let sync_gateway_rc = app_manager.sync_gateway.clone();
        spawn_local(async move { sync_gateway_rc.run().instrument(client_span()).await });

        Ok(app_manager)
    }

    #[instrument(name = "Vd", skip_all)]
    pub async fn virtual_device_setup(
        device_repo: Arc<Repo>,
        sync_protocol: Arc<WasmSyncProtocol<Repo>>,
    ) -> Result<()> {
        info!("virtual device initialization");

        let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

        let creds_repo = PersistentCredentials {
            p_obj: persistent_object.clone(),
        };

        let user_creds = creds_repo
            .get_or_generate_user_creds(DeviceName::virtual_device(), VaultName::test())
            .await?;
        let device_creds = Arc::new(user_creds.device_creds.clone());

        let dt_meta_client = Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        });

        let gateway = Arc::new(SyncGateway {
            id: String::from("vd-gateway"),
            p_obj: persistent_object.clone(),
            sync: sync_protocol.clone(),
            device_creds: device_creds.clone(),
        });

        let state_provider = Arc::new(MetaClientStateProvider::new());
        let meta_client_service = MetaClientService {
            data_transfer: dt_meta_client.clone(),
            sync_gateway: gateway.clone(),
            state_provider,
            p_obj: persistent_object.clone(),
            device_creds: device_creds.clone(),
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
