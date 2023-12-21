use std::sync::Arc;

use anyhow::anyhow;
use tracing::{info, instrument};

use crate::node::app::app_state_update_manager::JsAppStateManager;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::user::UserDataOutsiderStatus;
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::events::local::CredentialsObject;
use crate::node::db::objects::device_log::PersistentDeviceLog;
use crate::node::db::objects::shared_secret::PersistentSharedSecret;
use crate::node::db::objects::vault::PersistentVault;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::MetaDistributor;

pub struct MetaClientService<Repo: KvLogEventRepo, StateManager: JsAppStateManager> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub state_manager: Arc<StateManager>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

impl<Repo, StateManager> MetaClientService<Repo, StateManager>
    where
        Repo: KvLogEventRepo,
        StateManager: JsAppStateManager
{
    #[instrument(skip_all)]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run meta_app service");

        let mut service_state = self.build_service_state().await?;

        while let Ok(request) = self.data_transfer.dt.service_receive().await {
            info!(
                "Action execution. Request {:?}, state: {:?}",
                &request, &service_state.state
            );

            self.sync_gateway.sync().await?;

            match request {
                GenericAppStateRequest::SignUp => {
                    let vault_status = self.sign_up().await?;
                    service_state.state.vault = Some(vault_status);
                }

                GenericAppStateRequest::ClusterDistribution(request) => {
                    let creds_repo = CredentialsRepo {
                        p_obj: self.sync_gateway.persistent_object.clone()
                    };

                    let user_creds = creds_repo.get_user_creds().await?;

                    let vault_repo = PersistentVault {
                        p_obj: self.sync_gateway.persistent_object.clone()
                    };
                    let vault = vault_repo.get_vault().await?;

                    let distributor = MetaDistributor {
                        persistent_obj: self.sync_gateway.persistent_object.clone(),
                        vault,
                        user_creds: Arc::new(user_creds),
                    };

                    distributor.distribute(request.pass_id, request.pass).await?;
                }

                GenericAppStateRequest::Recover(meta_pass_id) => {
                    let recovery_action = RecoveryAction {
                        persistent_obj: self.sync_gateway.persistent_object.clone(),
                    };
                    recovery_action
                        .recovery_request(meta_pass_id, &service_state.state)
                        .await?;
                }
            }

            self.on_update(&service_state.state).await;
        }

        Ok(())
    }

    async fn build_service_state(&self) -> anyhow::Result<ServiceState<ApplicationState>> {
        let mut service_state = ServiceState {
            state: ApplicationState::default()
        };

        let maybe_creds_event = {
            let creds_repo = CredentialsRepo {
                p_obj: self.sync_gateway.persistent_object.clone(),
            };
            creds_repo.find().await?
        };

        service_state.state.device = maybe_creds_event.map(|creds| creds.device());

        self.on_update(&service_state.state).await;
        Ok(service_state)
    }

    pub async fn on_update(&self, app_state: &ApplicationState) {
        // update app state in the external system (for instance, vue js)
        self.state_manager.update_js_state(app_state.clone()).await
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) {
        self.data_transfer.dt.send_to_service(request).await
    }

    async fn sign_up(&self) -> anyhow::Result<VaultStatus> {
        let creds = {
            let creds_repo = CredentialsRepo {
                p_obj: self.sync_gateway.persistent_object.clone(),
            };
            creds_repo.get().await?
        };

        match creds {
            CredentialsObject::Device { .. } => {
                Err(anyhow!("User credentials not found"))
            }
            CredentialsObject::DefaultUser(event) => {
                let user = event.value.user();

                //get vault status, if not member, then create request to join
                let p_vault = PersistentVault {
                    p_obj: self.sync_gateway.persistent_object.clone(),
                };

                let vault_status = p_vault
                    .find(user.clone())
                    .await?;

                if let VaultStatus::Outsider(outsider) = vault_status {
                    if let UserDataOutsiderStatus::Unknown = outsider.status {
                        let p_device_log = PersistentDeviceLog {
                            p_obj: self.sync_gateway.persistent_object.clone(),
                        };

                        p_device_log
                            .accept_join_cluster_request(user.clone())
                            .await?;

                        //Init SSDeviceLog
                        let p_ss_device_log = PersistentSharedSecret {
                            p_obj: self.sync_gateway.persistent_object.clone(),
                        };

                        p_ss_device_log.init(user.clone()).await?;

                        self.sync_gateway.sync().await?;
                    }
                }

                let vault_status = p_vault.find(user).await?;
                Ok(vault_status)
            }
        }
    }
}

pub struct MetaClientAccessProxy {
    pub dt: Arc<MetaClientDataTransfer>,
}

impl MetaClientAccessProxy {
    pub async fn send_request(&self, request: GenericAppStateRequest) {
        self.dt.dt.send_to_service(request).await
    }
}
