use std::rc::Rc;
use std::sync::Arc;

use async_mutex::Mutex;
use wasm_bindgen::{JsError, JsValue};

use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::{
    ApplicationState, MetaPasswordDoc, MetaPasswordId, MetaVault, UserCredentials, VaultDoc}
;
use meta_secret_core::models::password_recovery_request::PasswordRecoveryRequest;
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::actions::sign_up::SignUpRequest;
use meta_secret_core::node::db::meta_db::meta_db_manager::MetaDbManager;
use meta_secret_core::node::db::events::common::{MempoolObject, ObjectCreator, SharedSecretObject, VaultInfo};
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use meta_secret_core::node::db::events::object_descriptor::ObjectDescriptor;
use meta_secret_core::node::db::events::object_id::{IdGen, ObjectId};
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::meta_db::meta_db_view::{MetaDb, MetaPassStore, VaultStore};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::logger::MetaLogger;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use meta_secret_core::secret::MetaDistributor;
use meta_secret_core::secret::shared_secret::{PlainText, SharedSecretEncryption, UserShareDto};

use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::alert;

pub enum WasmMetaClient {
    Empty(EmptyMetaClient),
    Init(InitMetaClient),
    Registered(RegisteredMetaClient),
}

impl ToString for WasmMetaClient {
    fn to_string(&self) -> String {
        match self {
            WasmMetaClient::Empty(_) => String::from("Empty"),
            WasmMetaClient::Init(_) => String::from("Init"),
            WasmMetaClient::Registered(_) => String::from("Registered")
        }
    }
}

impl WasmMetaClient {
    pub fn get_ctx(&self) -> Rc<MetaClientContext> {
        match self {
            WasmMetaClient::Empty(client) => {
                client.ctx.clone()
            }
            WasmMetaClient::Init(client) => {
                client.ctx.clone()
            }
            WasmMetaClient::Registered(client) => {
                client.ctx.clone()
            }
        }
    }
}

pub struct EmptyMetaClient {
    pub ctx: Rc<MetaClientContext>,
    pub logger: Rc<dyn MetaLogger>,
}

impl EmptyMetaClient {
    pub async fn find_user_creds(&self) -> Result<Option<InitMetaClient>, Box<dyn std::error::Error>> {
        let maybe_creds = self.ctx.repo.find_user_creds().await?;

        match maybe_creds {
            Some(creds) => {
                let new_client = InitMetaClient {
                    ctx: self.ctx.clone(),
                    creds: Rc::new(creds),
                    logger: self.logger.clone(),
                };
                Ok(Some(new_client))
            }
            None => {
                Ok(None)
            }
        }
    }

    pub async fn get_or_create_local_vault(&self, vault_name: &str, device_name: &str) -> Result<InitMetaClient, Box<dyn std::error::Error>> {
        let meta_vault = self.create_meta_vault(vault_name, device_name).await?;
        let creds = self.generate_user_credentials(meta_vault).await?;

        Ok(InitMetaClient {
            ctx: self.ctx.clone(),
            creds: Rc::new(creds),
            logger: self.logger.clone(),
        })
    }

    async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> Result<MetaVault, Box<dyn std::error::Error>> {
        self.logger.info("wasm::create_meta_vault: create a meta vault");

        let logger = WasmMetaLogger {
            id: self.logger.id()
        };

        let maybe_meta_vault = self.ctx.repo
            .find_meta_vault(&logger)
            .await?;

        match maybe_meta_vault {
            None => {
                self.ctx.repo
                    .create_meta_vault(vault_name.to_string(), device_name.to_string())
                    .await
            }
            Some(meta_vault) => {
                if meta_vault.name != vault_name || meta_vault.device.device_name != device_name {
                    let err = {
                        let err_msg = String::from("Another meta vault already exists in the database");
                        WasmDbError::DbCustomError(err_msg)
                    };
                    Err(Box::from(err))
                } else {
                    Ok(meta_vault)
                }
            }
        }
    }

    async fn generate_user_credentials(&self, meta_vault: MetaVault) -> Result<UserCredentials, Box<dyn std::error::Error>> {
        self.logger.info("wasm::generate_user_credentials: generate a new security box");

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
    pub logger: Rc<dyn MetaLogger>,
}

impl InitMetaClient {
    pub async fn sign_up(&self) -> RegisteredMetaClient {
        self.logger.info("InitClient: sign up");
        let vault_info = self.get_vault().await;

        let join = self.ctx.is_join().await;
        if join {
            //TODO we need to know if the user in pending state (waiting for approval)
            self.join_cluster().await;

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
            logger: self.logger.clone(),
        }
    }

    async fn join_cluster(&self) {
        self.logger.info("Wasm::register. Join");

        let mem_pool_tail_id = self.ctx
            .meta_db_manager
            .persistent_obj
            .find_tail_id_by_obj_desc(&ObjectDescriptor::Mempool)
            .await
            .unwrap_or(ObjectId::mempool_unit());

        let join_request = GenericKvLogEvent::Mempool(MempoolObject::JoinRequest {
            event: KvLogEvent {
                key: KvKey::Key {
                    obj_id: mem_pool_tail_id,
                    obj_desc: ObjectDescriptor::Mempool,
                },
                value: self.creds.user_sig.as_ref().clone(),
            }
        });

        let _ = self.ctx.repo
            .save_event(&join_request)
            .await;
    }

