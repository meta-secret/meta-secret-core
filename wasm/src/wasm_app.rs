use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use async_mutex::Mutex;
use wasm_bindgen::{JsError, JsValue};

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::{ApplicationState, MetaVault, UserCredentials};
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::events::join::join_cluster_request;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::events::sign_up::SignUpRequest;
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::meta_db::MetaDb;
use meta_secret_core::node::db::models::{
    GenericKvLogEvent, KvKey, KvLogEvent, MempoolObject, ObjectDescriptor, VaultInfo,
};
use meta_secret_core::node::server::data_sync::{DataSync, MetaServerContextState};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use meta_secret_core::shared_secret::MetaDistributor;

use crate::{alert, log};
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::objects::ToJsValue;
use crate::server::WasmMetaServer;

pub fn get_data_sync(repo: Rc<WasmRepo>, creds: &UserCredentials) -> DataSync<WasmRepo, WasmDbError> {
    let persistent_object = get_persistent_object(repo.clone());
    let persistent_object_rc = Rc::new(persistent_object);

    let meta_db_manager = MetaDbManager {
        persistent_obj: persistent_object_rc.clone(),
        repo: repo.clone(),
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

pub struct WasmMetaClient {
    meta_db: Arc<Mutex<MetaDb>>,
    meta_db_manager: MetaDbManager<WasmRepo, WasmDbError>,
    app_state: Arc<Mutex<ApplicationState>>,
}

impl WasmMetaClient {
    pub fn new(app_state: Arc<Mutex<ApplicationState>>) -> Self {
        let repo = WasmRepo::default();
        let repo_rc = Rc::new(repo);
        let persistent_object = get_persistent_object(repo_rc.clone());
        let persistent_object_rc = Rc::new(persistent_object);

        let meta_db_manager = MetaDbManager {
            persistent_obj: persistent_object_rc,
            repo: repo_rc,
        };

        let meta_db = Arc::new(Mutex::new(MetaDb::default()));
        WasmMetaClient {
            meta_db,
            meta_db_manager,
            app_state,
        }
    }

    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        log("wasm: cluster distribution!!!!");

        let repo = WasmRepo::default();
        let repo_rc = Rc::new(repo);

        let creds = repo_rc.find_user_creds()
            .await
            .unwrap()
            .unwrap();

        let vault_info = self.get_vault()
            .await
            .unwrap();

        let persistent_object = get_persistent_object(repo_rc.clone());
        let persistent_object_rc = Rc::new(persistent_object);

        match vault_info {
            VaultInfo::Member { vault } => {
                let distributor = MetaDistributor {
                    meta_db_manager: MetaDbManager {
                        persistent_obj: persistent_object_rc,
                        repo: repo_rc
                    },
                    vault,
                    user_creds: creds,
                };

                distributor
                    .distribute(pass_id.to_string(), pass.to_string())
                    .await;
            }
            VaultInfo::Pending => {}
            VaultInfo::Declined => {}
            VaultInfo::NotFound => {}
            VaultInfo::NotMember => {}
        };
    }

    pub async fn sign_up(&self, vault_name: &str, device_name: &str) {
        let join = {
            let app_state = self.app_state.lock().await;
            app_state.join_component
        };

        if join {
            self.join_cluster().await;
            WasmMetaServer::new().run_server().await;
            let vault_info = self.get_vault().await.unwrap();
            if let VaultInfo::Member { vault } = vault_info {
                {
                    let mut app_state = self.app_state.lock().await;
                    app_state.vault = Some(Box::new(vault))
                }
            }
        } else {
            self.sign_up_action(vault_name, device_name).await;
        }
    }

    async fn sign_up_action(&self, vault_name: &str, device_name: &str) {
        self.create_local_vault(vault_name, device_name)
            .await
            .expect("Error");

        let vault_info_res = self.get_vault().await;

        match vault_info_res {
            Ok(vault_info) => {
                match vault_info {
                    VaultInfo::Member { vault } => {}
                    VaultInfo::Pending => {}
                    VaultInfo::Declined => {}
                    VaultInfo::NotFound => {
                        let reg_res = self
                            .register()
                            .await;

                        match reg_res {
                            Ok(_vault_info) => {
                                log("Successful registration");
                            }
                            Err(_) => {
                                log("Error. Registration failed");
                            }
                        }
                    }
                    VaultInfo::NotMember => {
                        //join!!!
                        {
                            let mut app_state = self.app_state.lock().await;
                            app_state.join_component = true;
                        }
                    }
                }
            }
            Err(_) => {
                log("Error. Can't get vault");
            }
        }
    }

    async fn create_local_vault(&self, vault_name: &str, device_name: &str) -> Result<(), JsValue> {
        self.create_meta_vault(vault_name, device_name).await?;
        self.generate_user_credentials().await
    }

    pub async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> Result<JsValue, JsValue> {
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

    async fn generate_user_credentials(&self) -> Result<(), JsValue> {
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

    pub async fn get_vault(&self) -> Result<VaultInfo, JsValue> {
        log("wasm: get vault!");

        let logger = WasmMetaLogger {};
        WasmMetaServer::new().run_server().await;

        let repo = WasmRepo::default();

        let maybe_creds = repo.find_user_creds()
            .await
            .map_err(JsError::from)?;

        let vault_info = match maybe_creds {
            Some(creds) => {
                let mut meta_db = self.meta_db.lock().await;

                let vault_obj = ObjectId::vault_unit(creds.user_sig.vault.name.as_str());
                let vault_id = meta_db.vault_store.tail_id.clone().unwrap_or(vault_obj);

                meta_db.vault_store.tail_id = Some(vault_id.clone());

                let updated_meta_db = self.meta_db_manager.sync_meta_db(meta_db.clone(), &logger).await;

                meta_db.vault_store = updated_meta_db.vault_store;
                meta_db.global_index_store = updated_meta_db.global_index_store;

                let global_index = meta_db.global_index_store.global_index.clone();
                let vault_obj_id = vault_id.unit_id();
                if global_index.contains(vault_obj_id.id_str().as_str()) {
                    //if the vault is already present:
                    match meta_db.vault_store.vault.as_ref() {
                        None => {
                            VaultInfo::NotMember
                        }
                        Some(vault_doc) => {
                            VaultInfo::Member { vault: vault_doc.clone() }
                        }
                    }
                } else {
                    VaultInfo::NotFound
                }
            }
            None => VaultInfo::NotMember,
        };

        Ok(vault_info)
    }

    async fn join_cluster(&self) -> Result<VaultInfo, JsValue> {
        let repo = WasmRepo::default();

        let maybe_creds = repo.find_user_creds()
            .await
            .map_err(JsError::from)?;

        match maybe_creds {
            Some(creds) => {
                log("Wasm::register. Join");

                let mem_pool_tail_id = self.meta_db_manager.persistent_obj
                    .find_tail_id_by_obj_desc(&ObjectDescriptor::Mempool)
                    .await
                    .unwrap_or(ObjectId::mempool_unit());

                let join_request = GenericKvLogEvent::Mempool(MempoolObject::JoinRequest {
                    event: KvLogEvent {
                        key: KvKey {
                            obj_id: mem_pool_tail_id,
                            obj_desc: ObjectDescriptor::Mempool,
                        },
                        value: creds.user_sig.as_ref().clone(),
                    }
                });

                repo
                    .save_event(&join_request)
                    .await
                    .map_err(JsError::from)?;

                Ok(VaultInfo::Pending)
            }
            None => {
                log("Registration error: user credentials not found");
                panic!("Empty user credentials");
            }
        }
    }

    async fn register(&self) -> Result<VaultInfo, JsValue> {
        let repo = WasmRepo::default();

        let maybe_creds = repo.find_user_creds()
            .await
            .map_err(JsError::from)?;

        match maybe_creds {
            Some(creds) => {
                log("Wasm::register. Sign up");

                let sign_up_request_factory = SignUpRequest {};
                let sign_up_request = sign_up_request_factory.generic_request(&creds.user_sig);

                repo
                    .save_event(&sign_up_request)
                    .await
                    .map_err(JsError::from)?;

                Ok(VaultInfo::Pending)
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