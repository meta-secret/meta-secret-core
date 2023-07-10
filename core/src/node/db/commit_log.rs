use std::error::Error;
use crate::models::VaultDoc;
use crate::node::db::meta_db::{MetaDb};
use crate::node::server::persistent_object::PersistentObject;

use crate::node::db::models::{GenericKvLogEvent, GlobalIndexObject, LogCommandError, LogEventKeyBasedRecord, VaultObject};
use crate::node::db::generic_db::KvLogEventRepo;
use std::rc::Rc;
use crate::node::server::meta_server::MetaLogger;


pub struct MetaDbManager<Repo: KvLogEventRepo<Err>, Err: Error> {
    pub persistent_obj: Rc<PersistentObject<Repo, Err>>,
}

impl<Repo: KvLogEventRepo<Err>, Err: Error> MetaDbManager<Repo, Err> {
    /// Apply new events to the database
    pub fn apply(&self, commit_log: Vec<GenericKvLogEvent>, mut meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            let mut vault_store = &mut meta_db.vault_store;
            let g_store = &mut meta_db.global_index_store;

            match generic_event {
                GenericKvLogEvent::GlobalIndex(gi_obj_info) => {
                    match gi_obj_info {
                        GlobalIndexObject::Unit { .. } => {
                            println!("Skip. Unit type doesn't contain any data needed for creating a vault object");
                        }
                        GlobalIndexObject::Genesis { event } => {
                            let server_pk = event.value.clone();
                            meta_db.global_index_store.tail_id = Some(event.key.obj_id.clone());
                            meta_db.global_index_store.server_pk = Some(server_pk)
                        }
                        GlobalIndexObject::Update { event } => {
                            let vault_id: String = event.value.vault_id.clone();
                            g_store.global_index.insert(vault_id);
                        }
                    }
                }
                GenericKvLogEvent::Vault(vault_obj_info) => {
                    match vault_obj_info {
                        VaultObject::Unit { .. } => {
                            println!("Skip. Unit type doesn't contain any data needed for creating a vault object");
                        }
                        VaultObject::Genesis { event } => {
                            let server_pk = event.value.clone();
                            meta_db.vault_store.tail_id = Some(event.key.obj_id.clone());
                            meta_db.vault_store.server_pk = Some(server_pk)
                        }
                        VaultObject::SignUpUpdate { event } => {
                            let vault: VaultDoc = event.value.clone();
                            vault_store.vault = Some(vault);
                            vault_store.tail_id = Some(event.key.obj_id.clone())
                        }
                        VaultObject::JoinUpdate { event } => {
                            let vault: VaultDoc = event.value.clone();
                            vault_store.vault = Some(vault);
                            vault_store.tail_id = Some(event.key.obj_id.clone())
                        }
                        VaultObject::JoinRequest { .. } => {
                            println!("Skip 'join' request");
                        }
                    }
                }
                GenericKvLogEvent::LocalEvent(_) => {
                    panic!("Internal event");
                }
                GenericKvLogEvent::Error { .. } => {
                    println!("Skip errors");
                }
            }
        }

        Ok(meta_db)
    }

    pub fn transform(&self, commit_log: Vec<GenericKvLogEvent>) -> Result<MetaDb, LogCommandError> {
        let meta_db = MetaDb::default();
        self.apply(commit_log, meta_db)
    }

    pub async fn sync_meta_db<L: MetaLogger>(&self, mut meta_db: MetaDb, logger: &L) -> Result<MetaDb, LogCommandError> {
        logger.log("Sync meta db");

        let tail_id = meta_db.vault_store.tail_id.clone();

        if let Some(key_id) = tail_id {
            let tail = self
                .persistent_obj
                .find_object_events(&key_id, logger).await;

            if let Some(latest_event) = tail.last() {
                meta_db.vault_store.tail_id = Some(latest_event.key().obj_id.clone());

                if let GenericKvLogEvent::Vault(VaultObject::SignUpUpdate { event }) = latest_event {
                    meta_db.vault_store.vault = Some(event.value.clone())
                }

                if let GenericKvLogEvent::Vault(VaultObject::JoinUpdate { event }) = latest_event {
                    meta_db.vault_store.vault = Some(event.value.clone())
                }
            }
        }

        Ok(meta_db)
    }
}
