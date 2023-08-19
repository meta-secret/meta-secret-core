use std::collections::HashSet;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::models::vault_doc::VaultDoc;
use crate::models::MetaPasswordDoc;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::object_id::ObjectId;
use crate::node::server::data_sync::MetaLogger;
use std::rc::Rc;

pub struct MetaDb {
    pub id: String,
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
    pub meta_pass_store: MetaPassStore,
    pub logger: Rc<dyn MetaLogger>,
}

impl Display for MetaDb {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(id: {}, vault: {:?}, gi: {:?}, meta pass: {:?})",
            self.id, self.vault_store, self.global_index_store, self.meta_pass_store
        )
    }
}

impl MetaDb {
    pub fn new(id: String, logger: Rc<dyn MetaLogger>) -> Self {
        Self {
            id,
            vault_store: VaultStore::Empty,
            global_index_store: GlobalIndexStore::Empty,
            meta_pass_store: MetaPassStore::Empty,
            logger,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VaultStore {
    Empty,
    Unit {
        tail_id: ObjectId,
    },
    Genesis {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
    },
    Store {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
        vault: VaultDoc,
    },
}

pub trait TailId {
    fn tail_id(&self) -> Option<ObjectId>;
}

impl TailId for VaultStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            VaultStore::Empty => None,
            VaultStore::Unit { tail_id } => Some(tail_id.clone()),
            VaultStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            VaultStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GlobalIndexStore {
    Empty,
    Genesis {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
    },
    Store {
        server_pk: PublicKeyRecord,
        tail_id: ObjectId,
        global_index: HashSet<String>,
    },
}

impl TailId for GlobalIndexStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            GlobalIndexStore::Empty => None,
            GlobalIndexStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            GlobalIndexStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}

impl GlobalIndexStore {
    pub fn contains(&self, vault_id: String) -> bool {
        match self {
            GlobalIndexStore::Empty => false,
            GlobalIndexStore::Genesis { .. } => false,
            GlobalIndexStore::Store { global_index, .. } => global_index.contains(vault_id.as_str()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MetaPassStore {
    Empty,
    Unit {
        tail_id: ObjectId,
    },
    Genesis {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
    },
    Store {
        tail_id: ObjectId,
        server_pk: PublicKeyRecord,
        passwords: Vec<MetaPasswordDoc>,
    },
}

impl MetaPassStore {
    pub fn passwords(&self) -> Vec<MetaPasswordDoc> {
        match self {
            MetaPassStore::Empty => {
                vec![]
            }
            MetaPassStore::Unit { .. } => {
                vec![]
            }
            MetaPassStore::Genesis { .. } => {
                vec![]
            }
            MetaPassStore::Store { passwords, .. } => passwords.clone(),
        }
    }
}

impl TailId for MetaPassStore {
    fn tail_id(&self) -> Option<ObjectId> {
        match self {
            MetaPassStore::Empty => None,
            MetaPassStore::Unit { tail_id } => Some(tail_id.clone()),
            MetaPassStore::Genesis { tail_id, .. } => Some(tail_id.clone()),
            MetaPassStore::Store { tail_id, .. } => Some(tail_id.clone()),
        }
    }
}

impl MetaDb {
    pub fn apply_global_index_event(&mut self, gi_event: &GlobalIndexObject) {
        self.logger
            .debug(format!("Apply global index event: {:?}", gi_event).as_str());

        let gi_store = self.global_index_store.clone();
        match gi_store {
            GlobalIndexStore::Empty => {
                match gi_event {
                    GlobalIndexObject::Unit { .. } => {
                        //nothing to do
                    }
                    GlobalIndexObject::Genesis { event } => {
                        self.global_index_store = GlobalIndexStore::Genesis {
                            tail_id: event.key.obj_id.clone(),
                            server_pk: event.value.clone(),
                        }
                    }
                    GlobalIndexObject::Update { .. } => {
                        self.logger
                            .error("Error: applying gi event: update. Invalid state: Empty. Must be Genesis or Store");
                        panic!("Invalid state");
                    }
                }
            }
            GlobalIndexStore::Genesis { server_pk, .. } => match gi_event {
                GlobalIndexObject::Unit { .. } => {
                    self.logger.error("Invalid event. Must be at least Genesis");
                    panic!("Invalid state");
                }
                GlobalIndexObject::Genesis { .. } => {
                    self.logger.error("Invalid event. Meta db is already has Genesis");
                    panic!("Invalid state");
                }
                GlobalIndexObject::Update { event } => {
                    let mut global_index_set = HashSet::new();
                    global_index_set.insert(event.value.vault_id.clone());

                    self.global_index_store = GlobalIndexStore::Store {
                        tail_id: event.key.obj_id.clone(),
                        server_pk: server_pk.clone(),
                        global_index: global_index_set,
                    }
                }
            },
            GlobalIndexStore::Store { mut global_index, .. } => match gi_event {
                GlobalIndexObject::Unit { .. } => {
                    self.logger.error("Invalid event: unit. MetaDb state is: store");
                    panic!("Invalid event");
                }
                GlobalIndexObject::Genesis { .. } => {
                    self.logger.error("Invalid event: genesis. MetaDb state is: store");
                    panic!("Invalid event");
                }
                GlobalIndexObject::Update { event } => {
                    global_index.insert(event.value.vault_id.clone());
                }
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::events::kv_log_event::KvLogEvent;
    use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
    use crate::node::db::meta_db::meta_db_view::{GlobalIndexStore, MetaDb};
    use crate::node::server::data_sync::{DefaultMetaLogger, LoggerId};
    use std::rc::Rc;

    #[test]
    fn test_appy_global_index_event() {
        let mut meta_db = MetaDb::new(
            String::from("test"),
            Rc::new(DefaultMetaLogger { id: LoggerId::Client }),
        );

        let s_box = KeyManager::generate_security_box("test_vault".to_string());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);
        let server_pk = PublicKeyRecord::from(user_sig.public_key.as_ref().clone());

        meta_db.apply_global_index_event(&GlobalIndexObject::unit());

        let genesis_event = &GlobalIndexObject::genesis(&server_pk);
        meta_db.apply_global_index_event(genesis_event);

        let obj_id = &ObjectId::vault_unit("test_vault");
        let vault_id = IdStr::from(obj_id);

        meta_db.apply_global_index_event(&GlobalIndexObject::Update {
            event: KvLogEvent::new_global_index_event(&genesis_event.key().obj_id.next(), &vault_id),
        });

        match meta_db.global_index_store {
            GlobalIndexStore::Store { global_index, .. } => {
                assert_eq!(1, global_index.len())
            }
            _ => panic!("Invalid state"),
        }
    }
}
