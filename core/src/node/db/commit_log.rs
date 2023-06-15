use std::collections::HashSet;
use std::rc::Rc;

use crate::models::{Base64EncodedText, VaultDoc};
use crate::node::db::models::{
    AppOperation, AppOperationType, GlobalIndexStore, KvLogEvent, LogCommandError, MetaDb, ObjectType, VaultStore,
};

/// Apply new events to the database
pub fn apply(commit_log: Rc<Vec<KvLogEvent>>, mut meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
    for (_index, event) in commit_log.iter().enumerate() {
        let mut vault_store = &mut meta_db.vault_store;
        let g_store = &mut meta_db.global_index_store;

        match event.cmd_type {
            AppOperationType::Request(_op) => {
                println!("Skip requests");
            }

            AppOperationType::Update(op) => match op {
                AppOperation::ObjectFormation => {
                    let server_pk: Base64EncodedText = serde_json::from_value(event.value.clone()).unwrap();

                    match event.key.object_type {
                        ObjectType::Vault => {
                            meta_db.vault_store.server_pk = Some(server_pk);
                            meta_db.vault_store.tail_id = Some(event.key.key_id.clone())
                        }
                        ObjectType::GlobalIndex => {
                            meta_db.global_index_store.server_pk = Some(server_pk);
                            meta_db.global_index_store.tail_id = Some(event.key.key_id.clone())
                        }
                    }
                }
                AppOperation::SignUp => {
                    let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                    vault_store.vault = Some(vault);
                    vault_store.tail_id = Some(event.key.key_id.clone())
                }
                AppOperation::JoinCluster => {
                    let vault: VaultDoc = serde_json::from_value(event.value.clone()).unwrap();
                    vault_store.vault = Some(vault);
                    vault_store.tail_id = Some(event.key.key_id.clone())
                }
                AppOperation::GlobalIndex => {
                    let vault_id: String = serde_json::from_value(event.value.clone()).unwrap();
                    g_store.global_index.insert(vault_id);
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
        global_index_store: GlobalIndexStore {
            server_pk: None,
            tail_id: None,
            global_index: HashSet::new(),
        },
    };

    apply(commit_log, meta_db)
}

#[cfg(test)]
pub mod test {}
