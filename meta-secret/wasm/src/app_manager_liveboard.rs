//! Checked operations use the existing serial Core service owner.
use crate::app_manager::ApplicationManager;
use anyhow::Result;
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::sync::sync_protocol::SyncProtocol;
use meta_secret_core::node::common::model::{
    liveboard::LiveboardOperationReceipt,
    meta_pass::{MetaPasswordId, PlainPassInfo},
};
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> ApplicationManager<Repo, Sync> {
    pub async fn cluster_distribution_checked(
        &self,
        input: PlainPassInfo,
    ) -> Result<LiveboardOperationReceipt> {
        self.meta_client_service
            .send_checked_request(GenericAppStateRequest::ClusterDistributionChecked(input))
            .await
    }
    pub async fn recover_checked(&self, pass: MetaPasswordId) -> Result<LiveboardOperationReceipt> {
        self.meta_client_service
            .send_checked_request(GenericAppStateRequest::RecoverChecked(pass))
            .await
    }
    pub async fn decide_join_request(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        self.meta_client_service
            .send_checked_request(GenericAppStateRequest::DecideJoinRequest(input))
            .await
    }
    pub async fn decide_remove_member(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        self.meta_client_service
            .send_checked_request(GenericAppStateRequest::DecideRemoveRequest(input))
            .await
    }
}
