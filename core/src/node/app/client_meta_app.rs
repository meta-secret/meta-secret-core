use std::sync::Arc;

use anyhow::anyhow;
use log::error;
use tracing::{debug, info, instrument, Instrument};

use crate::crypto::keys::KeyManager;
use crate::models::{MetaVault, UserCredentials};
use crate::node::app::meta_app::app_state::{ConfiguredAppState, GenericAppState, JoinedAppState};
use crate::node::app::meta_vault_manager::{MetaVaultManager, UserCredentialsManager};
use crate::node::db::actions::sign_up::SignUpRequest;
use crate::node::db::events::common::{MemPoolObject, VaultInfo};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbServiceProxy;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::secret::MetaDistributor;

pub struct MetaClient<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub meta_db_service_proxy: Arc<MetaDbServiceProxy>,
}

impl<Repo: KvLogEventRepo> MetaClient<Repo> {
    pub async fn find_user_creds(&self, curr_state: &GenericAppState) -> anyhow::Result<Option<ConfiguredAppState>> {
        let maybe_creds = self.persistent_obj.repo.find_user_creds().in_current_span().await?;

        match maybe_creds {
            None => Ok(None),
            Some(creds) => Ok(Some(ConfiguredAppState {
                app_state: curr_state.get_state(),
                creds,
            })),
        }
    }

    #[instrument(skip_all)]
    pub async fn get_or_create_local_vault(
        &self,
        vault_name: &str,
        device_name: &str,
    ) -> anyhow::Result<UserCredentials> {
        let meta_vault = self
            .create_meta_vault(vault_name, device_name)
            .in_current_span()
            .await?;
        let creds = self.generate_user_credentials(meta_vault).in_current_span().await?;
        Ok(creds)
    }

    async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> anyhow::Result<MetaVault> {
        info!("Create a meta vault");

        let maybe_meta_vault = self.persistent_obj.repo.find_meta_vault().await?;

        match maybe_meta_vault {
            None => {
                self.persistent_obj
                    .repo
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

    #[instrument(skip(self))]
    async fn generate_user_credentials(&self, meta_vault: MetaVault) -> anyhow::Result<UserCredentials> {
        info!("generate_user_credentials: generate a new security box");

        let maybe_creds = self.persistent_obj.repo.find_user_creds().in_current_span().await?;

        match maybe_creds {
            None => {
                let security_box = KeyManager::generate_security_box(meta_vault.name);
                let user_sig = security_box.get_user_sig(&meta_vault.device);
                let creds = UserCredentials::new(security_box, user_sig);
                self.persistent_obj.repo.save_user_creds(&creds).await?;

                info!(
                    "User creds has been generated. Pk: {}",
                    creds.user_sig.public_key.base64_text
                );
                Ok(creds)
            }
            Some(creds) => Ok(creds),
        }
    }
}

impl<Repo: KvLogEventRepo> MetaClient<Repo> {
    #[instrument(skip_all)]
    pub async fn sign_up(&self, curr_state: &ConfiguredAppState) -> JoinedAppState {
        let join = curr_state.app_state.join_component;

        if join {
            //TODO we need to know if the user in pending state (waiting for approval)
            info!("Join to cluster: {:?}", curr_state.creds.user_sig.vault.name.clone());
            self.join_cluster(curr_state.clone()).in_current_span().await;
        } else {
            let vault_info = self.get_vault(curr_state.creds.user_sig.vault.name.clone()).await;
            self.sign_up_action(&vault_info, curr_state).await;
        }

        let mut updated_app_state = curr_state.app_state.clone();

        let vault_info = self.get_vault(curr_state.creds.user_sig.vault.name.clone()).await;
        if let VaultInfo::Member { vault } = &vault_info {
            updated_app_state.vault = Some(Box::new(vault.clone()))
        }

        JoinedAppState {
            app_state: updated_app_state,
            creds: curr_state.creds.clone(),
            vault_info,
        }
    }

    async fn join_cluster(&self, curr_state: ConfiguredAppState) {
        info!("Registration: Join cluster");

        let mem_pool_tail_id = self
            .persistent_obj
            .find_tail_id_by_obj_desc(&ObjectDescriptor::MemPool)
            .await
            .unwrap_or(ObjectId::mempool_unit());

        let join_request = GenericKvLogEvent::MemPool(MemPoolObject::JoinRequest {
            event: KvLogEvent {
                key: KvKey {
                    obj_id: mem_pool_tail_id,
                    obj_desc: ObjectDescriptor::MemPool,
                },
                value: curr_state.creds.user_sig.as_ref().clone(),
            },
        });

        let _ = self.persistent_obj.repo.save_event(join_request).await;
    }

    #[instrument(skip_all)]
    async fn sign_up_action(&self, vault_info: &VaultInfo, curr_state: &ConfiguredAppState) {
        match vault_info {
            VaultInfo::Member { .. } => {
                info!("The client is already signed up")
            }
            VaultInfo::Pending => {
                info!("Pending is not expected here");
            }
            VaultInfo::Declined => {
                info!("Declined - is not expected here");
            }
            VaultInfo::NotFound => {
                info!("Register a new vault: {:?}", curr_state.creds.user_sig.vault);

                let reg_res = self.register(&curr_state.creds).in_current_span().await;

                match reg_res {
                    Ok(vault_info) => {
                        info!("Successful registration, vault: {:?}", vault_info);
                    }
                    Err(err) => {
                        error!("Error. Registration failed: {:?}", err);
                    }
                }
            }
            VaultInfo::NotMember => {
                panic!("Invalid state: sign_up action. The client is not a member of a vault.")
            }
        }
    }

    #[instrument(skip_all)]
    async fn register(&self, creds: &UserCredentials) -> anyhow::Result<VaultInfo> {
        info!("Register. Sign up");

        let sign_up_request_factory = SignUpRequest {};
        let sign_up_request = sign_up_request_factory.generic_request(&creds.user_sig);

        self.persistent_obj
            .repo
            .save_event(sign_up_request)
            .in_current_span()
            .await?;

        Ok(VaultInfo::Pending)
    }

    #[instrument(skip_all)]
    pub async fn get_vault(&self, vault_name: String) -> VaultInfo {
        debug!("Get vault");

        self.meta_db_service_proxy
            .get_vault_info(vault_name)
            .in_current_span()
            .await
            .unwrap()
    }
}

impl<Repo> MetaClient<Repo>
where
    Repo: KvLogEventRepo,
{
    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str, app_state: &JoinedAppState) {
        info!("Cluster distribution. App state: {:?}", app_state);

        if let VaultInfo::Member { vault } = &app_state.vault_info {
            let distributor = MetaDistributor {
                persistent_obj: self.persistent_obj.clone(),
                vault: vault.clone(),
                user_creds: Arc::new(app_state.creds.clone()),
            };

            distributor.distribute(pass_id.to_string(), pass.to_string()).await;
        } else {
            error!("Password distribution is not available. The user is not a member of the vault");
            panic!();
        }
    }
}
