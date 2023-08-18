use crate::node::db::events::common::MetaPassObject;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;

use crate::node::db::events::vault_event::VaultObject;
use std::rc::Rc;

use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_view::{MetaDb, MetaPassStore, TailId, VaultStore};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::data_sync::MetaLogger;

pub struct MetaDbManager {
    pub persistent_obj: Rc<PersistentObject>,
    pub repo: Rc<dyn KvLogEventRepo>,
    pub logger: Rc<dyn MetaLogger>,
}

impl From<Rc<PersistentObject>> for MetaDbManager {
    fn from(persistent_object: Rc<PersistentObject>) -> Self {
        MetaDbManager {
            persistent_obj: persistent_object.clone(),
            repo: persistent_object.repo.clone(),
            logger: persistent_object.logger.clone(),
        }
    }
}

impl MetaDbManager {
    pub async fn sync_meta_db(&self, meta_db: &mut MetaDb) {
        let vault_events = match meta_db.vault_store.tail_id() {
            None => {
                vec![]
            }
            Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).await,
        };

        //sync global index
        let gi_events = {
            let maybe_gi_tail_id = &meta_db.global_index_store.tail_id();
            match maybe_gi_tail_id {
                None => {
                    vec![]
                }
                Some(tail_id) => self.persistent_obj.find_object_events(tail_id).await,
            }
        };

        let meta_pass_events = {
            match &meta_db.meta_pass_store.tail_id() {
                None => {
                    vec![]
                }
                Some(tail_id) => self.persistent_obj.find_object_events(tail_id).await,
            }
        };

        let mut commit_log = vec![];
        commit_log.extend(vault_events);
        commit_log.extend(gi_events);
        commit_log.extend(meta_pass_events);

        self.apply(commit_log, meta_db);
    }

    /// Apply new events to the database
    fn apply(&self, commit_log: Vec<GenericKvLogEvent>, meta_db: &mut MetaDb) {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            self.apply_event(meta_db, generic_event);
        }
    }

    fn apply_event(&self, meta_db: &mut MetaDb, generic_event: &GenericKvLogEvent) {
        self.logger.info(format!("Apply event: ").as_str());

        match generic_event {
            GenericKvLogEvent::GlobalIndex(gi_event) => {
                meta_db.apply_global_index_event(gi_event);
            }
            GenericKvLogEvent::Vault(vault_obj_info) => {
                self.apply_vault_event(meta_db, vault_obj_info);
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
            GenericKvLogEvent::SecretShare(_) => {
                //not yet implemented
            }
            GenericKvLogEvent::Error { .. } => {
                self.logger.info("Skip. errors");
                println!("Skip errors");
            }
        }
    }

    fn apply_vault_event(&self, meta_db: &mut MetaDb, vault_obj: &VaultObject) {
        match vault_obj {
            VaultObject::Unit { event } => match &meta_db.vault_store {
                VaultStore::Empty => {
                    meta_db.vault_store = VaultStore::Unit {
                        tail_id: event.key.obj_id.clone(),
                    }
                }
                VaultStore::Unit { .. } => {
                    meta_db.vault_store = VaultStore::Unit {
                        tail_id: event.key.obj_id.clone(),
                    }
                }
                _ => {
                    self.logger
                        .info(format!("Unit event. Invalid vault store state: {:?}", &meta_db.vault_store).as_str());
                }
            },
            VaultObject::Genesis { event } => {
                match &meta_db.vault_store {
                    VaultStore::Unit { .. } => {
                        meta_db.vault_store = VaultStore::Genesis {
                            tail_id: event.key.obj_id.clone(),
                            server_pk: event.value.clone(),
                        }
                    }
                    _ => {
                        self.logger.info(
                            format!("Genesis event. Invalid vault store state: {:?}", &meta_db.vault_store).as_str(),
                        );
                    }
                };
            }
            VaultObject::SignUpUpdate { event } => {
                match &meta_db.vault_store {
                    VaultStore::Genesis { server_pk, .. } => {
                        meta_db.vault_store = VaultStore::Store {
                            tail_id: event.key.obj_id.clone(),
                            server_pk: server_pk.clone(),
                            vault: event.value.clone(),
                        }
                    }
                    _ => {
                        self.logger.debug(
                            format!("SignUp event. Invalid vault store state: {:?}", &meta_db.vault_store).as_str(),
                        );
                    }
                };
            }
            VaultObject::JoinUpdate { event } => {
                match &meta_db.vault_store {
                    VaultStore::Store { server_pk, .. } => {
                        meta_db.vault_store = VaultStore::Store {
                            tail_id: event.key.obj_id.clone(),
                            server_pk: server_pk.clone(),
                            vault: event.value.clone(),
                        }
                    }
                    _ => {
                        self.logger.info(
                            format!(
                                "JoinUpdate event. Invalid vault store state: {:?}",
                                &meta_db.vault_store
                            )
                            .as_str(),
                        );
                    }
                };
            }
            VaultObject::JoinRequest { event } => {
                match &meta_db.vault_store {
                    VaultStore::Store { server_pk, vault, .. } => {
                        meta_db.vault_store = VaultStore::Store {
                            tail_id: event.key.obj_id.clone(),
                            server_pk: server_pk.clone(),
                            vault: vault.clone(),
                        }
                    }
                    _ => {
                        self.logger.info(
                            format!(
                                "JoinRequest event. Invalid vault store state: {:?}",
                                &meta_db.vault_store
                            )
                            .as_str(),
                        );
                    }
                };
            }
        }
    }

    fn apply_meta_pass_event(&self, meta_db: &mut MetaDb, meta_pass_obj: &MetaPassObject) {
        match meta_pass_obj {
            MetaPassObject::Unit { event } => {
                meta_db.meta_pass_store = match &meta_db.meta_pass_store {
                    MetaPassStore::Empty => MetaPassStore::Unit {
                        tail_id: event.key.obj_id.clone(),
                    },
                    //TODO fix duplicate synchronization
                    MetaPassStore::Unit { .. } => MetaPassStore::Unit {
                        tail_id: event.key.obj_id.clone(),
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
            MetaPassObject::Genesis { event } => {
                meta_db.meta_pass_store = match &meta_db.meta_pass_store {
                    MetaPassStore::Unit { .. } => MetaPassStore::Genesis {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: event.value.clone(),
                    },
                    //TODO fix duplicate synchronization
                    MetaPassStore::Genesis { .. } => MetaPassStore::Genesis {
                        tail_id: event.key.obj_id.clone(),
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
            MetaPassObject::Update { event } => {
                meta_db.meta_pass_store = match meta_db.meta_pass_store.clone() {
                    MetaPassStore::Genesis { server_pk, .. } => {
                        let passwords = vec![event.value.clone()];

                        MetaPassStore::Store {
                            tail_id: event.key.obj_id.clone(),
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
                            tail_id: event.key.obj_id.clone(),
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
                };
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::server::data_sync::{DefaultMetaLogger, LoggerId};
    use std::rc::Rc;

    #[test]
    fn test() {
        let repo = Rc::new(InMemKvLogEventRepo::default());
        let logger = Rc::new(DefaultMetaLogger { id: LoggerId::Client });
        let persistent_object = Rc::new(PersistentObject::new(repo, logger));
        let _manager = MetaDbManager::from(persistent_object);
    }
}
