use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use async_mutex::Mutex;
use wasm_bindgen::{JsError, JsValue};

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::{ApplicationState, MetaVault, UserCredentials, VaultDoc};
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::events::sign_up::SignUpRequest;
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::meta_db::{MetaDb, MetaPassStore, VaultStore};
use meta_secret_core::node::db::models::{
    GenericKvLogEvent, KvKey, KvLogEvent, MempoolObject, ObjectDescriptor, VaultInfo,
};
use meta_secret_core::node::server::data_sync::{DataSync, MetaServerContextState};
use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
use meta_secret_core::shared_secret::MetaDistributor;

use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::gateway::WasmSyncGateway;
use crate::log;

pub fn get_data_sync(repo: Rc<WasmRepo>, creds: &UserCredentials) -> DataSync<WasmRepo, WasmMetaLogger, WasmDbError> {
    let persistent_object = get_persistent_object(repo.clone());
    let persistent_object_rc = Rc::new(persistent_object);

    let meta_db_manager = MetaDbManager {
        persistent_obj: persistent_object_rc.clone(),
        repo: repo.clone(),
        logger: WasmMetaLogger {},
    };
    let meta_db_manager_rc = Rc::new(meta_db_manager);

    DataSync {
        persistent_obj: persistent_object_rc,
        repo,
        context: Rc::new(MetaServerContextState::from(creds)),
        meta_db_manager: meta_db_manager_rc,
        logger: WasmMetaLogger {},
    }
}

pub fn get_persistent_object(repo: Rc<WasmRepo>) -> PersistentObject<WasmRepo, WasmDbError> {
    PersistentObject {
        repo: repo.clone(),
        global_index: PersistentGlobalIndex {
            repo,
            _phantom: PhantomData,
        },
    }
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

pub enum WasmMetaClient {
    Empty(EmptyMetaClient),
    Init(InitMetaClient),
    Registered(RegisteredMetaClient),
}

pub struct EmptyMetaClient {
    pub ctx: Rc<MetaClientContext>,
}

impl EmptyMetaClient {

    pub async fn find_user_creds(&self) -> Result<Option<InitMetaClient>, WasmDbError> {
        let maybe_creds = self.ctx.repo.find_user_creds().await?;

        match maybe_creds {
            Some(creds) => {
                let new_client = InitMetaClient {
                    ctx: self.ctx.clone(),
                    creds: Rc::new(creds)
                };
                Ok(Some(new_client))
            }
            None => {
                Ok(None)
            }
        }
    }

    pub async fn get_or_create_local_vault(&self, vault_name: &str, device_name: &str) -> Result<InitMetaClient, WasmDbError> {
        let meta_vault = self.create_meta_vault(vault_name, device_name).await?;
        let creds = self.generate_user_credentials(meta_vault).await?;

        Ok(InitMetaClient {
            ctx: self.ctx.clone(),
            creds: Rc::new(creds),
        })
    }

    async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> Result<MetaVault, WasmDbError> {
        log("wasm::create_meta_vault: create a meta vault");

        let logger = WasmMetaLogger {};

        let maybe_meta_vault = self.ctx.repo
            .find_meta_vault(&logger)
            .await?;

        match maybe_meta_vault {
            None => {
                self.ctx.repo
                    .create_meta_vault(vault_name.to_string(), device_name.to_string(), &logger)
                    .await
            }
            Some(meta_vault) => {
                if meta_vault.name != vault_name || meta_vault.device.device_name != device_name {
                    Err(WasmDbError::DbCustomError(String::from("Another meta vault already exists in the database")))
                } else {
                    Ok(meta_vault)
                }
            }
        }
    }

    async fn generate_user_credentials(&self, meta_vault: MetaVault) -> Result<UserCredentials, WasmDbError> {
        log("wasm::generate_user_credentials: generate a new security box");

        let maybe_creds = self.ctx.repo.find_user_creds()
            .await?;

        match maybe_creds {
            None => {
                let security_box = KeyManager::generate_security_box(meta_vault.name);
                let user_sig = security_box.get_user_sig(&meta_vault.device);
                let creds = UserCredentials::new(security_box, user_sig);
                self.ctx.repo
                    .save_user_creds(&creds)
                    .await?;

                Ok(creds)
            }
            Some(creds) => {
                Ok(creds)
            }
        }
    }
}

pub struct InitMetaClient {
    pub ctx: Rc<MetaClientContext>,
    pub creds: Rc<UserCredentials>,
}

impl InitMetaClient {
    pub async fn sign_up(&self) -> RegisteredMetaClient {
        let vault_info = self.get_vault().await;

        let join = self.ctx.is_join().await;
        if join {
            //TODO we need to know if the user in pending state (waiting for approval)
            self.join_cluster().await;
            self.ctx.sync_gateway.sync().await;
            if let VaultInfo::Member { vault } = &vault_info {
                self.ctx.update_vault(vault.clone()).await
            }
        } else {
            self.sign_up_action(&vault_info).await;
        }

        RegisteredMetaClient {
            ctx: self.ctx.clone(),
            creds: self.creds.clone(),
            vault_info,
        }
    }

    async fn join_cluster(&self) -> Result<VaultInfo, JsValue> {
        log("Wasm::register. Join");

        let mem_pool_tail_id = self.ctx
            .meta_db_manager
            .persistent_obj
            .find_tail_id_by_obj_desc(&ObjectDescriptor::Mempool)
            .await
            .unwrap_or(ObjectId::mempool_unit());

        let join_request = GenericKvLogEvent::Mempool(MempoolObject::JoinRequest {
            event: KvLogEvent {
                key: KvKey {
                    obj_id: mem_pool_tail_id,
                    obj_desc: ObjectDescriptor::Mempool,
                },
                value: self.creds.user_sig.as_ref().clone(),
            }
        });

        self.ctx.repo
            .save_event(&join_request)
            .await
            .map_err(JsError::from)?;

        Ok(VaultInfo::Pending)
    }

    async fn sign_up_action(&self, vault_info: &VaultInfo) {
        match vault_info {
            VaultInfo::Member { vault } => {
                self.ctx.update_vault(vault.clone()).await;
            }
            VaultInfo::Pending => {
                log("Pending is not expected here");
            }
            VaultInfo::Declined => {
                log("Declined - is not expected here");
            }
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
                self.ctx.enable_join().await;
            }
        }
    }

    async fn register(&self) -> Result<VaultInfo, WasmDbError> {
        log("Wasm::register. Sign up");

        let sign_up_request_factory = SignUpRequest {};
        let sign_up_request = sign_up_request_factory.generic_request(&self.creds.user_sig);

        self.ctx.repo
            .save_event(&sign_up_request)
            .await?;

        Ok(VaultInfo::Pending)
    }

    pub async fn get_vault(&self) -> VaultInfo {
        log("wasm: get vault!");

        self.ctx.sync_gateway.sync().await;

        let creds = self.creds.clone();

        let mut meta_db = self.ctx.meta_db.lock().await;

        let vault_unit_id = ObjectId::vault_unit(creds.user_sig.vault.name.as_str());

        if meta_db.vault_store == VaultStore::Empty {
            meta_db.vault_store = VaultStore::Unit { tail_id: vault_unit_id.clone() }
        }

        if meta_db.meta_pass_store == MetaPassStore::Empty {
            meta_db.meta_pass_store = MetaPassStore::Unit {
                tail_id: ObjectId::meta_pass_unit(creds.user_sig.vault.name.as_str())
            }
        }

        let updated_meta_db = self
            .ctx
            .meta_db_manager
            .sync_meta_db(meta_db.clone())
            .await;

        meta_db.vault_store = updated_meta_db.vault_store;
        meta_db.global_index_store = updated_meta_db.global_index_store;
        meta_db.meta_pass_store = updated_meta_db.meta_pass_store;

        let global_index = meta_db.global_index_store.global_index.clone();

        if global_index.contains(vault_unit_id.id_str().as_str()) {
            //if the vault is already present:
            match &meta_db.vault_store {
                VaultStore::Store { vault, .. } => {
                    VaultInfo::Member { vault: vault.clone() }
                }
                _ => VaultInfo::NotMember
            }
        } else {
            VaultInfo::NotFound
        }
    }
}

