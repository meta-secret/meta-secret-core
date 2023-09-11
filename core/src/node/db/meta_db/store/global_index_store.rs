use std::collections::HashSet;

use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::meta_db::meta_db_view::TailId;

impl GlobalIndexStore {
    pub fn contains(&self, vault_id: String) -> bool {
        match self {
            GlobalIndexStore::Empty => false,
            GlobalIndexStore::Genesis { .. } => false,
            GlobalIndexStore::Store { global_index, .. } => global_index.contains(vault_id.as_str()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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

impl GlobalIndexStore {

    pub fn apply(&mut self, gi_event: &GlobalIndexObject) {
        let KvKey::Key { obj_id, .. } = gi_event.key() else {
            panic!("Invalid event. Empty key");
        };

        match self {
            GlobalIndexStore::Empty => {
                match gi_event {
                    GlobalIndexObject::Unit { .. } => {
                        // ignore
                    }
                    GlobalIndexObject::Genesis { event } => {
                        *self = GlobalIndexStore::Genesis {
                            tail_id: obj_id.clone(),
                            server_pk: event.value.clone(),
                        };
                    }
                    GlobalIndexObject::Update { .. } => {
                        panic!("Error: applying gi event: update. Invalid state: Empty. Must be Genesis or Store");
                    }
                }
            }
            GlobalIndexStore::Genesis { server_pk, .. } => {
                match gi_event {
                    GlobalIndexObject::Unit { .. } => {
                        panic!("Invalid event. Must be at least Genesis");
                    }
                    GlobalIndexObject::Genesis { .. } => {
                        let err_msg = String::from("Invalid event. Meta db already has Genesis");
                        panic!("{}", err_msg);
                    }
                    GlobalIndexObject::Update { event } => {
                        let mut global_index_set = HashSet::new();
                        global_index_set.insert(event.value.vault_id.clone());

                        *self = GlobalIndexStore::Store {
                            tail_id: obj_id.clone(),
                            server_pk: server_pk.clone(),
                            global_index: global_index_set,
                        };
                    }
                }
            }
            GlobalIndexStore::Store { global_index, tail_id, .. } => {
                match gi_event {
                    GlobalIndexObject::Unit { .. } => {
                        panic!("Invalid event: unit. MetaDb state is: store");
                    }
                    GlobalIndexObject::Genesis { .. } => {
                        panic!("Invalid event: genesis. MetaDb state is: store");
                    }
                    GlobalIndexObject::Update { event } => {
                        global_index.insert(event.value.vault_id.clone());
                        *tail_id = obj_id.clone();
                    }
                }
            }
        }
    }
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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::models::Base64EncodedText;
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::events::kv_log_event::KvLogEvent;
    use crate::node::db::events::object_id::{IdGen, IdStr, ObjectId};
    use crate::node::db::meta_db::meta_db_view::MetaDb;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[test]
    fn test_happy_case() {
        let mut meta_db = MetaDb::new(
            String::from("test"),
            Arc::new(DefaultMetaLogger::new(LoggerId::Test)),
        );

        let unit_event = GlobalIndexObject::unit();
        meta_db.global_index_store.apply(&unit_event);

        let genesis = GlobalIndexObject::genesis(&PublicKeyRecord::from(Base64EncodedText::from("test")));
        meta_db.global_index_store.apply(&genesis);

        let update = {
            let obj_id = &ObjectId::vault_unit("test_vault")
                .next()
                .next();
            let vault_id = IdStr::from(obj_id);

            GlobalIndexObject::Update {
                event: KvLogEvent::new_global_index_event(obj_id, &vault_id),
            }
        };
        meta_db.global_index_store.apply(&update);
        println!("{:?}", meta_db.global_index_store);
        assert!(meta_db.global_index_store.contains(String::from("Vault:test_vault::2")));
    }
}