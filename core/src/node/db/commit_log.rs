use std::error::Error;
use std::rc::Rc;

use crate::models::VaultDoc;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::MetaDb;
use crate::node::db::models::{
    GenericKvLogEvent, GlobalIndexObject, LogCommandError, LogEventKeyBasedRecord, VaultObject,
};
use crate::node::server::data_sync::MetaLogger;
use crate::node::server::persistent_object::PersistentObject;

pub struct MetaDbManager<Repo: KvLogEventRepo<Err>, Err: Error> {
    pub persistent_obj: Rc<PersistentObject<Repo, Err>>,
    pub repo: Rc<Repo>,
}

impl<Repo: KvLogEventRepo<Err>, Err: Error> MetaDbManager<Repo, Err> {
    /// Apply new events to the database
    pub fn apply(&self, commit_log: Vec<GenericKvLogEvent>, mut meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            let mut vault_store = &mut meta_db.vault_store;
            let g_store = &mut meta_db.global_index_store;

            match generic_event {
                GenericKvLogEvent::GlobalIndex(gi_obj_info) => match gi_obj_info {
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
                },
                GenericKvLogEvent::Vault(vault_obj_info) => match vault_obj_info {
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
                },
                GenericKvLogEvent::Mempool(_) => {
                    panic!("Internal mempool event");
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

    pub async fn sync_meta_db<L: MetaLogger>(&self, mut meta_db: MetaDb, logger: &L) -> MetaDb {
        //logger.log("Sync meta db");

        let maybe_vault_tail_id = meta_db.vault_store.tail_id.clone();

        let vault_events = match maybe_vault_tail_id {
            None => {
                vec![]
            }
            Some(vault_tail_id) => self.persistent_obj.find_object_events(&vault_tail_id, logger).await,
        };

        for curr_event in vault_events {
            match &curr_event {
                GenericKvLogEvent::GlobalIndex(_) => {
                    panic!("Invalid state");
                }

                GenericKvLogEvent::Vault(vault_obj) => {
                    meta_db.vault_store.tail_id = Some(curr_event.key().obj_id.clone());

                    match vault_obj {
                        VaultObject::Unit { .. } => {
                            //ignore
                        }
                        VaultObject::Genesis { event } => {
                            meta_db.vault_store.server_pk = Some(event.value.clone());
                        }
                        VaultObject::SignUpUpdate { event } => {
                            meta_db.vault_store.vault = Some(event.value.clone());
                        }
                        VaultObject::JoinUpdate { event } => {
                            meta_db.vault_store.vault = Some(event.value.clone());
                        }
                        VaultObject::JoinRequest { .. } => {
                            //ignore
                        }
                    }
                }
                GenericKvLogEvent::Mempool(_) => {
                    panic!("Invalid state");
                }
                GenericKvLogEvent::LocalEvent(_) => {
                    panic!("Invalid state");
                }
                GenericKvLogEvent::Error { .. } => {
                    panic!("Invalid state");
                }
            }
        }

        //sync global index
        let maybe_gi_tail_id = meta_db.global_index_store.tail_id.clone();
        let gi_tail_id = maybe_gi_tail_id.unwrap_or(ObjectId::global_index_unit());

        let gi_events = self.persistent_obj.find_object_events(&gi_tail_id, logger).await;

        for gi_event in gi_events {
            match &gi_event {
                GenericKvLogEvent::GlobalIndex(gi_obj) => {
                    meta_db.global_index_store.tail_id = Some(gi_event.key().obj_id.clone());

                    match gi_obj {
                        GlobalIndexObject::Unit { .. } => {
                            //ignore
                        }
                        GlobalIndexObject::Genesis { event } => {
                            meta_db.global_index_store.server_pk = Some(event.value.clone());
                        }
                        GlobalIndexObject::Update { event } => {
                            meta_db
                                .global_index_store
                                .global_index
                                .insert(event.value.vault_id.clone());
                        }
                    }
                }
                _ => {
                    panic!("Invalid state");
                }
            }
        }

        meta_db
    }
}
