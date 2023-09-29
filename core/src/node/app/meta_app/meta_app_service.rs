use async_trait::async_trait;
use std::sync::Arc;
use tracing::{error, info, instrument, Instrument};

use crate::models::ApplicationState;
use crate::node::app::app_state_update_manager::JsAppStateManager;
use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::app_state::{ConfiguredAppState, EmptyAppState, GenericAppState, JoinedAppState};
use crate::node::app::meta_app::messaging::{
    ClusterDistributionRequest, GenericAppStateRequest, GenericAppStateResponse, RecoveryRequest, SignUpRequest,
};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::actor::{ActionHandler, ServiceState};
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::events::common::VaultInfo;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;

pub struct MetaClientService<Repo: KvLogEventRepo, StateManager: JsAppStateManager> {
    pub data_transfer: Arc<MetaClientDataTransfer>,
    pub meta_client: Arc<MetaClient<Repo>>,
    pub state_manager: Arc<StateManager>,
    pub sync_gateway: Arc<SyncGateway<Repo>>,
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
        match &state.state {
            GenericAppState::Empty(EmptyAppState { app_state }) => {
                let creds_result = self
                    .meta_client
                    .get_or_create_local_vault(request.vault_name.as_str(), request.device_name.as_str())
                    .in_current_span()
                    .await;

                self.sync_gateway.sync().in_current_span().await;

                if let Ok(creds) = creds_result {
                    let mut new_app_state = app_state.clone();

                    let vault_info = self.meta_client.get_vault(&creds).in_current_span().await;

                    info!("VAULTT infa: {:?}", vault_info);
                    if let VaultInfo::NotMember = vault_info {
                        info!("ACTIVATE JOIIINNN. state: {:?}", state.state);
                        new_app_state.join_component = true;
                    }

                    let new_generic_app_state = ConfiguredAppState {
                        app_state: new_app_state,
                        creds,
                    };

                    state.state = GenericAppState::Configured(new_generic_app_state);
                }
            }
            GenericAppState::Configured(configured) => {
                let vault_info = self.meta_client.get_vault(&configured.creds).in_current_span().await;

                info!("VAULTT infa: {:?}", vault_info);
                if let VaultInfo::NotMember = vault_info {
                    info!("ACTIVATE JOIIINNN. state: {:?}", state.state);
                    //configured.app_state.join_component = true;
                }

                info!("CONFIGURE!!!! {:?}", state.state);
                let joined_app_state = self.meta_client.sign_up(configured).in_current_span().await;
                state.state = GenericAppState::Joined(joined_app_state);
            }
            GenericAppState::Joined(_) => {
                error!("ignore sign up requests (device has been already joined");
            }
        }
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
            self.meta_client
                .recovery_request(request.meta_pass_id, app_state)
                .in_current_span()
                .await;
        } else {
            panic!("Invalid request. Recovery request not allowed if the state is not 'Joined'");
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
                    .meta_db_service_proxy
                    .get_meta_pass_store()
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
            panic!("Invalid request. Distribution request not allowed if the state is not 'Joined'")
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
            info!("Handle: {:?}", &request);

            self.sync_gateway.sync().in_current_span().await;

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
        if let Ok(Some(configured_app_state)) = maybe_configured_app_state {
            let vault_info = self.meta_client.get_vault(&configured_app_state.creds).await;

            if let VaultInfo::Member { .. } = &vault_info {
                service_state.state = GenericAppState::Joined(JoinedAppState {
                    app_state: configured_app_state.app_state,
                    creds: configured_app_state.creds,
                    vault_info,
                });

                let meta_pass_store = self
                    .meta_client
                    .meta_db_service_proxy
                    .get_meta_pass_store()
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
