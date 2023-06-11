use serde::{Serialize, Deserialize};
use crate::crypto::utils::to_id;
use crate::node::db::models::{MetaDb, VaultStore};

#[derive(Deserialize, Serialize)]
pub struct SyncRequest {
    pub vault: Option<VaultSyncRequest>,
    pub global_index: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct VaultSyncRequest {
    pub vault_id: Option<String>,
    pub tail_id: Option<String>,
}

impl From<&MetaDb> for SyncRequest {
    fn from(meta_db: &MetaDb) -> Self {
        let global_index = meta_db
            .global_index_store
            .tail_id.clone()
            .map(|tail_id| tail_id.key_id);

        Self {
            vault: Some(VaultSyncRequest::from(&meta_db.vault_store)),
            global_index,
        }
    }
}

impl From<&VaultStore> for VaultSyncRequest {
    fn from(vault_store: &VaultStore) -> Self {
        let vault_id = vault_store
            .vault.clone()
            .map(|vault| to_id(vault.vault_name.as_str()));

        Self {
            vault_id,
            tail_id: vault_store.tail_id.clone().map(|tail_id| tail_id.key_id),
        }
    }
}
