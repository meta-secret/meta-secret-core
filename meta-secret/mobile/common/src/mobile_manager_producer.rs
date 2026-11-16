//! Native wrappers resolve exact IDs and forward to the existing typed service.
use super::MobileApplicationManager;
use anyhow::Result;
use meta_secret_core::node::common::model::liveboard_producer::{
    LiveboardProducerReceipt, ProducerReference,
};

impl MobileApplicationManager {
    pub async fn delete_secret_checked(
        &self,
        identifier: &str,
    ) -> Result<LiveboardProducerReceipt> {
        let pass = self
            .app_manager
            .meta_client_service
            .resolve_producer_secret(identifier)
            .await?;
        self.app_manager.delete_secret_checked(pass).await
    }
    pub async fn remove_member_checked(
        &self,
        target_device_id: &str,
    ) -> Result<LiveboardProducerReceipt> {
        self.app_manager
            .remove_member_checked(target_device_id.to_owned())
            .await
    }
    pub async fn reconcile_producer_receipt(
        &self,
        reference: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        self.app_manager.reconcile_producer_receipt(reference).await
    }
    pub async fn continue_local_remove_checked(
        &self,
        reference: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        self.app_manager
            .continue_local_remove_checked(reference)
            .await
    }
}

#[cfg(test)]
#[path = "producer_native_tests.rs"]
mod tests;
