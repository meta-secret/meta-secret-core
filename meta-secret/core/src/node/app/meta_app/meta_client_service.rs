use anyhow::bail;
use flume::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, instrument};

use crate::node::app::meta_app::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use crate::node::app::orchestrator::MetaOrchestrator;
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::actor::ServiceState;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::device_creds::DeviceCreds;
use crate::node::common::model::meta_pass::SecurePassInfo;
use crate::node::common::model::secret::ClaimId;
use crate::node::common::model::user::common::{UserData, UserDataOutsiderStatus};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::{ApplicationState, UserMemberFullInfo, VaultFullInfo};
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use crate::node::db::actions::sign_up::join::JoinActionUpdate;
use crate::node::db::events::vault::vault_log_event::{JoinClusterEvent, VaultActionEvents};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::secret::MetaDistributor;
use anyhow::Result;
use log::error;

pub struct MetaClientService<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub sync_gateway: Arc<SyncGateway<Repo, Sync>>,
    pub state_provider: Arc<MetaClientStateProvider>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub device_creds: Arc<DeviceCreds>,
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
            let request = self.data_transfer.dt.service_receive().await?;
            let new_app_state_result = self
                .handle_client_request(service_state.app_state, request)
                .await;

            let new_app_state = match new_app_state_result {
                Ok(new_state) => new_state,
                Err(err) => {
                    error!("Error while handling request: {:?}", err);
                    bail!("Error while handling request: {:?}", err);
                }
            };

            service_state.app_state = new_app_state;
            //self.state_provider.push(&service_state.app_state).await?;
            let response_state = GenericAppStateResponse::AppState(service_state.app_state.clone());
            self.data_transfer.dt.send_to_client(response_state).await;

            async_std::task::sleep(Duration::from_millis(100)).await;
        }
    }

    #[instrument(skip(self))]
    pub async fn handle_client_request(
        &self,
        app_state: ApplicationState,
        request: GenericAppStateRequest,
    ) -> Result<ApplicationState> {
        debug!(
            "Action execution. Request {:?}, state: {:?}",
            &request, &app_state
        );

        match &request {
            GenericAppStateRequest::GetState => {
                //skip - nothing to do
            }
            GenericAppStateRequest::GenerateUserCreds(_) => {
                self.get_user_creds(&request).await?;
            }
            GenericAppStateRequest::SignUp(_) => {
                info!("Handle sign up request");

                let user_creds = self.get_user_creds(&request).await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

                match &app_state {
                    ApplicationState::Local(_) => {
                        self.sign_up(&user_creds).await?;
                    }
                    ApplicationState::Vault(VaultFullInfo::Member(member_info)) => {
                        let vault = &member_info.member.vault;
                        bail!("You are already a vault member: {:?}", vault);
                    }
                    ApplicationState::Vault(VaultFullInfo::NotExists(_)) => {
                        self.sign_up(&user_creds).await?;
                    }
                    ApplicationState::Vault(VaultFullInfo::Outsider(outsider)) => {
                        info!("Handle outsider sign up request");

                        match outsider.status {
                            UserDataOutsiderStatus::NonMember => {
                                info!("Handle outsider NON_MEMBER sign up request");
                                self.sign_up(&user_creds).await?;
                            }
                            UserDataOutsiderStatus::Pending => {
                                bail!("Your request is already in pending status")
                            }
                            UserDataOutsiderStatus::Declined => {
                                bail!("Device has been declined")
                            }
                        }
                    }
                }

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }

            GenericAppStateRequest::ClusterDistribution(plain_request) => {
                let user_creds = self.get_user_creds(&request).await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

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
                        let vault = p_vault
                            .get_vault(member.user().vault_name())
                            .await?
                            .to_data();
                        let vault_member = VaultMember { member, vault };
                        let distributor = MetaDistributor {
                            p_obj: self.p_obj.clone(),
                            vault_member: vault_member.clone(),
                            user_creds: Arc::new(user_creds.clone()),
                        };

                        let secure_request = SecurePassInfo::from(plain_request.clone());
                        distributor.distribute(vault_member, secure_request).await?;
                    }
                }

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }

            GenericAppStateRequest::Recover(meta_pass_id) => {
                let user_creds = self.get_user_creds(&request).await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;

                let recovery_action = RecoveryAction::from(self.p_obj.clone());
                recovery_action
                    .recovery_request(user_creds.clone(), meta_pass_id.clone())
                    .await?;

                self.sync_gateway.sync(user_creds.user()).await?;
                self.sync_gateway.sync(user_creds.user()).await?;
            }
        }

        //Update app state
        let app_state = self.get_app_state().await?;
        Ok(app_state)
    }

    async fn sign_up(&self, user_creds: &UserCredentials) -> Result<()> {
        let user_data = UserData {
            vault_name: user_creds.vault_name.clone(),
            device: user_creds.device(),
        };

        let sign_up_claim = SignUpClaim::from(self.p_obj.clone());
        sign_up_claim.sign_up(user_data.clone()).await?;

        Ok(())
    }

    async fn get_user_creds(&self, request: &GenericAppStateRequest) -> Result<UserCredentials> {
        let creds_repo = PersistentCredentials::from(self.p_obj.clone());

        let user_creds = match &request {
            GenericAppStateRequest::GetState => {
                bail!("Invalid state. GetState request!")
            }
            GenericAppStateRequest::GenerateUserCreds(vault_name) => {
                let device_name = self.device_creds.device.device_name.clone();
                creds_repo
                    .get_or_generate_user_creds(device_name, vault_name.clone())
                    .await?
            }
            GenericAppStateRequest::SignUp(vault_name) => {
                let device_name = self.device_creds.device.device_name.clone();
                creds_repo
                    .get_or_generate_user_creds(device_name, vault_name.clone())
                    .await?
            }
            GenericAppStateRequest::ClusterDistribution(_) => self.find_user_creds().await?,
            GenericAppStateRequest::Recover(_) => self.find_user_creds().await?,
        };
        Ok(user_creds)
    }

    pub async fn find_user_creds(&self) -> Result<UserCredentials> {
        let creds_repo = PersistentCredentials::from(self.p_obj.clone());
        let user_creds = {
            let maybe_user_creds = creds_repo.get_user_creds().await?;

            let Some(user_creds) = maybe_user_creds else {
                bail!("Invalid state. UserCredentials must be present")
            };

            user_creds
        };

        Ok(user_creds)
    }

    pub async fn build_service_state(&self) -> Result<ServiceState<ApplicationState>> {
        let app_state = self.get_app_state().await?;
        let service_state = ServiceState { app_state };

        //self.state_provider.push(&service_state.app_state).await?;
        Ok(service_state)
    }

    pub async fn get_app_state(&self) -> Result<ApplicationState> {
        let creds_repo = PersistentCredentials::from(self.p_obj());
        let maybe_user_creds = creds_repo.get_user_creds().await?;

        let app_state = match maybe_user_creds {
            None => ApplicationState::Local(self.device_creds.device.clone()),
            Some(user_creds) => {
                self.sync_gateway.sync(user_creds.user()).await?;

                let p_vault = PersistentVault::from(self.p_obj());
                let vault_status = p_vault.find(user_creds.user()).await?;

                match vault_status {
                    VaultStatus::NotExists(user) => {
                        ApplicationState::Vault(VaultFullInfo::NotExists(user))
                    }
                    VaultStatus::Outsider(outsider) => {
                        ApplicationState::Vault(VaultFullInfo::Outsider(outsider))
                    }
                    VaultStatus::Member(member_user) => {
                        let vault = p_vault
                            .get_vault(member_user.user_data.vault_name())
                            .await?;

                        let ss_claims = {
                            let p_ss = PersistentSharedSecret::from(self.p_obj());
                            p_ss.get_ss_log_obj(user_creds.vault_name).await?
                        };

                        let maybe_vault_log_event = {
                            let vault_name = member_user.user_data.vault_name();
                            p_vault.vault_log(vault_name).await?
                        };

                        let vault_action_events = maybe_vault_log_event
                            .map(|obj| obj.0.value)
                            .unwrap_or(VaultActionEvents::default());

                        let user_full_info = UserMemberFullInfo {
                            member: VaultMember {
                                member: member_user,
                                vault: vault.to_data(),
                            },
                            ss_claims,
                            vault_events: vault_action_events,
                        };
                        ApplicationState::Vault(VaultFullInfo::Member(Box::from(user_full_info)))
                    }
                }
            }
        };
        Ok(app_state)
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) -> Result<ApplicationState> {
        let resp = self
            .data_transfer
            .dt
            .send_to_service_and_get(request)
            .await?;
        match resp {
            GenericAppStateResponse::AppState(app_state) => Ok(app_state),
        }
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        match &self.get_app_state().await? {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist")
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider")
                }
                VaultFullInfo::Member(_) => {
                    let user_creds = self.find_user_creds().await?;

                    let orchestrator = MetaOrchestrator {
                        p_obj: self.sync_gateway.p_obj.clone(),
                        user_creds,
                    };

                    orchestrator.accept_recover(claim_id).await?;
                    Ok(())
                }
            },
        }
    }

    pub async fn update_membership(
        &self,
        join_request: JoinClusterEvent,
        upd: JoinActionUpdate,
    ) -> Result<()> {
        match self.get_app_state().await? {
            ApplicationState::Local(_) => {
                bail!("Invalid state. Local App State")
            }
            ApplicationState::Vault(vault_info) => match vault_info {
                VaultFullInfo::NotExists(_) => {
                    bail!("Invalid state. Vault doesn't exist")
                }
                VaultFullInfo::Outsider(_) => {
                    bail!("Invalid state. User is outsider")
                }
                VaultFullInfo::Member(_) => {
                    let user_creds = self.find_user_creds().await?;

                    let orchestrator = MetaOrchestrator {
                        p_obj: self.sync_gateway.p_obj.clone(),
                        user_creds,
                    };

                    orchestrator.update_membership(join_request, upd).await?;
                    Ok(())
                }
            },
        }
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.sync_gateway.p_obj.clone()
    }
}

