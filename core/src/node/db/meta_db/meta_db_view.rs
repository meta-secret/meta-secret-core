use std::fmt::{Display, Formatter};
use std::sync::Arc;

use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::meta_db::store::{
    global_index_store::GlobalIndexStore,
    vault_store::VaultStore
};
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::logger::MetaLogger;

pub trait TailId {
    fn tail_id(&self) -> Option<ObjectId>;
}

pub struct MetaDb<Logger: MetaLogger> {
    pub id: String,
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
    pub meta_pass_store: MetaPassStore,
    pub logger: Arc<Logger>,
}

impl<Logger: MetaLogger> MetaDb<Logger> {
    pub fn new(id: String, logger: Arc<Logger>) -> Self {
        Self {
            id,
            vault_store: VaultStore::Empty,
            global_index_store: GlobalIndexStore::Empty,
            meta_pass_store: MetaPassStore::Empty,
            logger,
        }
    }

    /// Apply new events to the database
    pub fn apply(&mut self, commit_log: Vec<GenericKvLogEvent>) {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            self.apply_event(generic_event);
        }

        self.logger.debug(format!("Updated meta db: {}", self).as_str())
    }

    fn apply_event(&mut self, generic_event: &GenericKvLogEvent) {
        self.logger.debug(format!("Apply event: {:?}", generic_event).as_str());

        match generic_event {
            GenericKvLogEvent::GlobalIndex(gi_event) => {
                self.global_index_store.apply(gi_event);
            }
            GenericKvLogEvent::Vault(vault_obj_info) => {
                self.apply_vault_event(vault_obj_info);
            }
            GenericKvLogEvent::MetaPass(meta_pass_obj) => {
                self.apply_meta_pass_event(meta_pass_obj);
            }
            GenericKvLogEvent::Mempool(_) => {
                self.logger.info("Error. Mem pool events not for meta db");
                panic!("Internal mem pool event");
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
    }

    pub fn update_vault_info(&mut self, vault_name: &str) {
        let vault_unit_id = ObjectId::vault_unit(vault_name);

        if self.vault_store == VaultStore::Empty {
            self.vault_store = VaultStore::Unit {
                tail_id: vault_unit_id.clone(),
            }
        }

        if self.meta_pass_store == MetaPassStore::Empty {
            self.meta_pass_store = MetaPassStore::Unit {
                tail_id: ObjectId::meta_pass_unit(vault_name),
            }
        }
    }
}

impl<Logger: MetaLogger> Display for MetaDb<Logger> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(id: {}, vault: {:?}, gi: {:?}, meta pass: {:?})",
            self.id, self.vault_store, self.global_index_store, self.meta_pass_store
        )
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
    use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
    use crate::node::db::meta_db::meta_db_view::MetaDb;
    use crate::node::db::meta_db::store::global_index_store::GlobalIndexStore;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[test]
    fn test_apply_global_index_event() {
        let mut meta_db = MetaDb::new(
            String::from("test"),
            Arc::new(DefaultMetaLogger { id: LoggerId::Client }),
        );

        let s_box = KeyManager::generate_security_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);
        let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());

        meta_db.global_index_store.apply(&GlobalIndexObject::unit());

        let genesis_event = &GlobalIndexObject::genesis(&server_pk);
        meta_db.global_index_store.apply(genesis_event);

        let obj_id = &ObjectId::vault_unit("test_vault");
        let vault_id = IdStr::from(obj_id);

        match genesis_event.key() {
            KvKey::Empty { .. } => {
                panic!()
            }
            KvKey::Key { .. } => {
                meta_db.global_index_store.apply(&GlobalIndexObject::Update {
                    event: KvLogEvent::new_global_index_event(&obj_id.next(), &vault_id),
                });

                match meta_db.global_index_store {
                    GlobalIndexStore::Store { global_index, .. } => {
                        assert_eq!(1, global_index.len())
                    }
                    _ => panic!("Invalid state"),
                }
            }
        }
    }
}
