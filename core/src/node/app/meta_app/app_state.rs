use crate::node::common::actor::ServiceState;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::user::UserCredentials;
use crate::node::common::model::vault::VaultData;

#[derive(Clone, Debug)]
pub enum GenericAppState {
    Empty(EmptyAppState),
    Configured(ConfiguredAppState),
    Member(MemberAppState),
}

impl GenericAppState {
    pub fn get_state(&self) -> ApplicationState {
        match self {
            GenericAppState::Empty(EmptyAppState { app_state }) => app_state.clone(),
            GenericAppState::Configured(ConfiguredAppState { app_state, .. }) => app_state.clone(),
            GenericAppState::Member(MemberAppState { app_state, .. }) => app_state.clone(),
        }
    }
}

impl GenericAppState {
    pub fn empty() -> Self {
        GenericAppState::Empty(EmptyAppState::default())
    }
}

#[derive(Clone, Debug)]
pub struct EmptyAppState {
    pub app_state: ApplicationState,
}

impl Default for EmptyAppState {
    fn default() -> Self {
        EmptyAppState {
            app_state: ApplicationState {
                device_creds: None,
                vault: None,
                join_component: false,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfiguredAppState {
    pub app_state: ApplicationState,
    pub creds: UserCredentials,
}

#[derive(Clone, Debug)]
pub struct MemberAppState {
    pub app_state: ApplicationState,
    pub creds: UserCredentials,
    pub vault: VaultData,
}

impl From<GenericAppState> for ServiceState<GenericAppState> {
    fn from(state: GenericAppState) -> Self {
        ServiceState { state }
    }
}
