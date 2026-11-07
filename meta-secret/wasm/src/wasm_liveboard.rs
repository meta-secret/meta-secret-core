//! Additive, fallible Liveboard exports. Receipts describe queueing, not completion.
use crate::wasm_app_manager::WasmApplicationManager;
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::common::model::liveboard::LiveboardOperationReceipt;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl WasmApplicationManager {
    pub async fn cluster_distribution_checked(
        &self,
        input: &PlainPassInfo,
    ) -> Result<String, JsValue> {
        receipt_json(
            self.app_manager
                .cluster_distribution_checked(input.clone())
                .await,
        )
    }
    pub async fn recover_checked(&self, pass: &MetaPasswordId) -> Result<String, JsValue> {
        receipt_json(self.app_manager.recover_checked(pass.clone()).await)
    }
    pub async fn decide_join_request(&self, request_json: String) -> Result<String, JsValue> {
        let input = decision_input(&request_json)?;
        receipt_json(self.app_manager.decide_join_request(input).await)
    }
    pub async fn decide_remove_member(&self, request_json: String) -> Result<String, JsValue> {
        let input = decision_input(&request_json)?;
        receipt_json(self.app_manager.decide_remove_member(input).await)
    }
}

fn decision_input(json: &str) -> Result<CanonicalDecisionInput, JsValue> {
    CanonicalDecisionInput::parse_json(json)
        .map_err(|_| JsError::new("InvalidDecisionInput").into())
}

fn receipt_json(result: anyhow::Result<LiveboardOperationReceipt>) -> Result<String, JsValue> {
    let receipt = result.map_err(|_| JsError::new("OperationUnavailable"))?;
    serde_json::to_string(&receipt).map_err(|_| JsError::new("OperationUnavailable").into())
}