pub struct MetaClientAccessProxy {
    pub dt: Arc<MetaClientDataTransfer>,
}

pub struct MetaClientStateProvider {
    sender: Sender<ApplicationState>,
    receiver: Receiver<ApplicationState>,
}

impl MetaClientStateProvider {
    pub fn new() -> Self {
        let (sender, receiver) = flume::bounded(1);
        Self { sender, receiver }
    }

    pub async fn push(&self, state: &ApplicationState) -> Result<()> {
        self.sender.send_async(state.clone()).await?;
        Ok(())
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::app::meta_app::meta_client_service::{
        MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
    };
    use crate::node::app::sync::sync_gateway::fixture::SyncGatewayFixture;
    use crate::node::app::sync::sync_protocol::SyncProtocol;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use std::sync::Arc;

    pub struct MetaClientServiceFixture<Sync: SyncProtocol> {
        pub client: Arc<MetaClientService<InMemKvLogEventRepo, Sync>>,
        pub vd: Arc<MetaClientService<InMemKvLogEventRepo, Sync>>,

        pub state_provider: MetaClientStateProviderFixture,
        pub data_transfer: MetaClientDataTransferFixture,

        pub sync_gateway: SyncGatewayFixture<Sync>,
    }

    impl<Sync: SyncProtocol> MetaClientServiceFixture<Sync> {
        pub fn from(base: &BaseState, sync: Arc<Sync>) -> Self {
            let state_provider = MetaClientStateProviderFixture::generate();
            let dt_fxr = MetaClientDataTransferFixture::generate();

            let sync_gateway = SyncGatewayFixture::from(&base.empty, sync);

            let client = Arc::new(MetaClientService {
                data_transfer: dt_fxr.client.clone(),
                sync_gateway: sync_gateway.client_gw.clone(),
                state_provider: state_provider.client.clone(),
                p_obj: sync_gateway.client_gw.p_obj.clone(),
                device_creds: Arc::new(base.empty.device_creds.client.clone()),
            });

            let vd = Arc::new(MetaClientService {
                data_transfer: dt_fxr.vd.clone(),
                sync_gateway: sync_gateway.vd_gw.clone(),
                state_provider: state_provider.vd.clone(),
                p_obj: sync_gateway.vd_gw.p_obj.clone(),
                device_creds: Arc::new(base.empty.device_creds.vd.clone()),
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
