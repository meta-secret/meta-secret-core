use std::sync::Arc;

use async_trait::async_trait;
use tracing::debug;

use crate::node::common::model::ApplicationState;
use crate::node::db::repo::generic_db::KvLogEventRepo;

#[async_trait(? Send)]
pub trait JsAppStateManager {
    async fn update_js_state(&self, new_state: ApplicationState);
}

pub struct NoOpJsAppStateManager {}

#[async_trait(? Send)]
impl JsAppStateManager for NoOpJsAppStateManager {
    async fn update_js_state(&self, new_state: ApplicationState) {
        debug!(
            "NoOp state manager. Update js state: {}",
            serde_json::to_string(&new_state).unwrap()
        );
    }
}

pub struct ApplicationStateManagerConfigurator<Repo, StateManager>
where
    Repo: KvLogEventRepo,
    StateManager: JsAppStateManager,
{
    pub client_repo: Arc<Repo>,
    pub server_repo: Arc<Repo>,
    pub device_repo: Arc<Repo>,

    pub js_app_state: Arc<StateManager>,
    pub vd_js_app_state: Arc<StateManager>,
}
