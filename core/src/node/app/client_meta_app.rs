use std::sync::Arc;

use tracing::{debug, error, info, instrument, Instrument};
use crate::node::app::meta_app::app_state::{ConfiguredAppState, GenericAppState, JoinedAppState};
use crate::node::common::model::vault::VaultName;
use crate::node::db::actions::sign_up::SignUpRequest;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::secret::MetaDistributor;

pub struct MetaClient<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>
}

impl<Repo: KvLogEventRepo> MetaClient<Repo> {
    pub async fn find_user_creds(&self, curr_state: &GenericAppState) -> anyhow::Result<Option<ConfiguredAppState>> {
        let maybe_creds = self.persistent_obj.repo.find_device_creds().await?;

        match maybe_creds {
            None => Ok(None),
            Some(creds) => Ok(Some(ConfiguredAppState {
                app_state: curr_state.get_state(),
                creds,
            })),
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
            info!("Sign up to cluster: {:?}", curr_state.creds.user_sig.vault.name.clone());
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
                value: curr_state.creds.user_sig.clone(),
            },
        });

        let _ = self.persistent_obj.repo.save(join_request).await;
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
                error!("Invalid state: sign_up action. The client is not a member of a vault.")
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
            .save(sign_up_request)
            .in_current_span()
            .await?;

        Ok(VaultInfo::Pending)
    }

    #[instrument(skip_all)]
    pub async fn get_vault(&self, vault_name: VaultName) -> VaultInfo {
        debug!("Get vault");

        self.read_db_service_proxy
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
        }
    }
}
