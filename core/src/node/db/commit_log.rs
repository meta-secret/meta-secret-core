use std::collections::HashSet;
use std::rc::Rc;

use crate::models::{Base64EncodedText, VaultDoc};
use crate::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, LogCommandError, MetaDb, MetaStore, VaultsStore};

pub mod store_names {
    pub const GENESIS: &str = "genesis";
    pub const COMMIT_LOG: &str = "commit_log";
    /// An instance of a vault of the user
    pub const USER_VAULT: &str = "user_vault";
    pub const VAULTS_IDX: &str = "vaults_idx";
}

pub fn generate_commit_log_key(curr_id: &KvKeyId, vault_id: Option<String>) -> KvKey {
    generate_key(store_names::COMMIT_LOG, curr_id, vault_id)
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
    for (index, event) in commit_log.iter().enumerate() {
        let mut meta_store = &mut meta_db.meta_store;
        let vaults_store = &mut meta_db.vaults;

        match index {
            0 => {
                if event.cmd_type == AppOperationType::Update(AppOperation::Genesis) {
                    let server_pk: Base64EncodedText = serde_json::from_value(event.value.clone()).unwrap();
                    meta_db.meta_store.server_pk = Some(server_pk);
                } else {
                    return Err(LogCommandError::IllegalDbState {
                        err_msg: "Missing genesis event".to_string(),
                    });
                }
            }
            _ => match event.cmd_type {
                AppOperationType::Request(_op) => {
                    println!("Skip requests");
                }

                AppOperationType::Update(op) => match op {
                    AppOperation::Genesis => {}
                    AppOperation::SignUp => {
                        let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                        meta_store.vault = Some(vault);
                    }
                    AppOperation::JoinCluster => {
                        let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                        meta_store.vault = Some(vault);
                    }
                    AppOperation::VaultsIndex => {
                        let vault_id: String = serde_json::from_value(event.value.clone()).unwrap();
                        vaults_store.vaults_index.insert(vault_id);
                    }
                },
            },
        }
    }

    Ok(meta_db)
}

pub fn transform(commit_log: Rc<Vec<KvLogEvent>>) -> Result<MetaDb, LogCommandError> {
    let meta_db = MetaDb {
        meta_store: MetaStore {
            tail_id: None,
            server_pk: None,
            vault: None,
        },
        vaults: VaultsStore {
            tail_id: None,
            vaults_index: HashSet::new()
        },
    };

    apply(commit_log, meta_db)
}

#[cfg(test)]
pub mod test {

}
