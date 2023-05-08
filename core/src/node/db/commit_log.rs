use std::collections::HashSet;
use std::rc::Rc;

use crate::models::{Base64EncodedText, VaultDoc};
use crate::node::db::events::global_index::generate_global_index_formation_key_id;
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, LogCommandError, MetaDb, GlobalIndexStore, VaultStore};

pub mod store_names {
    pub const GLOBAL_OBJECT_ID: &str = "meta-secret:global";

    /// An instance of a vault of the user
    pub const VAULT: &str = "vault";
    pub const GLOBAL_IDX: &str = "global_idx";
}

pub fn generate_key(store_name: &str, curr_id: &KvKeyId, vault_id: Option<String>) -> KvKey {
    KvKey {
        store: store_name.to_string(),
        id: curr_id.next(),
        vault_id,
    }
}

pub fn generate_next(prev_key: &KvKey) -> KvKey {
    KvKey {
        id: prev_key.id.next(),
        store: prev_key.store.clone(),
        vault_id: prev_key.vault_id.clone(),
    }
}

/// Apply new events to the database
pub fn apply(commit_log: Rc<Vec<KvLogEvent>>, mut meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
    for (_index, event) in commit_log.iter().enumerate() {
        let mut meta_store = &mut meta_db.vault_store;
        let vaults_store = &mut meta_db.vaults;

        match event.cmd_type {
            AppOperationType::Request(_op) => {
                println!("Skip requests");
            }

            AppOperationType::Update(op) => match op {
                AppOperation::VaultFormation => {
                    let server_pk: Base64EncodedText = serde_json::from_value(event.value.clone()).unwrap();
                    meta_db.vault_store.server_pk = Some(server_pk);
                }
                AppOperation::SignUp => {
                    let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                    meta_store.vault = Some(vault);
                }
                AppOperation::JoinCluster => {
                    let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                    meta_store.vault = Some(vault);
                }
                AppOperation::GlobalIndexx => {
                    if event.key.id.key_id != generate_global_index_formation_key_id().key_id {
                        let vault_id: String = serde_json::from_value(event.value.clone()).unwrap();
                        vaults_store.global_index.insert(vault_id);
                    } else {
                        // validate server signature of a formation block
                    }
                }
            },
        }
    }

    Ok(meta_db)
}

pub fn transform(commit_log: Rc<Vec<KvLogEvent>>) -> Result<MetaDb, LogCommandError> {
    let meta_db = MetaDb {
        vault_store: VaultStore {
            tail_id: None,
            server_pk: None,
            vault: None,
        },
        vaults: GlobalIndexStore {
            tail_id: None,
            global_index: HashSet::new(),
        },
    };

    apply(commit_log, meta_db)
}

#[cfg(test)]
pub mod test {}
