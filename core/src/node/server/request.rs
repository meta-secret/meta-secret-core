use crate::node::db::events::object_id::{ObjectId};
use serde::{Deserialize, Serialize};
use crate::node::db::meta_db::{MetaDb, VaultStore};

#[derive(Deserialize, Serialize)]
pub struct SyncRequest {
    pub vault: Option<VaultSyncRequest>,
    pub global_index: Option<ObjectId>,
}

#[derive(Deserialize, Serialize)]
pub struct VaultSyncRequest {
    pub tail_id: Option<ObjectId>,
}

impl From<&MetaDb> for SyncRequest {
    fn from(meta_db: &MetaDb) -> Self {
        let global_index = meta_db.global_index_store.tail_id;

        Self {
            vault: Some(VaultSyncRequest::from(&meta_db.vault_store)),
            global_index,
        }
    }
}

impl From<&VaultStore> for VaultSyncRequest {
    fn from(vault_store: &VaultStore) -> Self {
        Self {
            tail_id: vault_store.tail_id,
        }
    }
}
