use std::sync::Arc;

use crate::node::common::data_transfer::MpscDataTransfer;
use anyhow::anyhow;
use tracing::{debug, info, instrument, Instrument};

use crate::node::db::events::common::VaultInfo;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_view::{MetaDb, TailId};
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;

pub struct MetaDbService<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub repo: Arc<Repo>,
    pub meta_db_id: String,
    pub data_transfer: Arc<MetaDbDataTransfer>,
}

pub struct MetaDbDataTransfer {
    pub dt: MpscDataTransfer<MetaDbRequestMessage, MetaDbResponseMessage>,
}

pub enum MetaDbRequestMessage {
    GetVaultInfo { vault_name: String },
    GetVaultStore { vault_name: String },
    GetMetaPassStore { vault_name: String },
}

pub enum MetaDbResponseMessage {
    VaultInfo { vault: VaultInfo },
    VaultStore { vault_store: VaultStore },
    MetaPassStore { meta_pass_store: MetaPassStore },
}

impl<Repo: KvLogEventRepo> MetaDbService<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run meta_db service");

        while let Ok(msg) = self.data_transfer.dt.service_receive().in_current_span().await {
            let mut meta_db = MetaDb::new(self.meta_db_id.clone());
            //Global index - FULL synchronization
            self.sync_meta_db(&mut meta_db).in_current_span().await;

            match msg {
                MetaDbRequestMessage::GetVaultInfo { vault_name } => {
                    meta_db.update_vault_info(vault_name.as_str());
                    self.sync_meta_db(&mut meta_db).in_current_span().await;

                    let vault_unit_id = ObjectId::vault_unit(vault_name.as_str());
                    let vault_info = if meta_db.global_index_store.contains(vault_unit_id.id_str()) {
                        //if the vault is already present:
                        match &meta_db.vault_store {
                            VaultStore::Store { vault, .. } => VaultInfo::Member { vault: vault.clone() },
                            _ => VaultInfo::NotMember,
                        }
                    } else {
                        VaultInfo::NotFound
                    };

                    let response = MetaDbResponseMessage::VaultInfo { vault: vault_info };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
                MetaDbRequestMessage::GetVaultStore { vault_name } => {
                    meta_db.update_vault_info(vault_name.as_str());
                    self.sync_meta_db(&mut meta_db).in_current_span().await;

                    let response = MetaDbResponseMessage::VaultStore {
                        vault_store: meta_db.vault_store.clone(),
                    };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
                MetaDbRequestMessage::GetMetaPassStore { vault_name } => {
                    meta_db.update_vault_info(vault_name.as_str());
                    self.sync_meta_db(&mut meta_db).in_current_span().await;

                    let response = MetaDbResponseMessage::MetaPassStore {
                        meta_pass_store: meta_db.meta_pass_store.clone(),
                    };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
            }
        }
    }

    async fn sync_meta_db(&self, meta_db: &mut MetaDb) {
        debug!("Sync meta db");

        let vault_events = match meta_db.vault_store.tail_id() {
            None => {
                vec![]
            }
            Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).in_current_span().await,
        };

        //sync global index
        let gi_events = {
            let maybe_gi_tail_id = meta_db.global_index_store.tail_id();

            match maybe_gi_tail_id {
                None => {
                    self.persistent_obj
                        .find_object_events(&ObjectId::global_index_unit())
                        .in_current_span()
                        .await
                }
                Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).in_current_span().await,
            }
        };

        let meta_pass_events = {
            match meta_db.meta_pass_store.tail_id() {
                None => {
                    vec![]
                }
                Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).in_current_span().await,
            }
        };

        let mut commit_log = vec![];
        commit_log.extend(vault_events);
        commit_log.extend(gi_events);
        commit_log.extend(meta_pass_events);

        meta_db.apply(commit_log);
    }
}

pub struct MetaDbServiceProxy {
    pub dt: Arc<MetaDbDataTransfer>,
}

impl MetaDbServiceProxy {
    pub async fn get_vault_info(&self, vault_name: String) -> anyhow::Result<VaultInfo> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(MetaDbRequestMessage::GetVaultInfo { vault_name })
            .in_current_span()
            .await?;

        match msg {
            MetaDbResponseMessage::VaultInfo { vault } => Ok(vault),
            _ => Err(anyhow!("Invalid message")),
        }
    }

    pub async fn get_vault_store(&self, vault_name: String) -> anyhow::Result<VaultStore> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(MetaDbRequestMessage::GetVaultStore { vault_name })
            .in_current_span()
            .await?;

        match msg {
            MetaDbResponseMessage::VaultStore { vault_store } => Ok(vault_store),
            MetaDbResponseMessage::VaultInfo { .. } => {
                Err(anyhow!("Invalid message. Expected VaultStore, got VaultInfo"))
            }
            MetaDbResponseMessage::MetaPassStore { .. } => {
                Err(anyhow!("Invalid message. Expected VaultStore, got MetaPassStore"))
            }
        }
    }

    pub async fn get_meta_pass_store(&self, vault_name: String) -> anyhow::Result<MetaPassStore> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(MetaDbRequestMessage::GetMetaPassStore { vault_name })
            .in_current_span()
            .await?;

        match msg {
            MetaDbResponseMessage::MetaPassStore { meta_pass_store } => Ok(meta_pass_store),
            _ => Err(anyhow!("Invalid message")),
        }
    }
}
