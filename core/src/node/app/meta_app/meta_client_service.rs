use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info, instrument, Instrument};

use crate::node::app::app_state_update_manager::JsAppStateManager;
use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::app_state::{ConfiguredAppState, EmptyAppState, GenericAppState, JoinedAppState};
use crate::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest, GenericAppStateResponse, RecoveryRequest, SignUpRequest,
};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::actor::{ActionHandler, ServiceState};
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::device::DeviceCredentials;
use crate::node::db::actions::recover::RecoveryAction;
use crate::node::db::generic_db::KvLogEventRepo;

pub struct MetaClientService<Repo: KvLogEventRepo, StateManager: JsAppStateManager> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub meta_client: Arc<MetaClient<Repo>>,
    pub state_manager: Arc<StateManager>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
    pub device_creds: DeviceCredentials,
}

pub struct MetaClientDataTransfer {
    pub dt: MpscDataTransfer<GenericAppStateRequest, GenericAppStateResponse>,
}

/// SignUp handler
#[async_trait(? Send)]
impl<Repo, StateManager> ActionHandler<SignUpRequest, GenericAppState> for MetaClientService<Repo, StateManager>
where
    Repo: KvLogEventRepo,
    StateManager: JsAppStateManager,
{
    #[instrument(skip_all)]
    async fn handle(&self, request: SignUpRequest, state: &mut ServiceState<GenericAppState>) {
        info!("Handle sign up request");

        match &state.state {
            GenericAppState::Empty(EmptyAppState { app_state }) => {
                self.sync_gateway.sync().in_current_span().await;
                let new_app_state = self.update_app_state(app_state, &self.user_creds).await;

                let new_generic_app_state = ConfiguredAppState {
                    app_state: new_app_state,
                    creds: self.user_creds.clone(),
                };

                state.state = GenericAppState::Configured(new_generic_app_state);
            }
            GenericAppState::Configured(configured) => {
                let joined_app_state = self.meta_client.sign_up(configured).in_current_span().await;
                state.state = GenericAppState::Joined(joined_app_state);
            }
            GenericAppState::Joined(_) => {
                error!("ignore sign up requests (device has been already joined");
            }
        }
    }
}

impl<Repo, StateManager> MetaClientService<Repo, StateManager>
where
    Repo: KvLogEventRepo,
    StateManager: JsAppStateManager,
{
    async fn update_app_state(&self, app_state: &ApplicationState, creds: DeviceCredentials) -> ApplicationState {
        let mut new_app_state = app_state.clone();
        new_app_state.device_creds = Some(creds);

        let vault_info = self
            .meta_client
            .get_vault(creds.user_sig.vault.name.clone())
            .in_current_span()
            .await;

        match vault_info {
            VaultInfo::Member { vault } => {
                let vault_name = vault.vault_name.clone();

                new_app_state.vault = Some(vault);

                let meta_pass_store = self
                    .meta_client
                    .read_db_service_proxy
                    .get_meta_pass_store(vault_name)
                    .await
                    .unwrap();
                new_app_state.meta_passwords = meta_pass_store.passwords();
            }
            VaultInfo::Pending => {}
            VaultInfo::Declined => {}
            VaultInfo::NotFound => {}
            VaultInfo::NotMember => {
                new_app_state.join_component = true;
            }
        }

        new_app_state
    }
}

#[async_trait(? Send)]
impl<Repo, StateManager> ActionHandler<RecoveryRequest, GenericAppState> for MetaClientService<Repo, StateManager>
where
    Repo: KvLogEventRepo,
    StateManager: JsAppStateManager,
{
    #[instrument(skip_all)]
    async fn handle(&self, request: RecoveryRequest, state: &mut ServiceState<GenericAppState>) {
        if let GenericAppState::Joined(app_state) = &state.state {
            let recovery_action = RecoveryAction {
                persistent_obj: self.meta_client.persistent_obj.clone(),
            };
            recovery_action
                .recovery_request(request.meta_pass_id, app_state)
                .in_current_span()
                .await;
        } else {
            error!("Invalid request. Recovery request not allowed when the state is not 'Joined'");
        }
    }
}

#[async_trait(? Send)]
impl<Repo, StateManager> ActionHandler<ClusterDistributionRequest, GenericAppState>
    for MetaClientService<Repo, StateManager>
