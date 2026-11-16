//! Borrowed WASM producers preserve input ownership and forward typed Core results.
use crate::wasm_app_manager::WasmApplicationManager;
use meta_secret_core::node::common::model::{
    device::common::DeviceId,
    liveboard_producer::{LiveboardProducerReceipt, ProducerReference},
    meta_pass::MetaPasswordId,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl WasmApplicationManager {
    pub async fn delete_secret_checked(&self, pass: &MetaPasswordId) -> Result<String, JsValue> {
        receipt(self.app_manager.delete_secret_checked(pass.clone()).await)
    }
    pub async fn remove_member_checked(&self, target: &DeviceId) -> Result<String, JsValue> {
        receipt(
            self.app_manager
                .remove_member_checked(target.to_string())
                .await,
        )
    }
    pub async fn reconcile_producer_receipt(
        &self,
        reference_json: String,
    ) -> Result<String, JsValue> {
        let reference = parse(&reference_json)?;
        receipt(self.app_manager.reconcile_producer_receipt(reference).await)
    }
    pub async fn continue_local_remove_checked(
        &self,
        reference_json: String,
    ) -> Result<String, JsValue> {
        let reference = parse(&reference_json)?;
        receipt(
            self.app_manager
                .continue_local_remove_checked(reference)
                .await,
        )
    }
}
fn parse(input: &str) -> Result<ProducerReference, JsValue> {
    ProducerReference::parse_json(input)
        .map_err(|_| JsError::new("InvalidProducerReference").into())
}
fn receipt(result: anyhow::Result<LiveboardProducerReceipt>) -> Result<String, JsValue> {
    let receipt = result.map_err(|_| JsError::new("OperationUnavailable"))?;
    serde_json::to_string(&receipt).map_err(|_| JsError::new("OperationUnavailable").into())
}
