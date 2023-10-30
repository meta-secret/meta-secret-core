use std::sync::Arc;

use crate::node::common::data_transfer::MpscDataTransfer;
use anyhow::anyhow;
use tracing::{debug, info, instrument, Instrument};
use crate::node::common::model::vault::{VaultInfo, VaultName};

use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::read_db::read_db_view::{ReadDb, TailId};
use crate::node::db::read_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;

pub struct ReadDbService<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub repo: Arc<Repo>,
    pub read_db_id: String,
    pub data_transfer: Arc<ReadDbDataTransfer>,
}

pub struct ReadDbDataTransfer {
    pub dt: MpscDataTransfer<ReadDbRequestMessage, ReadDbResponseMessage>,
}

pub enum ReadDbRequestMessage {
    GetVaultInfo { vault_name: VaultName },
    GetVaultStore { vault_name: VaultName },
    GetMetaPassStore { vault_name: VaultName },
}

pub enum ReadDbResponseMessage {
    VaultInfo { vault: VaultInfo },
    VaultStore { vault_store: VaultStore },
    MetaPassStore { meta_pass_store: MetaPassStore },
}

impl<Repo: KvLogEventRepo> ReadDbService<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run read_db service");

        while let Ok(msg) = self.data_transfer.dt.service_receive().in_current_span().await {
            let mut read_db = ReadDb::new(self.read_db_id.clone());
            //Global index - FULL synchronization
            self.sync_read_db(&mut read_db).in_current_span().await;

            match msg {
                ReadDbRequestMessage::GetVaultInfo { vault_name } => {
                    read_db.update_vault_info(vault_name.as_str());
                    self.sync_read_db(&mut read_db).in_current_span().await;

                    let vault_unit_id = ObjectId::vault_unit(vault_name.as_str());
                    let vault_info = if read_db.global_index_store.contains(vault_unit_id.id_str()) {
                        //if the vault is already present:
                        match &read_db.vault_store {
                            VaultStore::Store { vault, .. } => VaultInfo::Member { vault: vault.clone() },
                            _ => VaultInfo::NotMember,
                        }
                    } else {
                        VaultInfo::NotFound
                    };

                    let response = ReadDbResponseMessage::VaultInfo { vault: vault_info };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
                ReadDbRequestMessage::GetVaultStore { vault_name } => {
                    read_db.update_vault_info(vault_name.as_str());
                    self.sync_read_db(&mut read_db).in_current_span().await;

                    let response = ReadDbResponseMessage::VaultStore {
                        vault_store: read_db.vault_store.clone(),
                    };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
                ReadDbRequestMessage::GetMetaPassStore { vault_name } => {
                    read_db.update_vault_info(vault_name.as_str());
                    self.sync_read_db(&mut read_db).in_current_span().await;

                    let response = ReadDbResponseMessage::MetaPassStore {
                        meta_pass_store: read_db.meta_pass_store.clone(),
                    };
                    let _ = self.data_transfer.dt.send_to_client(response).in_current_span().await;
                }
            }
        }
    }

    async fn sync_read_db(&self, read_db: &mut ReadDb) {
        debug!("Sync meta db");

        let vault_events = match read_db.vault_store.tail_id() {
            None => {
                vec![]
            }
            Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).in_current_span().await,
        };

        //sync global index
        let gi_events = {
            let maybe_gi_tail_id = read_db.global_index_store.tail_id();

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
            match read_db.meta_pass_store.tail_id() {
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

        read_db.apply(commit_log);
    }
}

pub struct ReadDbServiceProxy {
    pub dt: Arc<ReadDbDataTransfer>,
}

impl ReadDbServiceProxy {
    pub async fn get_vault_info(&self, vault_name: VaultName) -> anyhow::Result<VaultInfo> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(ReadDbRequestMessage::GetVaultInfo { vault_name })
            .in_current_span()
            .await?;

        match msg {
            ReadDbResponseMessage::VaultInfo { vault } => Ok(vault),
            _ => Err(anyhow!("Invalid message")),
        }
    }

    pub async fn get_vault_store(&self, vault_name: VaultName) -> anyhow::Result<VaultStore> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(ReadDbRequestMessage::GetVaultStore { vault_name })
            .in_current_span()
            .await?;

        match msg {
            ReadDbResponseMessage::VaultStore { vault_store } => Ok(vault_store),
            ReadDbResponseMessage::VaultInfo { .. } => {
                Err(anyhow!("Invalid message. Expected VaultStore, got VaultInfo"))
            }
            ReadDbResponseMessage::MetaPassStore { .. } => {
                Err(anyhow!("Invalid message. Expected VaultStore, got MetaPassStore"))
            }
        }
    }

    pub async fn get_meta_pass_store(&self, vault_name: VaultName) -> anyhow::Result<MetaPassStore> {
        let msg = self
            .dt
            .dt
            .send_to_service_and_get(ReadDbRequestMessage::GetMetaPassStore { vault_name })
            .in_current_span()
            .await?;

        match msg {
            ReadDbResponseMessage::MetaPassStore { meta_pass_store } => Ok(meta_pass_store),
            _ => Err(anyhow!("Invalid message")),
        }
    }
}
