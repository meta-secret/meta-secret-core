use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::{alert, log};
use crate::objects::ToJsValue;
use meta_secret_core::models::{UserCredentials, VaultInfoData, VaultInfoStatus};
use meta_secret_core::node::app::meta_app::UserCredentialsManager;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::meta_db::MetaDb;
use meta_secret_core::node::server::data_sync::{DataSync, MetaServerContextState};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use std::marker::PhantomData;
use std::rc::Rc;
use wasm_bindgen::{JsError, JsValue};
use meta_secret_core::node::db::events::sign_up::SignUpRequest;
use meta_secret_core::node::db::generic_db::SaveCommand;
use crate::db::WasmDbError;
use wasm_bindgen::prelude::*;
use meta_secret_core::node::db::events::object_id::ObjectId;
use crate::server::WasmMetaServer;

pub fn get_data_sync(repo: Rc<WasmRepo>, creds: &UserCredentials) -> DataSync<WasmRepo, WasmDbError> {
    let persistent_object = get_persistent_object(repo.clone());
    let persistent_object_rc = Rc::new(persistent_object);

    let meta_db_manager = MetaDbManager {
        persistent_obj: persistent_object_rc.clone(),
        repo: repo.clone()
    };
    let meta_db_manager_rc = Rc::new(meta_db_manager);

    DataSync {
        persistent_obj: persistent_object_rc,
        repo,
        context: Rc::new(MetaServerContextState::from(creds)),
        meta_db_manager: meta_db_manager_rc,
    }
}

fn get_persistent_object(repo: Rc<WasmRepo>) -> PersistentObject<WasmRepo, WasmDbError> {
    let persistent_object = PersistentObject {
        repo: repo.clone(),
        global_index: PersistentGlobalIndex {
            repo,
            _phantom: PhantomData,
        },
    };
    persistent_object
}

/// Sync local commit log with server
pub async fn sync_shares() -> Result<JsValue, JsValue> {
    /*
    let maybe_creds = objects::internal::find_user_credentials()
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

     */
    Ok(JsValue::null())
}

pub async fn cluster_distribution(pass_id: &str, pass: &str) -> Result<JsValue, JsValue> {
    /*
    log("wasm: cluster distribution!!!!");

    let maybe_creds = objects::internal::find_user_credentials()
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

     */
    Ok(JsValue::null())
}

pub async fn membership(
    candidate_user_sig: JsValue,
    request_type: JsValue,
) -> Result<JsValue, JsValue> {
    /*
    let candidate: UserSignature = serde_wasm_bindgen::from_value(candidate_user_sig)?;
    let request_type: MembershipRequestType = serde_wasm_bindgen::from_value(request_type)?;

    let log_msg = format!(
        "wasm: membership request. type: {:?}, candidate: {:?}",
        request_type, candidate
    );
    log(log_msg.as_str());

    let maybe_user_creds = objects::internal::find_user_credentials()
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
     */
    Ok(JsValue::null())
}

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
pub struct WasmMetaClient {
    meta_db: MetaDb,
    meta_db_manager: MetaDbManager<WasmRepo, WasmDbError>
}

#[wasm_bindgen]
impl WasmMetaClient {
    pub fn new() -> Self {
        let repo = WasmRepo::default();
        let repo_rc = Rc::new(repo);
        let persistent_object = get_persistent_object(repo_rc.clone());
        let persistent_object_rc = Rc::new(persistent_object);

        let meta_db_manager = MetaDbManager {
            persistent_obj: persistent_object_rc.clone(),
            repo: repo_rc,
        };

        let meta_db = MetaDb::default();

        WasmMetaClient {
            meta_db,
            meta_db_manager
        }
    }

    pub async fn get_vault(mut self) -> Result<JsValue, JsValue> {
        log("wasm: get vault!");

        let logger = WasmMetaLogger {};
        WasmMetaServer::new().run_server().await;

        let repo = WasmRepo::default();

        let maybe_creds = repo.find_user_creds()
            .await
            .map_err(JsError::from)?;

        let vault_info = match maybe_creds {
            Some(creds) => {
                let vault_obj = ObjectId::vault_unit(creds.user_sig.vault.name.as_str());
                let vault_id = self.meta_db.vault_store.tail_id.unwrap_or(vault_obj);

                self.meta_db.vault_store.tail_id = Some(vault_id.clone());
                self.meta_db = self.meta_db_manager.sync_meta_db(self.meta_db, &logger).await;

                let global_index = &self.meta_db.global_index_store.global_index;
                if global_index.contains(vault_id.unit_id().id_str().as_str()) {
                    //if the vault is already present:
                    match self.meta_db.vault_store.vault.as_ref() {
                        None => {
                            VaultInfoData::empty(VaultInfoStatus::Unknown)
                        },
                        Some(vault_doc) => VaultInfoData {
                            vault_info: Some(VaultInfoStatus::Member),
                            vault: Some(Box::new(vault_doc.clone())),
                        },
                    }
                } else {
                    VaultInfoData::empty(VaultInfoStatus::NotFound)
                }
            }
            None => panic!("Empty user credentials"),
        };

        vault_info.to_js()
    }

    pub async fn register(mut self) -> Result<JsValue, JsValue> {
        let logger = WasmMetaLogger {};
        let repo = WasmRepo::default();

        WasmMetaServer::new().run_server().await;
        self.meta_db = self.meta_db_manager.sync_meta_db(self.meta_db, &logger).await;
        log(format!("meta db: {:?}", self.meta_db).as_str());

        let maybe_creds = repo.find_user_creds()
            .await
            .map_err(JsError::from)?;

        match maybe_creds {
            Some(creds) => {
                log("Wasm::register. Sign up");

                //check if vault is already exists in global index
                let vault_name = creds.user_sig.vault.name.clone();
                let vault_id = ObjectId::vault_unit(vault_name.as_str());
                if self.meta_db.global_index_store.global_index.contains(&vault_id.id_str()) {
                    // check if I have a vault?


                    let js_val = serde_wasm_bindgen::to_value(&VaultInfoStatus::Pending)?;
                    Ok(js_val)
                } else {
                    let sign_up_request_factory = SignUpRequest {};
                    let sign_up_request = sign_up_request_factory.generic_request(&creds.user_sig);

                    repo.save_event(&sign_up_request)
                        .await
                        .map_err(JsError::from)?;

                    let js_val = serde_wasm_bindgen::to_value(&VaultInfoStatus::Pending)?;
                    Ok(js_val)
                }
            }
            None => {
                log("Registration error: user credentials not found");
                panic!("Empty user credentials");
            }
        }
    }
}

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