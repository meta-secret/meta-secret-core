use std::sync::Arc;
use flume::{Receiver, Sender};
use tracing::{info, instrument};

use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::device::DeviceName;
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::actions::sign_up_claim::SignUpClaim;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::MetaDistributor;

pub struct MetaClientService<Repo: KvLogEventRepo> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
    pub state_provider: Arc<MetaClientStateProvider>
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

impl<Repo: KvLogEventRepo> MetaClientService<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run meta_app service");
        
        //todo!("get or generate device credentials");
        let p_obj = self.sync_gateway.persistent_object.clone();
        
        let mut service_state = self.build_service_state().await?;

        while let Ok(request) = self.data_transfer.dt.service_receive().await {
            info!(
                "Action execution. Request {:?}, state: {:?}",
                &request, &service_state.state
            );

            self.sync_gateway.sync().await?;

            match request {
                GenericAppStateRequest::SignUp => {
                    let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };

                    sign_up_claim.sign_up().await?;
                    self.sync_gateway.sync().await?;

                    let (_, vault_status) = sign_up_claim.get_vault_status().await?;

                    todo!("update vault status");
                    //service_state.state.vault = Some(vault_status);
                }

                GenericAppStateRequest::ClusterDistribution(request) => {
                    let creds_repo = CredentialsRepo { p_obj: p_obj.clone() };

                    let user_creds = creds_repo.get_user_creds().await?;

                    let vault_repo = PersistentVault {
                        p_obj: p_obj.clone(),
                    };
                    let vault = vault_repo.get_vault().await?;

                    let distributor = MetaDistributor {
                        persistent_obj: p_obj.clone(),
                        vault,
                        user_creds: Arc::new(user_creds),
                    };

                    distributor.distribute(request.pass_id, request.pass).await?;
                }

                GenericAppStateRequest::Recover(meta_pass_id) => {
                    let recovery_action = RecoveryAction {
                        persistent_obj: p_obj.clone(),
                    };
                    recovery_action
                        .recovery_request(meta_pass_id, &service_state.state)
                        .await?;
                }
            }
            
            self.state_provider.push(&service_state.state).await
        }

        Ok(())
    }

    async fn build_service_state(&self) -> anyhow::Result<ServiceState<ApplicationState>> {
        let maybe_creds_event = {
            let creds_repo = CredentialsRepo { p_obj: self.p_obj() };
            creds_repo.find().await?
        };

        let app_state = match maybe_creds_event {
            None => ApplicationState::Empty,
            Some(creds) => match creds {
                CredentialsObject::Device(device_creds_event) => ApplicationState::Local {
                    device: device_creds_event.value.device
                },
                CredentialsObject::DefaultUser(user_creds_event) => ApplicationState::User {
                    user: user_creds_event.value
                }
            }
        };

        let service_state = ServiceState {
            state: app_state,
        };
        
        self.state_provider.push(&service_state.state).await;
        Ok(service_state)
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) {
        self.data_transfer.dt.send_to_service(request).await
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.sync_gateway.persistent_object.clone()
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

pub struct MetaClientStateProvider {
    sender: Sender<ApplicationState>,
    receiver: Receiver<ApplicationState>
}

impl MetaClientStateProvider {
    pub fn new() -> Self {
        let (sender, receiver) = flume::bounded(100);
        Self {
            sender,
            receiver
        }
    }

    pub async fn get(&self) -> ApplicationState {
        self.receiver.recv_async().await.unwrap()
    }

    pub async fn push(&self, state: &ApplicationState) {
        self.sender.send_async(state.clone()).await.unwrap()
    }
}