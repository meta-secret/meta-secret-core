use crate::models::ApplicationState;
use crate::node::app_models::UserCredentials;
use crate::node::db::events::common::VaultInfo;

#[derive(Clone, Debug)]
pub enum GenericAppState {
    Empty(EmptyAppState),
    Configured(ConfiguredAppState),
    Joined(JoinedAppState),
}

impl GenericAppState {
    pub fn get_state(&self) -> ApplicationState {
        match self {
            GenericAppState::Empty(EmptyAppState { app_state }) => app_state.clone(),
            GenericAppState::Configured(ConfiguredAppState { app_state, .. }) => app_state.clone(),
            GenericAppState::Joined(JoinedAppState { app_state, .. }) => app_state.clone(),
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
                meta_vault: None,
                vault: None,
                meta_passwords: vec![],
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
pub struct JoinedAppState {
    pub app_state: ApplicationState,
    pub creds: UserCredentials,
    pub vault_info: VaultInfo,
}
