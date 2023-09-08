use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use anyhow::anyhow;

use async_mutex::Mutex;

use crate::crypto::keys::KeyManager;
use crate::models::{
    ApplicationState, MetaPasswordDoc, MetaPasswordId, MetaVault, UserCredentials, VaultDoc}
;
use crate::models::password_recovery_request::PasswordRecoveryRequest;
use crate::node::app::meta_manager::{MetaVaultManager, UserCredentialsManager};
use crate::node::db::actions::sign_up::SignUpRequest;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::events::common::{MempoolObject, ObjectCreator, SharedSecretObject, VaultInfo};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_view::MetaDb;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::secret::MetaDistributor;

pub enum MetaClient<Repo: KvLogEventRepo, Logger: MetaLogger> {
    Empty(EmptyMetaClient<Repo, Logger>),
    Init(InitMetaClient<Repo, Logger>),
    Registered(RegisteredMetaClient<Repo, Logger>),
}

impl<Repo, Logger> ToString for MetaClient<Repo, Logger> where Repo: KvLogEventRepo, Logger: MetaLogger {
    fn to_string(&self) -> String {
        match self {
            MetaClient::Empty(_) => String::from("Empty"),
            MetaClient::Init(_) => String::from("Init"),
            MetaClient::Registered(_) => String::from("Registered")
        }
    }
}

impl<Repo, Logger> MetaClient<Repo, Logger>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger
{
    pub fn get_ctx(&self) -> Rc<MetaClientContext<Repo, Logger>> {
        match self {
            MetaClient::Empty(client) => {
                client.ctx.clone()
            }
            MetaClient::Init(client) => {
                client.ctx.clone()
            }
            MetaClient::Registered(client) => {
                client.ctx.clone()
            }
        }
    }
}

pub struct EmptyMetaClient<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub ctx: Rc<MetaClientContext<Repo, Logger>>,
    pub logger: Rc<Logger>,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> EmptyMetaClient<Repo, Logger> {
    pub async fn find_user_creds(&self) -> anyhow::Result<Option<InitMetaClient<Repo, Logger>>> {
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

    pub async fn get_or_create_local_vault(&self, vault_name: &str, device_name: &str) -> anyhow::Result<InitMetaClient<Repo, Logger>> {
        let meta_vault = self.create_meta_vault(vault_name, device_name).await?;
        let creds = self.generate_user_credentials(meta_vault).await?;

        Ok(InitMetaClient {
            ctx: self.ctx.clone(),
            creds: Rc::new(creds),
            logger: self.logger.clone(),
        })
    }

    async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> anyhow::Result<MetaVault> {
        self.logger.info("create_meta_vault: create a meta vault");


        let maybe_meta_vault = self.ctx.repo
            .find_meta_vault()
            .await?;

        match maybe_meta_vault {
            None => {
                self.ctx.repo
                    .create_meta_vault(vault_name.to_string(), device_name.to_string())
                    .await
            }
            Some(meta_vault) => {
                if meta_vault.name != vault_name || meta_vault.device.device_name != device_name {
                    Err(anyhow!("Another meta vault already exists in the database"))
                } else {
                    Ok(meta_vault)
                }
            }
        }
    }

    async fn generate_user_credentials(&self, meta_vault: MetaVault) -> anyhow::Result<UserCredentials> {
        self.logger.info("generate_user_credentials: generate a new security box");

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

pub struct InitMetaClient<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub ctx: Rc<MetaClientContext<Repo, Logger>>,
    pub creds: Rc<UserCredentials>,
    pub logger: Rc<Logger>,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> InitMetaClient<Repo, Logger> {
    pub async fn sign_up(&self) -> RegisteredMetaClient<Repo, Logger> {
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
        self.logger.info("register. Join");

        let mem_pool_tail_id = self.ctx
            .persistent_object
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

    async fn register(&self) -> anyhow::Result<VaultInfo> {
        self.logger.info("register. Sign up");

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

        let meta_db_service = self.ctx.meta_db_service.clone();
        let vault_name = creds.user_sig.vault.name.as_str();
        let _ = meta_db_service.update_with_vault(vault_name.to_string()).await;

        let vault_unit_id = ObjectId::vault_unit(vault_name);
        meta_db_service.get_vault_info(vault_unit_id).await.unwrap()
    }
}

pub struct RegisteredMetaClient<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub ctx: Rc<MetaClientContext<Repo, Logger>>,
    pub creds: Rc<UserCredentials>,
    pub vault_info: VaultInfo,
    logger: Rc<Logger>,
}

impl<Repo, Logger> RegisteredMetaClient<Repo, Logger>
    where
        Repo: KvLogEventRepo,
        Logger: MetaLogger {
    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str) {
        self.logger.info("cluster distribution!!!!");

        match &self.vault_info {
            VaultInfo::Member { vault } => {
                let vault_name = vault.vault_name.clone();
                self.ctx.meta_db_service.update_with_vault(vault_name).await;

                let distributor = MetaDistributor {
                    meta_db_service: self.ctx.meta_db_service.clone(),
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
            meta_db_service.sync_db()
            .await;
    }

    pub async fn recovery_request(&self, meta_pass_id: MetaPasswordId) {
        match &self.vault_info {
            VaultInfo::Member { vault } => {
                for curr_sig in &vault.signatures {
                    if self.creds.user_sig.public_key.base64_text == curr_sig.public_key.base64_text {
                        continue;
                    }

                    let recovery_request = PasswordRecoveryRequest {
                        id: Box::new(meta_pass_id.clone()),
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

                self.ctx.meta_db_service.sync_db().await
            }
            VaultInfo::Pending => {}
            VaultInfo::Declined => {}
            VaultInfo::NotFound => {}
            VaultInfo::NotMember => {}
        }
    }
}

pub struct MetaClientContext<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub persistent_object: Rc<PersistentObject<Repo, Logger>>,
    pub repo: Rc<Repo>,
    pub logger: Rc<Logger>,
    pub meta_db_service: Rc<MetaDbService<Repo, Logger>>,
    pub app_state: RefCell<ApplicationState>,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> MetaClientContext<Repo, Logger> {
    pub fn new(app_state: RefCell<ApplicationState>, repo: Rc<Repo>, logger: Rc<Logger>, meta_db_service: Rc<MetaDbService<Repo, Logger>>) -> Self {
        let persistent_object = Rc::new(PersistentObject::new(repo.clone(), logger.clone()));

        MetaClientContext {
            persistent_object,
            repo,
            logger,
            meta_db_service,
            app_state
        }
    }

    pub async fn is_join(&self) -> bool {
        self.app_state.borrow().join_component
    }

    pub async fn update_vault(&self, vault: VaultDoc) {
        self.logger.info(format!("App state. Update vault: {:?}", &vault).as_str());

        let mut app_state = self.app_state.borrow_mut();
        app_state.vault = Some(Box::new(vault));
    }

    pub async fn update_meta_passwords(&self, passes: Vec<MetaPasswordDoc>) {
        let mut app_state = self.app_state.borrow_mut();
        app_state.meta_passwords = passes.clone();
    }

    pub async fn update_meta_vault(&self, meta_vault: Box<MetaVault>) {
        let mut app_state = self.app_state.borrow_mut();
        app_state.meta_vault = Some(meta_vault)
    }

    pub async fn enable_join(&self) {
        let mut app_state = self.app_state.borrow_mut();
        app_state.join_component = true;
    }
}
