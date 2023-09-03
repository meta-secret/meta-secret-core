use crate::node::db::events::common::MetaPassObject;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;

use crate::node::db::events::object_id::ObjectId;
use crate::node::db::events::vault_event::VaultObject;
use std::rc::Rc;
use crate::node::db::events::kv_log_event::KvKey;

use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_view::{MetaDb, MetaPassStore, TailId, VaultStore};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;

pub struct MetaDbManager<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub persistent_obj: Rc<PersistentObject<Repo, Logger>>,
    pub repo: Rc<Repo>,
    pub logger: Rc<Logger>,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> From<Rc<PersistentObject<Repo, Logger>>> for MetaDbManager<Repo, Logger> {
    fn from(persistent_object: Rc<PersistentObject<Repo, Logger>>) -> Self {
        MetaDbManager {
            persistent_obj: persistent_object.clone(),
            repo: persistent_object.repo.clone(),
            logger: persistent_object.logger.clone(),
        }
    }
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> MetaDbManager<Repo, Logger> {
    pub async fn sync_meta_db(&self, meta_db: &mut MetaDb<Logger>) {
        self.logger.debug("Sync meta db");

        let vault_events = match meta_db.vault_store.tail_id() {
            None => {
                vec![]
            }
            Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).await,
        };

        //sync global index
        let gi_events = {
            let maybe_gi_tail_id = meta_db.global_index_store.tail_id();

            match maybe_gi_tail_id {
                None => {
                    self.persistent_obj
                        .find_object_events(&ObjectId::global_index_unit())
                        .await
                }
                Some(tail_id) => self
                    .persistent_obj
                    .find_object_events(&tail_id).await,
            }
        };

        let meta_pass_events = {
            match meta_db.meta_pass_store.tail_id() {
                None => {
                    vec![]
                }
                Some(tail_id) => self
                    .persistent_obj
                    .find_object_events(&tail_id).await,
            }
        };

        let mut commit_log = vec![];
        commit_log.extend(vault_events);
        commit_log.extend(gi_events);
        commit_log.extend(meta_pass_events);

        self.apply(commit_log, meta_db);
    }

    /// Apply new events to the database
    fn apply(&self, commit_log: Vec<GenericKvLogEvent>, meta_db: &mut MetaDb<Logger>) {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            self.apply_event(meta_db, generic_event);
        }

        self.logger.debug(format!("Updated meta db: {}", meta_db).as_str())
    }

    fn apply_event(&self, meta_db: MetaDb<Logger>, generic_event: &GenericKvLogEvent) -> MetaDb<Logger> {
        self.logger.debug(format!("Apply event: {:?}", generic_event).as_str());

        let mut new_meta_db = MetaDb {
            ..meta_db
        };

        match generic_event {
            GenericKvLogEvent::GlobalIndex(gi_event) => {
                new_meta_db = meta_db.apply_global_index_event(gi_event);
            }
            GenericKvLogEvent::Vault(vault_obj_info) => {
                new_meta_db = meta_db.apply_vault_event(vault_obj_info);
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => {
                self.apply_meta_pass_event(meta_db, meta_pass_obj);
            }
            GenericKvLogEvent::Mempool(_) => {
                self.logger.info("Error. Mempool events not for meta db");
                panic!("Internal mempool event");
            }
            GenericKvLogEvent::LocalEvent(_) => {
                self.logger.info("Error. LocalEvents not for sync");
                panic!("Internal event");
            }
            GenericKvLogEvent::SharedSecret(_) => {
                //not yet implemented
            }
            GenericKvLogEvent::Error { .. } => {
                self.logger.info("Skip. errors");
                println!("Skip errors");
            }
        }

        new_meta_db
    }

    fn apply_meta_pass_event(&self, meta_db: &mut MetaDb<Logger>, meta_pass_obj: &MetaPassObject) {
        self.logger.debug("Apply meta pass event");

        let obj_id = match meta_pass_obj.key().clone() {
            KvKey::Empty { .. } => {
                panic!("Invalid event. Empty key")
            }
            KvKey::Key { obj_id, .. } => {
                obj_id
            }
        };

        match meta_pass_obj {
            MetaPassObject::Unit { .. } => {
                meta_db.meta_pass_store = {
                    match &meta_db.meta_pass_store {
                        MetaPassStore::Empty => MetaPassStore::Unit {
                            tail_id: obj_id,
                        },
                        //TODO fix duplicate synchronization
                        MetaPassStore::Unit { .. } => MetaPassStore::Unit {
                            tail_id: obj_id,
                        },
                        _ => {
                            let err_str = format!(
                                "Invalid state. Meta pass. Got a unit event, expected db state is Empty or Unit, actual: {:?}",
                                &meta_db.meta_pass_store
                            );
                            self.logger.info(err_str.as_str());
                            panic!("Invalid state")
                        }
                    }
                }
            }
            MetaPassObject::Genesis { event } => {
                meta_db.meta_pass_store = {
                    match &meta_db.meta_pass_store {
                        MetaPassStore::Unit { .. } => MetaPassStore::Genesis {
                            tail_id: obj_id,
                            server_pk: event.value.clone(),
                        },

                        MetaPassStore::Genesis { .. } => MetaPassStore::Genesis {
                            tail_id: obj_id,
                            server_pk: event.value.clone(),
                        },
                        _ => {
                            let err_msg = format!(
                                "Invalid state. Meta Pass, genesis event. Actual: {:?}, expected: unit",
                                meta_db.meta_pass_store
                            );
                            self.logger.info(err_msg.as_str());
                            panic!("Invalid state")
                        }
                    }
                }
            }
            MetaPassObject::Update { event } => {
                meta_db.meta_pass_store = {
                    match meta_db.meta_pass_store.clone() {
                        MetaPassStore::Genesis { server_pk, .. } => {
                            let passwords = vec![event.value.clone()];

                            MetaPassStore::Store {
                                tail_id: obj_id,
                                server_pk,
                                passwords,
                            }
                        }
                        MetaPassStore::Store {
                            server_pk,
                            mut passwords,
                            ..
                        } => {
                            passwords.push(event.value.clone());
                            MetaPassStore::Store {
                                tail_id: obj_id,
                                server_pk,
                                passwords: passwords.clone(),
                            }
                        }
                        _ => {
                            let err_msg = format!(
                                "Invalid state. Meta Pass, update event. Actual state: {:?}, expected: genesis or store",
                                meta_db.meta_pass_store
                            );
                            self.logger.info(err_msg.as_str());
                            panic!("Invalid state")
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use std::rc::Rc;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[test]
    fn test() {
        let repo = Rc::new(InMemKvLogEventRepo::default());
        let logger = Rc::new(DefaultMetaLogger { id: LoggerId::Client });
        let persistent_object = Rc::new(PersistentObject::new(repo, logger));
        let _manager = MetaDbManager::from(persistent_object);
    }
}