where
    Repo: KvLogEventRepo,

    StateManager: JsAppStateManager,
{
    #[instrument(skip_all)]
    async fn handle(&self, request: ClusterDistributionRequest, state: &mut ServiceState<GenericAppState>) {
        if let GenericAppState::Joined(app_state) = &state.state {
            self.meta_client
                .cluster_distribution(request.pass_id.as_str(), request.pass.as_str(), app_state)
                .await;

            let passwords = {
                let pass_store = self
                    .meta_client
                    .read_db_service_proxy
                    .get_meta_pass_store(app_state.creds.user_sig.vault.name.clone())
                    .await
                    .unwrap();
                match pass_store {
                    MetaPassStore::Store { passwords, .. } => passwords.clone(),
                    _ => {
                        vec![]
                    }
                }
            };

            let mut app_state = state.state.get_state();
            app_state.meta_passwords.clear();
            app_state.meta_passwords = passwords;
        } else {
            error!("Invalid request. Distribution request not allowed if the state is not 'Joined'")
        }
    }
}

impl<Repo, StateManager> MetaClientService<Repo, StateManager>
where
    Repo: KvLogEventRepo,
    StateManager: JsAppStateManager,
{
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run meta_app service");

        let mut service_state = self.build_service_state().await;

        while let Ok(request) = self.data_transfer.dt.service_receive().await {
            info!(
                "Action execution. Request {:?}, state: {:?}",
                &request, &service_state.state
            );

            self.sync_gateway.sync().in_current_span().await?;

            match &mut service_state.state {
                GenericAppState::Empty(_) => {
                    error!("Empty app state");
                }
                GenericAppState::Configured(configured_app_state) => {
                    let new_app_state = self
                        .update_app_state(&configured_app_state.app_state, configured_app_state.creds)
                        .await;

                    configured_app_state.app_state = new_app_state;
                }
                GenericAppState::Joined(joined_app_state) => {
                    let new_app_state = self
                        .update_app_state(&joined_app_state.app_state, joined_app_state.creds)
                        .await;

                    joined_app_state.app_state = new_app_state;
                    joined_app_state.vault_info = self
                        .meta_client
                        .get_vault(joined_app_state.creds.user_sig.vault.name.clone())
                        .await
                }
            }

            match request {
                GenericAppStateRequest::SignUp(sign_up_request) => {
                    self.handle(sign_up_request, &mut service_state).await;
                }

                GenericAppStateRequest::Recover(recovery_request) => {
                    self.handle(recovery_request, &mut service_state).await
                }

                GenericAppStateRequest::ClusterDistribution(request) => {
                    self.handle(request, &mut service_state).await;
                }
            }

            self.on_update(&service_state.state.get_state()).await;
        }
    }

    async fn build_service_state(&self) -> ServiceState<GenericAppState> {
        let mut service_state = ServiceState {
            state: GenericAppState::empty(),
        };

        let maybe_configured_app_state = self.meta_client.find_user_creds(&service_state.state).await;

        let app_state = service_state.state.get_state();

        if let Ok(Some(configured_app_state)) = maybe_configured_app_state {
            let new_app_state = self.update_app_state(&app_state, &configured_app_state.creds).await;

            let vault_info = self
                .meta_client
                .get_vault(configured_app_state.creds.user_sig.vault.name.clone())
                .await;

            if let VaultInfo::Member { vault } = &vault_info {
                let vault_name = vault.vault_name.clone();

                service_state.state = GenericAppState::Joined(JoinedAppState {
                    app_state: new_app_state,
                    creds: configured_app_state.creds,
                    vault_info,
                });

                let meta_pass_store = self
                    .meta_client
                    .read_db_service_proxy
                    .get_meta_pass_store(vault_name)
                    .await
                    .unwrap();

                service_state.state.get_state().meta_passwords = meta_pass_store.passwords();
            } else {
                service_state.state = GenericAppState::Configured(configured_app_state);
            }
        }

        self.on_update(&service_state.state.get_state()).await;
        service_state
    }

    pub async fn on_update(&self, app_state: &ApplicationState) {
        // update app state in the external system (for instance, vue js)
        self.state_manager.update_js_state(app_state.clone()).await
    }

    pub async fn send_request(&self, request: GenericAppStateRequest) {
        self.data_transfer.dt.send_to_service(request).await
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
