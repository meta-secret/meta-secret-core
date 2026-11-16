//! Typed producers enqueue one envelope through the existing Core FIFO.
use crate::app_manager::ApplicationManager;
use anyhow::Result;
use meta_secret_core::node::{
    app::sync::sync_protocol::SyncProtocol,
    common::model::{
        liveboard_producer::{LiveboardProducerReceipt, ProducerReference},
        meta_pass::MetaPasswordId,
    },
    db::repo::generic_db::KvLogEventRepo,
};

impl<R: KvLogEventRepo + Send + Sync + 'static, S: SyncProtocol + Send + Sync + 'static>
    ApplicationManager<R, S>
{
    pub async fn delete_secret_checked(
        &self,
        pass: MetaPasswordId,
    ) -> Result<LiveboardProducerReceipt> {
        self.meta_client_service.send_producer_request(meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest::DeleteSecretChecked(pass)).await
    }
    pub async fn remove_member_checked(
        &self,
        target_device_id: String,
    ) -> Result<LiveboardProducerReceipt> {
        self.meta_client_service.send_producer_request(meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest::RemoveMemberChecked(target_device_id)).await
    }
    pub async fn reconcile_producer_receipt(
        &self,
        reference: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        self.meta_client_service.send_producer_request(meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest::ReconcileProducerReceipt(reference)).await
    }
    pub async fn continue_local_remove_checked(
        &self,
        reference: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        self.meta_client_service.send_producer_request(meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest::ContinueLocalRemoveChecked(reference)).await
    }
}
