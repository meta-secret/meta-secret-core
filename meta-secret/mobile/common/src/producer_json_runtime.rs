//! Shared existing native runtime; no alternate service or synthetic receipt.
use crate::mobile_manager::MobileApplicationManager;
use anyhow::Result;
use meta_secret_core::node::common::model::liveboard_producer::{
    LiveboardProducerReceipt, ProducerReference,
};
use std::{future::Future, sync::Arc};

pub(crate) fn invoke<F, Fut>(operation: F) -> String
where
    F: FnOnce(Arc<MobileApplicationManager>) -> Fut,
    Fut: Future<Output = Result<LiveboardProducerReceipt>>,
{
    let manager = match MobileApplicationManager::liveboard_instance() {
        Ok(Some(manager)) => manager,
        Ok(None) => return error("NotInitialized"),
        Err(_) => return error("OperationUnavailable"),
    };
    match MobileApplicationManager::run_producer(operation(manager)) {
        Ok(receipt) => serde_json::json!({"success":true,"message":receipt}).to_string(),
        Err(_) => error("OperationUnavailable"),
    }
}

pub(crate) fn parse(input: &str) -> Result<ProducerReference, String> {
    ProducerReference::parse_json(input).map_err(|_| error("InvalidProducerReference"))
}

fn error(code: &'static str) -> String {
    serde_json::json!({"success":false,"errorCode":code}).to_string()
}
