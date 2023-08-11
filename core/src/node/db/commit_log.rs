use std::rc::Rc;

use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::{MetaDb, MetaPassStore, TailId, VaultStore};
use crate::node::db::models::{
    GenericKvLogEvent, GlobalIndexObject, LogEventKeyBasedRecord, MetaPassObject, VaultObject,
};
use crate::node::db::persistent_object::PersistentObject;
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
            let gi_tail_id = &meta_db.global_index_store.tail_id;
            self.persistent_obj.find_object_events(gi_tail_id).await
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
        match generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                self.apply_global_index_event(meta_db, generic_event);
            }
            GenericKvLogEvent::Vault(vault_obj_info) => {
                self.apply_vault_event(meta_db, vault_obj_info);
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => {
                self.apply_meta_pass_event(meta_db, meta_pass_obj);
            }
            GenericKvLogEvent::Mempool(_) => {
                self.logger.log("Error. Mempool events not for meta db");
                panic!("Internal mempool event");
            }
            GenericKvLogEvent::LocalEvent(_) => {
                self.logger.log("Error. LocalEvents not for sync");
                panic!("Internal event");
            }
            GenericKvLogEvent::Error { .. } => {
                self.logger.log("Skip. errors");
                println!("Skip errors");
            }
        }
    }

    fn apply_vault_event(&self, meta_db: &mut MetaDb, vault_obj: &VaultObject) {
        match vault_obj {
            VaultObject::Unit { event } => {
                meta_db.vault_store = match &meta_db.vault_store {
                    VaultStore::Empty => VaultStore::Unit {
                        tail_id: event.key.obj_id.clone(),
                    },
                    VaultStore::Unit { .. } => VaultStore::Unit {
                        tail_id: event.key.obj_id.clone(),
                    },
                    _ => {
                        self.logger
                            .log(format!("Invalid vault store state: {:?}", &meta_db.vault_store).as_str());
                        panic!("Invalid state")
                    }
                }
            }
            VaultObject::Genesis { event } => {
                meta_db.vault_store = match &meta_db.vault_store {
                    VaultStore::Unit { .. } => VaultStore::Genesis {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: event.value.clone(),
                    },
                    _ => {
                        panic!("Invalid state")
                    }
                };
            }
            VaultObject::SignUpUpdate { event } => {
                meta_db.vault_store = match &meta_db.vault_store {
                    VaultStore::Genesis { server_pk, .. } => VaultStore::Store {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: server_pk.clone(),
                        vault: event.value.clone(),
                    },
                    _ => {
                        panic!("Invalid state")
                    }
                };
            }
            VaultObject::JoinUpdate { event } => {
                meta_db.vault_store = match &meta_db.vault_store {
                    VaultStore::Store { server_pk, .. } => VaultStore::Store {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: server_pk.clone(),
                        vault: event.value.clone(),
                    },
                    _ => {
                        panic!("Invalid state")
                    }
                };
            }
            VaultObject::JoinRequest { event } => {
                meta_db.vault_store = match &meta_db.vault_store {
                    VaultStore::Store { server_pk, vault, .. } => VaultStore::Store {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: server_pk.clone(),
                        vault: vault.clone(),
                    },
                    _ => {
                        panic!("Invalid state")
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
                        self.logger.log(err_str.as_str());
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
                        self.logger.log(err_msg.as_str());
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
                        self.logger.log(err_msg.as_str());
                        panic!("Invalid state")
                    }
                };
            }
        }
    }

    fn apply_global_index_event(&self, meta_db: &mut MetaDb, gi_event: &GenericKvLogEvent) {
        match gi_event {
            GenericKvLogEvent::GlobalIndex(gi_obj) => {
                meta_db.global_index_store.tail_id = gi_event.key().obj_id.clone();

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
}
