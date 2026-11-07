//! Additive native JSON ports; operation receipts never imply canonical completion.
use crate::mobile_manager::MobileApplicationManager;
use anyhow::Result;
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::common::model::liveboard::LiveboardOperationReceipt;
use meta_secret_core::node::common::model::meta_pass::PlainPassInfo;
use serde_json::json;
use std::{future::Future, sync::Arc};

pub fn cluster_distribution_checked(name: String, secret: String) -> String {
    invoke(|manager| async move {
        manager
            .cluster_distribution_checked(PlainPassInfo::new(name, secret))
            .await
    })
}

pub fn recover_checked(secret_id: String) -> String {
    invoke(|manager| async move { manager.recover_checked(&secret_id).await })
}

pub fn decide_join_request(request_json: String) -> String {
    let Ok(input) = CanonicalDecisionInput::parse_json(&request_json) else {
        return error("InvalidDecisionInput");
    };
    invoke(|manager| async move { manager.decide_join_request(input).await })
}

pub fn decide_remove_member(request_json: String) -> String {
    let Ok(input) = CanonicalDecisionInput::parse_json(&request_json) else {
        return error("InvalidDecisionInput");
    };
    invoke(|manager| async move { manager.decide_remove_member(input).await })
}

fn invoke<F, Fut>(operation: F) -> String
where
    F: FnOnce(Arc<MobileApplicationManager>) -> Fut,
    Fut: Future<Output = Result<LiveboardOperationReceipt>>,
{
    let manager = match MobileApplicationManager::liveboard_instance() {
        Ok(Some(manager)) => manager,
        Ok(None) => return error("NotInitialized"),
        Err(_) => return error("OperationUnavailable"),
    };
    match MobileApplicationManager::run_liveboard(operation(manager)) {
        Ok(receipt) => json!({"success": true, "message": receipt}).to_string(),
        Err(_) => error("OperationUnavailable"),
    }
}

fn error(code: &'static str) -> String {
    json!({"success": false, "errorCode": code}).to_string()
}
