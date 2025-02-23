use anyhow::bail;
use flume::{Receiver, Sender};
use std::sync::Arc;
use tracing::{error, info, instrument};

use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::{ApplicationState, UserMemberFullInfo, VaultFullInfo};
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::secret::MetaDistributor;
use anyhow::Result;

pub struct MetaClientService<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo, Sync>>,
    pub state_provider: Arc<MetaClientStateProvider>,
    pub p_obj: Arc<PersistentObject<Repo>>
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> MetaClientService<Repo, Sync> {
    #[instrument(skip_all)]
    pub async fn run(&self) -> Result<()> {
        info!("Run meta_app service");

        let mut service_state = self.build_service_state().await?;

        loop {
            let client_requests = self.data_transfer.dt.service_drain();
            for request in client_requests {
                let new_app_state = self
                    .handle_client_request(service_state.app_state, request)
                    .await?;
                service_state.app_state = new_app_state;
                self.state_provider.push(&service_state.app_state).await;
            }

            //handle client <-> server synchronization

            async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    pub async fn handle_client_request(
        &self,
        app_state: ApplicationState,
        request: GenericAppStateRequest,
    ) -> Result<ApplicationState> {
        info!(
            "Action execution. Request {:?}, state: {:?}",
            &request, &app_state
        );

        self.sync_gateway.sync().await?;
        self.sync_gateway.sync().await?;

        match request {
            GenericAppStateRequest::SignUp(vault_name) => match &app_state {
                ApplicationState::Local(device) => {
                    let user_data = UserData {
                        vault_name,
                        device: device.clone(),
                    };

                    let sign_up_claim = SignUpClaim::from(self.p_obj.clone());
                    sign_up_claim.sign_up(user_data.clone()).await?;
                    self.sync_gateway.sync().await?;
                }
                ApplicationState::Vault(VaultFullInfo::Member(UserMemberFullInfo {
                    member,
                    ..
                })) => {
                    error!("You are already a vault member: {:?}", member.vault);
                }
                ApplicationState::Vault(VaultFullInfo::NotExists(_)) => {
                    todo!("!!!")
                }
                ApplicationState::Vault(VaultFullInfo::Outsider(_)) => {
                    todo!("!!!")
                }
            },

            GenericAppStateRequest::ClusterDistribution(request) => {
                let user_creds = {
                    let creds_repo = PersistentCredentials::from(self.p_obj.clone());
                    let maybe_user_creds = creds_repo.get_user_creds().await?;

                    let Some(user_creds) = maybe_user_creds else {
                        bail!("Invalid state. UserCredentials must be present")
                    };

                    user_creds
                };

                let vault_status = {
                    let vault_repo = PersistentVault::from(self.p_obj.clone());
                    vault_repo.find(user_creds.user()).await?
                };

                match vault_status {
                    VaultStatus::NotExists(_) => {
                        bail!("Vault doesn't exists")
                    }
                    VaultStatus::Outsider(_) => {
                        bail!("Outsider user can't manage a vault")
                    }
                    VaultStatus::Member(member) => {
                        let p_vault = PersistentVault::from(self.p_obj());
                        let vault = p_vault.get_vault(member.user()).await?.to_data();
                        let vault_member = VaultMember { member, vault };
                        let distributor = MetaDistributor {
                            p_obj: self.p_obj.clone(),
                            vault_member: vault_member.clone(),
                            user_creds: Arc::new(user_creds),
                        };

                        distributor
                            .distribute(vault_member, request.pass_id, request.pass)
                            .await?;
                    }
                }
            }

            GenericAppStateRequest::Recover(meta_pass_id) => {
                let recovery_action = RecoveryAction::from(self.p_obj.clone());
                recovery_action.recovery_request(meta_pass_id).await?;
            }
        }

        self.sync_gateway.sync().await?;
        self.sync_gateway.sync().await?;

        //Update app state
        let app_state = self.get_app_state().await?;
        Ok(app_state)
    }

    pub async fn build_service_state(&self) -> Result<ServiceState<ApplicationState>> {
        let app_state = self.get_app_state().await?;
        let service_state = ServiceState { app_state };

        self.state_provider.push(&service_state.app_state).await;
        Ok(service_state)
    }

    pub async fn get_app_state(&self) -> Result<ApplicationState> {
        let creds_repo = PersistentCredentials::from(self.p_obj());
        let maybe_creds = creds_repo.find().await?;

        let app_state = match maybe_creds {
            None => {
                let device_creds = creds_repo
                    .get_or_generate_device_creds(DeviceName::client())
                    .await?;
                ApplicationState::Local(device_creds.device)
            }
            Some(creds) => match creds {
                CredentialsObject::Device(device_creds_event) => {
                    ApplicationState::Local(device_creds_event.value.device)
                }
                CredentialsObject::DefaultUser(user_creds) => {
                    let p_vault = PersistentVault::from(self.p_obj());
                    let vault_status = p_vault.find(user_creds.value.user()).await?;

                    match vault_status {
                        VaultStatus::NotExists(user) => {
                            ApplicationState::Vault(VaultFullInfo::NotExists(user))
                        }
                        VaultStatus::Outsider(outsider) => {
                            ApplicationState::Vault(VaultFullInfo::Outsider(outsider))
                        }
                        VaultStatus::Member(member_user) => {
                            let vault = p_vault.get_vault(&member_user.user_data).await?;

                            let ss_claims = {
                                let p_ss = PersistentSharedSecret::from(self.p_obj());
                                p_ss.get_ss_log_obj(user_creds.value.vault_name).await?
                            };

                            let user_full_info = UserMemberFullInfo {
                                member: VaultMember {
                                    member: member_user,
                                    vault: vault.to_data(),
                                },
                                ss_claims,
                            };
                            ApplicationState::Vault(VaultFullInfo::Member(user_full_info))
                        }
                    }
                }
            },
        };
        Ok(app_state)
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
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::app::meta_app::meta_client_service::{
        MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
    };
    use crate::node::app::sync::sync_gateway::fixture::SyncGatewayFixture;
    use crate::node::app::sync::sync_protocol::fixture::SyncProtocolFixture;
    use crate::node::app::sync::sync_protocol::EmbeddedSyncProtocol;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct MetaClientServiceFixture {
        pub client: Arc<MetaClientService<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
        pub vd: Arc<MetaClientService<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,

        pub state_provider: MetaClientStateProviderFixture,
        pub data_transfer: MetaClientDataTransferFixture,

        pub sync_gateway: SyncGatewayFixture,
    }

    impl MetaClientServiceFixture {
        pub fn from(base: &BaseState, sync: &SyncProtocolFixture) -> Self {
            let state_provider = MetaClientStateProviderFixture::generate();
            let dt_fxr = MetaClientDataTransferFixture::generate();

            let sync_gateway = SyncGatewayFixture::from(&base.empty, sync);

            let client = Arc::new(MetaClientService {
                data_transfer: dt_fxr.client.clone(),
                sync_gateway: sync_gateway.client_gw.clone(),
                state_provider: state_provider.client.clone(),
                p_obj: sync_gateway.client_gw.p_obj.clone(),
            });

            let vd = Arc::new(MetaClientService {
                data_transfer: dt_fxr.vd.clone(),
                sync_gateway: sync_gateway.vd_gw.clone(),
                state_provider: state_provider.vd.clone(),
                p_obj: sync_gateway.vd_gw.p_obj.clone(),
            });

            Self {
                client,
                vd,
                state_provider,
                data_transfer: dt_fxr,
                sync_gateway,
            }
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
                vd: Arc::new(MetaClientDataTransfer {
                    dt: MpscDataTransfer::new(),
                }),
            }
        }
    }
}
