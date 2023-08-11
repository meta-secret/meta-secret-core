use serde::Serialize;
use wasm_bindgen::prelude::*;

use meta_secret_core::node::app::meta_app::MetaVaultManager;

use crate::log;
use crate::commit_log::{WasmMetaLogger, WasmRepo};

pub async fn get_meta_vault() -> Result<Option<JsValue>, JsValue> {
    log("wasm::get_meta_vault: get meta vault");

    let logger = WasmMetaLogger {};

    let repo = WasmRepo::default();
    let maybe_meta_vault = repo
        .find_meta_vault(&logger)
        .await
        .map_err(JsError::from)?;

    match maybe_meta_vault {
        Some(meta_vault) => {
            log("wasm: meta vault has been found");
            let meta_vault_js = meta_vault.to_js()?;
            Ok(Some(meta_vault_js))
        }
        None => {
            log("wasm: meta vault not present");
            Ok(None)
        }
    }
}

pub trait ToJsValue {
    fn to_js(&self) -> Result<JsValue, JsValue>;
}

impl<T: Serialize> ToJsValue for T {
    fn to_js(&self) -> Result<JsValue, JsValue> {
        let js_value: JsValue = serde_wasm_bindgen::to_value(self)?;
        Ok(js_value)
    }
}