    async fn sign_up_action(&self, vault_info: &VaultInfo) {
        match vault_info {
            VaultInfo::Member { vault } => {
                self.ctx.update_vault(vault.clone()).await;
            }
            VaultInfo::Pending => {
                self.logger.info("Pending is not expected here");
            }
            VaultInfo::Declined => {
                self.logger.info("Declined - is not expected here");
            }
            VaultInfo::NotFound => {
                let reg_res = self
                    .register()
                    .await;

                match reg_res {
                    Ok(_vault_info) => {
                        self.logger.info("Successful registration");
                    }
                    Err(_) => {
                        self.logger.info("Error. Registration failed");
                    }
                }
            }
            VaultInfo::NotMember => {
                self.ctx.enable_join().await;
            }
        }
    }

    async fn register(&self) -> Result<VaultInfo, Box<dyn std::error::Error>> {
        self.logger.info("Wasm::register. Sign up");

        let sign_up_request_factory = SignUpRequest {};
        let sign_up_request = sign_up_request_factory.generic_request(&self.creds.user_sig);

        self.ctx.repo
            .save_event(&sign_up_request)
            .await?;

        Ok(VaultInfo::Pending)
    }

    pub async fn get_vault(&self) -> VaultInfo {
        self.logger.debug("Get vault");

        let creds = self.creds.clone();

        let mut meta_db = self.ctx.meta_db.lock().await;
        let vault_name = creds.user_sig.vault.name.as_str();
        meta_db.update_vault_info(vault_name);
        let vault_unit_id = ObjectId::vault_unit(vault_name);

        self
            .ctx
            .meta_db_manager
            .sync_meta_db(&mut meta_db)
            .await;

        if meta_db.global_index_store.contains(vault_unit_id.id_str()) {
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
    logger: Rc<dyn MetaLogger>,
}

impl RegisteredMetaClient {
    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        self.logger.info("wasm: cluster distribution!!!!");

        let mut meta_db = self.ctx.meta_db.lock().await;

        match &self.vault_info {
            VaultInfo::Member { vault } => {
                let vault_name = vault.vault_name.clone();
                meta_db.update_vault_info(vault_name.as_str());

                let distributor = MetaDistributor {
                    meta_db_manager: Rc::new(MetaDbManager {
                        persistent_obj: self.ctx.persistent_object.clone(),
                        repo: self.ctx.repo.clone(),
                        logger: self.logger.clone(),
                    }),
                    vault: vault.clone(),
                    user_creds: self.creds.clone(),
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

        self
            .ctx.
            meta_db_manager
            .sync_meta_db(&mut meta_db)
            .await;
    }

    pub async fn recovery_request(&self, meta_pass_id: JsValue) {
        match &self.vault_info {
            VaultInfo::Member { vault } => {
                let mut meta_db = self.ctx.meta_db.lock().await;

                for curr_sig in &vault.signatures {
                    if self.creds.user_sig.public_key.base64_text == curr_sig.public_key.base64_text {
                        continue;
                    }

                    let id: MetaPasswordId = serde_wasm_bindgen::from_value(meta_pass_id.clone())
                        .unwrap();

                    let recovery_request = PasswordRecoveryRequest {
                        id: Box::new(id),
                        consumer: Box::new(curr_sig.clone()),
                        provider: self.creds.user_sig.clone(),
                    };

                    let obj_desc = ObjectDescriptor::SharedSecret {
                        vault_name: curr_sig.vault.name.clone(),
                        device_id: curr_sig.vault.device.device_id.clone(),
                    };
                    let generic_event = GenericKvLogEvent::SharedSecret(SharedSecretObject::RecoveryRequest {
                        event: KvLogEvent {
                            key: KvKey::Empty { obj_desc: obj_desc.clone() },
                            value: recovery_request,
                        },
                    });

                    let slot_id = self.ctx.persistent_object
                        .find_tail_id_by_obj_desc(&obj_desc)
                        .await
                        .map(|id| id.next())
                        .unwrap_or(ObjectId::unit(&obj_desc));

                    let _ = self.ctx.repo
                        .save(&slot_id, &generic_event)
                        .await;
                }

                self
                    .ctx.
                    meta_db_manager
                    .sync_meta_db(&mut meta_db)
                    .await;
            }
            VaultInfo::Pending => {}
            VaultInfo::Declined => {}
            VaultInfo::NotFound => {}
            VaultInfo::NotMember => {}
        }
    }
}

pub struct MetaClientContext {
    pub meta_db: Arc<Mutex<MetaDb>>,
    pub app_state: Arc<Mutex<ApplicationState>>,

    pub meta_db_manager: MetaDbManager,
    pub persistent_object: Rc<PersistentObject>,
    pub repo: Rc<WasmRepo>,
    pub logger: Rc<dyn MetaLogger>,
}

impl MetaClientContext {
    pub fn new(app_state: Arc<Mutex<ApplicationState>>, repo: Rc<WasmRepo>, logger: Rc<dyn MetaLogger>) -> Self {
        let persistent_object = Rc::new(PersistentObject::new(repo.clone(), logger.clone()));
        let meta_db_manager = MetaDbManager::from(persistent_object.clone());

        let meta_db = Arc::new(Mutex::new(MetaDb::new(String::from("client"), logger.clone())));
        MetaClientContext {
            meta_db,
            meta_db_manager,
            app_state,
            persistent_object,
            repo,
            logger
        }
    }

    pub async fn is_join(&self) -> bool {
        {
            let app_state = self.app_state.lock().await;
            app_state.join_component
        }
    }

    pub async fn update_vault(&self, vault: VaultDoc) {
        self.logger.info(format!("App state. Update vault: {:?}", &vault).as_str());

        let mut app_state = self.app_state.lock().await;
        app_state.vault = Some(Box::new(vault));
    }

    pub async fn update_meta_passwords(&self, passes: &Vec<MetaPasswordDoc>) {
        let mut app_state = self.app_state.lock().await;
        app_state.meta_passwords = passes.clone();
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