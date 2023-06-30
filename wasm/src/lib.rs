use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::objects::{get_meta_vault, ToJsValue};
use meta_secret_core::models::{
    FindSharesRequest, JoinRequest, MembershipRequestType, SecretDistributionType, UserSignature,
    VaultDoc, VaultInfoData, VaultInfoStatus,
};
use meta_secret_core::node::app::meta_app::UserCredentialsManager;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand, UserPasswordEntity};
use meta_secret_core::node::db::meta_db::MetaDb;
use meta_secret_core::node::db::models::ObjectDescriptor;
use meta_secret_core::node::server::meta_server::{DataSync, DataSyncApi, MetaServerContextState};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use meta_secret_core::node::server::request::SyncRequest;
use meta_secret_core::node::server_api;
use meta_secret_core::recover_from_shares;
use meta_secret_core::sdk::api::MessageType;
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret::shared_secret::{
    PlainText, SharedSecretEncryption, UserShareDto,
};
use meta_secret_core::shared_secret::MetaDistributor;
use std::marker::PhantomData;
use std::rc;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use meta_secret_core::node::db::events::sign_up::SignUpRequest;
use crate::wasm_app::get_data_sync;

mod commit_log;
mod db;
mod objects;
mod utils;
mod wasm_app;

/// Json utilities https://github.com/rustwasm/wasm-bindgen/blob/main/crates/js-sys/tests/wasm/JSON.rs

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    pub async fn idbGet(db_name: &str, store_name: &str, key: &str) -> JsValue;
    pub async fn idbSave(db_name: &str, store_name: &str, key: &str, value: JsValue);
    pub async fn idbFindAll(db_name: &str, store_name: &str) -> JsValue;
}

#[wasm_bindgen]
pub async fn get_vault() -> Result<JsValue, JsValue> {
    wasm_app::get_vault().await
}

///https://rustwasm.github.io/wasm-bindgen/examples/closures.html
#[wasm_bindgen]
pub async fn recover() -> Result<JsValue, JsValue> {
    log("wasm recover!");

    /*
    server_api::claim_for_password_recovery(&recovery_request)
    */

    Ok(JsValue::null())
}

/// Sync local commit log with server
#[wasm_bindgen]
pub async fn sync() -> Result<JsValue, JsValue> {
    wasm_app::sync_shares().await
}

#[wasm_bindgen]
pub async fn cluster_distribution(pass_id: &str, pass: &str) -> Result<JsValue, JsValue> {
    wasm_app::cluster_distribution(pass_id, pass).await
}

#[wasm_bindgen]
pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    wasm_app::membership(candidate_user_sig, request_type).await
}

#[wasm_bindgen]
pub async fn get_meta_passwords() -> Result<JsValue, JsValue> {
    /*
    let maybe_creds = objects::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            let user_sig = creds.user_sig;
            log("wasm: get meta passwords");
            let secrets = server_api::get_meta_passwords(&user_sig)
                .await
                .map_err(JsError::from)?;

            let secrets_js = serde_wasm_bindgen::to_value(&secrets)?;
            Ok(secrets_js)
        }
        None => Err(JsValue::from("User credentials not found")),
    }

     */
    Ok(JsValue::null())
}

#[wasm_bindgen]
pub async fn register() -> Result<JsValue, JsValue> {
    let logger = WasmMetaLogger {};

    let repo = WasmRepo::default();

    let maybe_creds = repo.find_user_creds().await.map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            log("Wasm::register. Sign up");
            let sign_up_request_factory = SignUpRequest {};
            let sign_up_request = sign_up_request_factory.generic_request(&creds.user_sig);

            let data_sync = get_data_sync(repo, &creds);
            data_sync.send_data(&sign_up_request, &Some(logger)).await;
        }
        None => {
            panic!("Empty user credentials");
        }
    };

    Ok(JsValue::null())
}

/// https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
#[wasm_bindgen]
pub fn split(pass: &str) -> Result<JsValue, JsValue> {
    /*let plain_text = PlainText::from(pass);
    let config = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };
    let shared_secret = SharedSecretEncryption::new(config, &plain_text).map_err(JsError::from)?;

    let mut res: Vec<UserShareDto> = vec![];
    for share_index in 0..config.number_of_shares {
        let share: UserShareDto = shared_secret.get_share(share_index);
        res.push(share);
    }

    let shares_js = serde_wasm_bindgen::to_value(&res)?;
    Ok(shares_js)*/
    Ok(JsValue::null())
}

#[wasm_bindgen]
pub fn restore_password(shares_json: JsValue) -> Result<JsValue, JsValue> {
    /*log("wasm: restore password, core functionality");

    let user_shares: Vec<UserShareDto> = serde_wasm_bindgen::from_value(shares_json)?;

    let plain_text = recover_from_shares(user_shares).map_err(JsError::from)?;
    Ok(JsValue::from_str(plain_text.text.as_str()))*/
    Ok(JsValue::null())
}
