use serde::Serialize;
use wasm_bindgen::prelude::*;
use meta_secret_core::node::app::meta_app::MetaVaultManager;
use crate::commit_log::CommitLogWasmRepo;

use crate::log;

#[wasm_bindgen]
pub async fn get_meta_vault() -> Result<Option<JsValue>, JsValue> {
    log("wasm. get meta vault");

    let meta_vault_manager = CommitLogWasmRepo::default();
    let maybe_meta_vault = meta_vault_manager
        .find_meta_vault()
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

#[wasm_bindgen]
pub async fn create_meta_vault(vault_name: &str, device_name: &str) -> Result<JsValue, JsValue> {
    let meta_vault_manager = CommitLogWasmRepo::default();

    let meta_vault = meta_vault_manager
        .create_meta_vault(vault_name.to_string(), device_name.to_string())
        .await
        .map_err(JsError::from)?;

    let meta_vault_js = meta_vault.to_js()?;

    Ok(meta_vault_js)
}

pub trait ToJsValue {
    fn to_js(&self) -> Result<JsValue, JsValue>;
}

impl <T: Serialize> ToJsValue for T {
    fn to_js(&self) -> Result<JsValue, JsValue> {
        let js_value: JsValue = serde_wasm_bindgen::to_value(self)?;
        Ok(js_value)
    }
}

#[wasm_bindgen]
pub async fn generate_user_credentials() -> Result<(), JsValue> {
    log("wasm: generate a new security box");

    /*
    let repo = MetaVaultWasmRepo {

    };

    let maybe_meta_vault = repo.find_meta_vault()
        .await
        .map_err(JsError::from)?;

    match maybe_meta_vault {
        Some(meta_vault) => {
            let security_box = KeyManager::generate_security_box(meta_vault.name);
            let user_sig = security_box.get_user_sig(&meta_vault.device);
            let creds = UserCredentials::new(security_box, user_sig);

            let creds_repo = UserCredentialsWasmRepo {};
            creds_repo
                .save(user_credentials::store_conf::KEY_NAME, &creds)
                .await
                .map_err(JsError::from)?;

            Ok(())
        }
        None => {
            let err_msg =
                JsValue::from("The parameters have not yet set for the vault. Empty meta vault");
            Err(err_msg)
        }
    }
    */
    Ok(())
}

/*pub mod internal {
    use meta_secret_core::models::UserCredentials;

    use crate::db::user_credentials::UserCredentialsWasmRepo;
    use crate::db::WasmDbError;

    pub async fn find_user_credentials() -> Result<Option<UserCredentials>, WasmDbError> {
        let repo = UserCredentialsWasmRepo {};
        repo.find_user_credentials().await
    }
}*/