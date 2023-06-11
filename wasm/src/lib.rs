use meta_secret_core::models::{
    FindSharesRequest, JoinRequest, MembershipRequestType, SecretDistributionType, UserSignature,
};
use meta_secret_core::node::server_api;
use meta_secret_core::recover_from_shares;
use meta_secret_core::sdk::api::MessageType;
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret::shared_secret::{
    PlainText, SharedSecretEncryption, UserShareDto,
};
use meta_secret_core::shared_secret::MetaDistributor;
use wasm_bindgen::prelude::*;
use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand, UserPasswordEntity};

use crate::db::meta_pass;

mod commit_log;
mod db;
mod security;
mod utils;

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
    log("wasm: get vault!");

    let maybe_creds = security::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            let user_sig = creds.user_sig;
            let vault = server_api::get_vault(&user_sig)
                .await
                .map_err(JsError::from)?;

            let vault_js = serde_wasm_bindgen::to_value(&vault)?;
            Ok(vault_js)
        }
        None => Err(JsValue::from("Empty user credentials")),
    }
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
    let maybe_creds = security::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            let find_shares_request = FindSharesRequest {
                user_request_type: SecretDistributionType::Split,
                user_signature: creds.user_sig,
            };

            let shares_response = server_api::find_shares(&find_shares_request)
                .await
                .map_err(JsError::from)?;

            match shares_response.msg_type {
                MessageType::Ok => {
                    let shares_result = shares_response.data.unwrap();
                    for share in shares_result.shares {
                        match share.distribution_type {
                            SecretDistributionType::Split => {
                                log("wasm, sync: split");

                                let user_passes_repo = meta_pass::UserPasswordsWasmRepo {};

                                let pass_id = &share.meta_password.meta_password.id.id;
                                let maybe_user_pass: Option<UserPasswordEntity> = user_passes_repo
                                    .find_one(pass_id.as_str())
                                    .await
                                    .map_err(JsError::from)?;

                                let user_pass_entity = match maybe_user_pass {
                                    Some(mut user_pass) => {
                                        user_pass.shares.push(share.clone());
                                        user_pass
                                    }
                                    None => UserPasswordEntity {
                                        meta_pass_id: *share.meta_password.meta_password.id.clone(),
                                        shares: vec![share.clone()],
                                    },
                                };

                                alert("Save user pass!!!");
                                user_passes_repo
                                    .save(pass_id.as_str(), &user_pass_entity)
                                    .await
                                    .map_err(JsError::from)?;
                            }
                            SecretDistributionType::Recover => {
                                //restore password
                                log("wasm, sync: recover");
                            }
                        }
                    }
                }
                MessageType::Err => {
                    let err_js =
                        serde_wasm_bindgen::to_value(&shares_response.err.unwrap()).unwrap();
                    log(format!("wasm, sync: error: {:?}", err_js).as_str());
                    //Err(err_js);
                }
            }

            log("wasm, sync: save to db");

            //save shares to db
            Ok(JsValue::null())
        }
        None => Err(JsValue::from("User credentials not found")),
    }
}

#[wasm_bindgen]
pub async fn cluster_distribution(pass_id: &str, pass: &str) -> Result<JsValue, JsValue> {
    log("wasm: cluster distribution!!!!");

    let maybe_creds = security::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            let user_sig = creds.user_sig;
            let vault_response = server_api::get_vault(&user_sig)
                .await
                .map_err(JsError::from)?;

            let maybe_vault = vault_response.data;

            match maybe_vault {
                None => Err(JsValue::from("Empty vault response")),
                Some(vault_info_data) => match vault_info_data.vault {
                    None => Err(JsValue::from("Vault not found")),
                    Some(vault) => {
                        let distributor = MetaDistributor {
                            security_box: *creds.security_box,
                            user_sig: *user_sig,
                            vault: *vault,
                        };

                        distributor
                            .distribute(pass_id.to_string(), pass.to_string())
                            .await;
                        Ok(JsValue::from_str("Password has been created"))
                    }
                },
            }
        }
        None => Err(JsValue::from("Empty user credentials")),
    }
}

#[wasm_bindgen]
pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    let candidate: UserSignature = serde_wasm_bindgen::from_value(candidate_user_sig)?;
    let request_type: MembershipRequestType = serde_wasm_bindgen::from_value(request_type)?;

    let log_msg = format!(
        "wasm: membership request. type: {:?}, candidate: {:?}",
        request_type, candidate
    );
    log(log_msg.as_str());

    let maybe_user_creds = security::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_user_creds {
        Some(user_creds) => {
            let join_request = JoinRequest {
                member: user_creds.user_sig,
                candidate: Box::new(candidate),
            };

            let secrets = match request_type {
                MembershipRequestType::Accept => server_api::accept(&join_request).await.unwrap(),
                MembershipRequestType::Decline => server_api::decline(&join_request).await.unwrap(),
            };

            let secrets_js = serde_wasm_bindgen::to_value(&secrets)?;
            Ok(secrets_js)
        }
        None => Err(JsValue::from("Empty user credentials")),
    }
}

#[wasm_bindgen]
pub async fn get_meta_passwords() -> Result<JsValue, JsValue> {
    let maybe_creds = security::internal::find_user_credentials()
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
}

#[wasm_bindgen]
pub async fn register() -> Result<JsValue, JsValue> {
    let maybe_creds = security::internal::find_user_credentials()
        .await
        .map_err(JsError::from)?;

    match maybe_creds {
        Some(creds) => {
            let user_sig = creds.user_sig;
            let register_response = server_api::register(&user_sig)
                .await
                .map_err(JsError::from)?;

            let register_js = serde_wasm_bindgen::to_value(&register_response)?;
            Ok(register_js)
        }
        None => Err(JsValue::from("User credentials not found")),
    }
}

/// https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
#[wasm_bindgen]
pub fn split(pass: &str) -> Result<JsValue, JsValue> {
    let plain_text = PlainText::from(pass);
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
    Ok(shares_js)
}

#[wasm_bindgen]
pub fn restore_password(shares_json: JsValue) -> Result<JsValue, JsValue> {
    log("wasm: restore password, core functionality");

    let user_shares: Vec<UserShareDto> = serde_wasm_bindgen::from_value(shares_json)?;

    let plain_text = recover_from_shares(user_shares).map_err(JsError::from)?;
    Ok(JsValue::from_str(plain_text.text.as_str()))
}
