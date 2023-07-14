use serde::Serialize;
use wasm_bindgen::prelude::*;

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::{MetaVault, UserCredentials};
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};

use crate::{alert, log};
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

pub async fn create_meta_vault(vault_name: &str, device_name: &str) -> Result<JsValue, JsValue> {
    log("wasm::create_meta_vault: create a meta vault");

    let logger = WasmMetaLogger {};
    let repo = WasmRepo::default();

    let meta_vault = repo
        .create_meta_vault(vault_name.to_string(), device_name.to_string(), &logger)
        .await
        .map_err(JsError::from)?;

    let meta_vault_js = meta_vault.to_js()?;

    Ok(meta_vault_js)
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

pub async fn generate_user_credentials() -> Result<(), JsValue> {
    log("wasm::generate_user_credentials: generate a new security box");

    let logger = WasmMetaLogger {};

    let repo = WasmRepo::default();
    let maybe_meta_vault: Option<MetaVault> = repo
        .find_meta_vault(&logger)
        .await
        .map_err(JsError::from)?;

    match maybe_meta_vault {
        Some(meta_vault) => {
            let security_box = KeyManager::generate_security_box(meta_vault.name);
            let user_sig = security_box.get_user_sig(&meta_vault.device);
            let creds = UserCredentials::new(security_box, user_sig);
            repo
                .save_user_creds(creds)
                .await
                .map_err(JsError::from)?;

            Ok(())
        }
        None => {
            let err_msg = "The parameters have not yet set for the vault. Empty meta vault";
            log(format!("wasm::generate_user_credentials: error: {:?}", err_msg).as_str());
            let err = JsValue::from(err_msg);

            Err(err)
        }
    }
}
