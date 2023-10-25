use std::fmt::{Display, Formatter};

use tracing::{error, info};

use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::read_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::read_db::store::{global_index_store::GlobalIndexStore, vault_store::VaultStore};

pub trait TailId {
    fn tail_id(&self) -> Option<ObjectId>;
}

pub struct ReadDb {
    pub id: String,
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
    pub meta_pass_store: MetaPassStore,
}

impl ReadDb {
    pub fn new(id: String) -> Self {
        Self {
            id,
            vault_store: VaultStore::Empty,
            global_index_store: GlobalIndexStore::Empty,
            meta_pass_store: MetaPassStore::Empty,
        }
    }

    /// Apply new events to the database
    pub fn apply(&mut self, commit_log: Vec<GenericKvLogEvent>) {
        for (_index, generic_event) in commit_log.iter().enumerate() {
            self.apply_event(generic_event);
        }
    }

    fn apply_event(&mut self, generic_event: &GenericKvLogEvent) {
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
            GenericKvLogEvent::MemPool(_) => {
                info!("Error. Mem pool events not for meta db");
            }
            GenericKvLogEvent::SharedSecret(_) => {
                //not yet implemented
            }
            GenericKvLogEvent::Error { .. } => {
                info!("Skip. errors");
            }
            GenericKvLogEvent::DeviceCredentials(_) => {
                error!("Error. LocalEvents not for sync");
            }
            GenericKvLogEvent::DbTail(_) => {
                error!("Error. LocalEvents not for sync");
            }
        }
    }

    pub fn update_vault_info(&mut self, vault_name: &str) {
        let vault_unit_id = ObjectId::vault_unit(vault_name);

        if self.vault_store == VaultStore::Empty {
            self.vault_store = VaultStore::Unit {
                id: vault_unit_id.clone(),
            }
        }

        if self.meta_pass_store == MetaPassStore::Empty {
            self.meta_pass_store = MetaPassStore::Unit {
                tail_id: ObjectId::meta_pass_unit(vault_name),
            }
        }
    }
}

impl Display for ReadDb {
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
    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::generic_log_event::UnitEventEmptyValue;
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::events::kv_log_event::KvLogEvent;
    use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
    use crate::node::db::read_db::read_db_view::ReadDb;
    use crate::node::db::read_db::store::global_index_store::GlobalIndexStore;

    #[test]
    fn test_apply_global_index_event() {
        let mut read_db = ReadDb::new(String::from("test"));

        let s_box = KeyManager::generate_secret_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);
        let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());

        read_db.global_index_store.apply(&GlobalIndexObject::unit());

        let genesis_event = &GlobalIndexObject::genesis(&server_pk);
        read_db.global_index_store.apply(genesis_event);

        let obj_id = &ObjectId::vault_unit("test_vault");
        let vault_id = IdStr::from(obj_id);

        read_db.global_index_store.apply(&GlobalIndexObject::Update {
            event: KvLogEvent::new_global_index_event(&obj_id.next(), &vault_id),
        });

        match read_db.global_index_store {
            GlobalIndexStore::Store { global_index, .. } => {
                assert_eq!(1, global_index.len())
            }
            _ => panic!("Invalid state"),
        }
    }
}
