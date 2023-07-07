use std::error::Error;
use crate::models::VaultDoc;
use crate::node::db::meta_db::{MetaDb};
use crate::node::server::persistent_object::PersistentObject;

use crate::node::db::models::{GenericKvLogEvent, KvLogEventUpdate, LogCommandError, LogEventKeyBasedRecord, ObjectType};
use crate::node::db::generic_db::KvLogEventRepo;
use std::rc::Rc;

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

                GenericKvLogEvent::Request(_) => {
                    println!("Skip requests");
                }

                GenericKvLogEvent::LocalEvent(_) => {
                    panic!("Internal event");
                }

                GenericKvLogEvent::Error { .. } => {
                    println!("Skip errors");
                }

                GenericKvLogEvent::Update(op) => match op {

                    KvLogEventUpdate::Genesis { event } => {
                        let server_pk = event.value.clone();

                        match event.key.object_type {
                            ObjectType::VaultObj => {
                                meta_db.vault_store.tail_id = Some(event.key.obj_id.clone());
                                meta_db.vault_store.server_pk = Some(server_pk)
                            }
                            ObjectType::GlobalIndexObj => {
                                meta_db.global_index_store.tail_id = Some(event.key.obj_id.clone());
                                meta_db.global_index_store.server_pk = Some(server_pk)
                            }
                            _ => {
                                panic!("Object type is not alowed to be used here: {:?}", event.key.object_type)
                            }
                        }
                    }

                    KvLogEventUpdate::GlobalIndex { event } => {
                        let vault_id: String = event.value.vault_id.clone();
                        g_store.global_index.insert(vault_id);
                    }

                    KvLogEventUpdate::SignUp { event } => {
                        let vault: VaultDoc = event.value.clone();
                        vault_store.vault = Some(vault);
                        vault_store.tail_id = Some(event.key.obj_id.clone())
                    }

                    KvLogEventUpdate::JoinCluster { event } => {
                        let vault: VaultDoc = event.value.clone();
                        vault_store.vault = Some(vault);
                        vault_store.tail_id = Some(event.key.obj_id.clone())
                    }
                },
            }
        }

        Ok(meta_db)
    }

    pub fn transform(&self, commit_log: Vec<GenericKvLogEvent>) -> Result<MetaDb, LogCommandError> {
        let meta_db = MetaDb::default();
        self.apply(commit_log, meta_db)
    }

    pub async fn sync_meta_db(&self, mut meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
        let tail_id = meta_db.vault_store.tail_id.clone();

        if let Some(key_id) = tail_id {
            let tail = self
                .persistent_obj
                .find_object_events(&key_id).await;

            if let Some(latest_event) = tail.last() {
                meta_db.vault_store.tail_id = Some(latest_event.key().obj_id.clone());

                if let GenericKvLogEvent::Update(KvLogEventUpdate::SignUp { event }) = latest_event {
                    meta_db.vault_store.vault = Some(event.value.clone())
                }

                if let GenericKvLogEvent::Update(KvLogEventUpdate::JoinCluster { event }) = latest_event {
                    meta_db.vault_store.vault = Some(event.value.clone())
                }
            }
        }

        Ok(meta_db)
    }
}
