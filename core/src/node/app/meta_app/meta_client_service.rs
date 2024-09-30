use anyhow::bail;
use flume::{Receiver, Sender};
use std::sync::Arc;
use tracing::{error, info, instrument};

use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::vault::VaultStatus;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::common::UserData;
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::actions::sign_up_claim::SignUpClaim;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::MetaDistributor;

pub struct MetaClientService<Repo: KvLogEventRepo> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
    pub state_provider: Arc<MetaClientStateProvider>,
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

impl<Repo: KvLogEventRepo> MetaClientService<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run meta_app service");

        let p_obj = self.sync_gateway.p_obj.clone();

        let mut service_state = self.build_service_state().await?;

        while let Ok(request) = self.data_transfer.dt.service_receive().await {
            info!(
                "Action execution. Request {:?}, state: {:?}",
                &request, &service_state.app_state
            );

            self.sync_gateway.sync().await?;

            match request {
                GenericAppStateRequest::SignUp(vault_name) => match &service_state.app_state {
                    ApplicationState::Local { device } => {
                        let user_data = UserData {
                            vault_name,
                            device: device.clone(),
                        };

                        let sign_up_claim = SignUpClaim { p_obj: p_obj.clone() };
                        sign_up_claim.sign_up(user_data.clone()).await?;
                        self.sync_gateway.sync().await?;

                        let p_vault = PersistentVault { p_obj: self.p_obj() };
                        let new_status = p_vault.find(user_data.clone()).await?;
                        service_state.app_state = ApplicationState::Vault { vault: new_status };
                    }
                    ApplicationState::Vault { vault } => {
                        error!("You are already a vault member: {:?}", vault);
                    }
                },

                GenericAppStateRequest::ClusterDistribution(request) => {
                    let creds_repo = PersistentCredentials { p_obj: p_obj.clone() };

                    let maybe_user_creds = creds_repo.get_user_creds().await?;

                    match maybe_user_creds {
                        None => {
                            bail!("Invalid state. UserCredentials must be present")
                        }
                        Some(user_creds) => {
                            let vault_repo = PersistentVault { p_obj: p_obj.clone() };
                            let vault_status = vault_repo.find(user_creds.user()).await?;

                            match vault_status {
                                VaultStatus::NotExists(_) => {
                                    bail!("Vault doesn't exists")
                                }
                                VaultStatus::Outsider(_) => {
                                    bail!("Outsider user can't manage a vault")
                                }
                                VaultStatus::Member { vault, .. } => {
                                    let distributor = MetaDistributor {
                                        persistent_obj: p_obj.clone(),
                                        vault,
                                        user_creds: Arc::new(user_creds),
                                    };

                                    distributor.distribute(request.pass_id, request.pass).await?;
                                }
                            }
                        }
                    }
                }

                GenericAppStateRequest::Recover(meta_pass_id) => {
                    let recovery_action = RecoveryAction {
                        persistent_obj: p_obj.clone(),
                    };
                    recovery_action
                        .recovery_request(meta_pass_id, &service_state.app_state)
                        .await?;
                }
            }

            self.state_provider.push(&service_state.app_state).await
        }

        Ok(())
    }

    async fn build_service_state(&self) -> anyhow::Result<ServiceState<ApplicationState>> {
        let creds_repo = PersistentCredentials { p_obj: self.p_obj() };
        let maybe_creds = creds_repo.find().await?;

        let app_state = match maybe_creds {
            None => {
                let device_creds = creds_repo.get_or_generate_device_creds(DeviceName::generate())
                    .await?;
                ApplicationState::Local {
                    device: device_creds.device,
                }
            }
            Some(creds) => match creds {
                CredentialsObject::Device(device_creds_event) => ApplicationState::Local {
                    device: device_creds_event.value.device,
                },
                CredentialsObject::DefaultUser(user_creds) => {
                    let p_vault = PersistentVault { p_obj: self.p_obj() };
                    let vault_status = p_vault.find(user_creds.value.user()).await?;

                    ApplicationState::Vault { vault: vault_status }
                }
            },
        };

        let service_state = ServiceState { app_state };

        self.state_provider.push(&service_state.app_state).await;
        Ok(service_state)
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) {
        self.data_transfer.dt.send_to_service(request).await
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.sync_gateway.p_obj.clone()
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
    receiver: Receiver<ApplicationState>,
}

impl MetaClientStateProvider {
    pub fn new() -> Self {
        let (sender, receiver) = flume::bounded(100);
        Self { sender, receiver }
    }

    pub async fn get(&self) -> ApplicationState {
        self.receiver.recv_async().await.unwrap()
    }

    pub async fn push(&self, state: &ApplicationState) {
        self.sender.send_async(state.clone()).await.unwrap()
    }
}

#[cfg(test)]
pub mod fixture {
    use std::sync::Arc;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::app::meta_app::meta_client_service::{MetaClientDataTransfer, MetaClientService, MetaClientStateProvider};
    use crate::node::app::sync_gateway::fixture::SyncGatewayFixture;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;

    pub struct MetaClientServiceFixture {
        pub client: Arc<MetaClientService<InMemKvLogEventRepo>>,
        pub vd: Arc<MetaClientService<InMemKvLogEventRepo>>,

        pub state_provider: MetaClientStateProviderFixture,
        pub data_transfer: MetaClientDataTransferFixture,

        pub sync_gateway: SyncGatewayFixture,
    }

    impl MetaClientServiceFixture {
        pub fn from(base: &FixtureRegistry<BaseState>) -> Self {
            let state_provider = MetaClientStateProviderFixture::generate();
            let dt_fxr = MetaClientDataTransferFixture::generate();

            let sync_gateway = SyncGatewayFixture::from(&base);

            let client = Arc::new(MetaClientService {
                data_transfer: dt_fxr.client.clone(),
                sync_gateway: sync_gateway.client_gw.clone(),
                state_provider: state_provider.client.clone(),
            });

            let vd = Arc::new(MetaClientService {
                data_transfer: dt_fxr.vd.clone(),
                sync_gateway: sync_gateway.vd_gw.clone(),
                state_provider: state_provider.vd.clone(),
            });

            Self { client, vd, state_provider, data_transfer: dt_fxr, sync_gateway }
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
                vd: Arc::new(MetaClientDataTransfer { dt: MpscDataTransfer::new() }),
            }
        }
    }
}