pub struct RegisteredMetaClient {
    pub ctx: Rc<MetaClientContext>,
    pub creds: Rc<UserCredentials>,
    pub vault_info: VaultInfo,
}

impl RegisteredMetaClient {
    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        log("wasm: cluster distribution!!!!");

        match &self.vault_info {
            VaultInfo::Member { vault } => {
                let distributor = MetaDistributor {
                    meta_db_manager: MetaDbManager {
                        persistent_obj: self.ctx.persistent_object.clone(),
                        repo: self.ctx.repo.clone(),
                        logger: WasmMetaLogger {},
                    },
                    vault: vault.clone(),
                    user_creds: self.creds.as_ref().clone(),
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

        //sync meta passwords
        self.ctx.sync_gateway.sync().await;

        let mut meta_db = self.ctx.meta_db.lock().await;

        let updated_meta_db = self
            .ctx.
            meta_db_manager
            .sync_meta_db(meta_db.clone())
            .await;

        meta_db.vault_store = updated_meta_db.vault_store;
        meta_db.global_index_store = updated_meta_db.global_index_store;
        meta_db.meta_pass_store = updated_meta_db.meta_pass_store;
    }
}

pub struct MetaClientContext {
    pub meta_db: Arc<Mutex<MetaDb>>,
    pub meta_db_manager: MetaDbManager<WasmRepo, WasmMetaLogger, WasmDbError>,
    pub app_state: Arc<Mutex<ApplicationState>>,
    pub sync_gateway: Rc<WasmSyncGateway>,
    pub persistent_object: Rc<PersistentObject<WasmRepo, WasmDbError>>,
    pub repo: Rc<WasmRepo>,
}

impl MetaClientContext {
    pub fn new(app_state: Arc<Mutex<ApplicationState>>, repo: Rc<WasmRepo>) -> Self {
        let persistent_object = {
            let obj = get_persistent_object(repo.clone());
            Rc::new(obj)
        };

        let meta_db_manager = MetaDbManager {
            persistent_obj: persistent_object.clone(),
            repo: repo.clone(),
            logger: WasmMetaLogger {},
        };

        let meta_db = Arc::new(Mutex::new(MetaDb::default()));
        MetaClientContext {
            meta_db,
            meta_db_manager,
            app_state,
            sync_gateway: Rc::new(WasmSyncGateway::new()),
            persistent_object,
            repo,
        }
    }

    pub async fn is_join(&self) -> bool {
        {
            let app_state = self.app_state.lock().await;
            app_state.join_component
        }
    }

    pub async fn update_vault(&self, vault: VaultDoc) {
        {
            let mut app_state = self.app_state.lock().await;
            app_state.vault = Some(Box::new(vault))
        }
    }

    pub async fn update_meta_vault(&self, meta_vault: Box<MetaVault>) {
        {
            let mut app_state = self.app_state.lock().await;
            app_state.meta_vault = Some(meta_vault)
        }
    }

    pub async fn enable_join(&self) {
        {
            let mut app_state = self.app_state.lock().await;
            app_state.join_component = true;
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