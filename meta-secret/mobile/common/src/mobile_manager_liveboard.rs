//! Thin native wrappers share the existing runtime, FFI lock and Core executor.
use super::{FFI_CALL_LOCK, GLOBAL_APP_MANAGER, MobileApplicationManager, RUNTIME};
use anyhow::{Result, anyhow};
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::common::model::liveboard::LiveboardOperationReceipt;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use std::{future::Future, sync::Arc};

impl MobileApplicationManager {
    pub(crate) fn liveboard_instance() -> Result<Option<Arc<Self>>> {
        GLOBAL_APP_MANAGER
            .lock()
            .map(|global| global.clone())
            .map_err(|_| anyhow!("OperationUnavailable"))
    }

    pub(crate) fn run_liveboard<F: Future<Output = Result<LiveboardOperationReceipt>>>(
        future: F,
    ) -> Result<LiveboardOperationReceipt> {
        let _owner = FFI_CALL_LOCK
            .lock()
            .map_err(|_| anyhow!("OperationUnavailable"))?;
        RUNTIME.block_on(future)
    }

    pub async fn cluster_distribution_checked(
        &self,
        input: PlainPassInfo,
    ) -> Result<LiveboardOperationReceipt> {
        self.app_manager.cluster_distribution_checked(input).await
    }

    pub async fn recover_checked(&self, identifier: &str) -> Result<LiveboardOperationReceipt> {
        let pass: MetaPasswordId = self.app_manager.resolve_secret(identifier).await?;
        self.app_manager.recover_checked(pass).await
    }

    pub async fn decide_join_request(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        self.app_manager.decide_join_request(input).await
    }

    pub async fn decide_remove_member(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        self.app_manager.decide_remove_member(input).await
    }
}
