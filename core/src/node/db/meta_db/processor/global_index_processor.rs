use std::collections::HashSet;

use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::meta_db::meta_db_view::MetaDb;
use crate::node::db::meta_db::store::global_index_store::GlobalIndexStore;
use crate::node::logger::MetaLogger;

impl<Logger: MetaLogger> MetaDb<Logger> {
    pub fn apply_global_index_event(&mut self, gi_event: &GlobalIndexObject) {
        self.logger
            .debug(format!("Apply global index event: {:?}", gi_event).as_str());

        let gi_store = self.global_index_store.clone();

        let KvKey::Key { obj_id, .. } = gi_event.key() else {
            panic!("Invalid event. Empty key");
        };

        match gi_store {
            GlobalIndexStore::Empty => {
                match gi_event {
                    GlobalIndexObject::Unit { .. } => {
                        // ignore
                    }
                    GlobalIndexObject::Genesis { event } => {
                        self.global_index_store = GlobalIndexStore::Genesis {
                            tail_id: obj_id.clone(),
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
                    let err_msg = String::from("Invalid event. Meta db already has Genesis");
                    self.logger.error(err_msg.as_str());
                    panic!("{}", err_msg);
                }
                GlobalIndexObject::Update { event } => {
                    let mut global_index_set = HashSet::new();
                    global_index_set.insert(event.value.vault_id.clone());

                    self.global_index_store = GlobalIndexStore::Store {
                        tail_id: obj_id.clone(),
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
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;

    use crate::node::db::events::global_index::GlobalIndexObject;
    use crate::node::db::meta_db::meta_db_view::MetaDb;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[test]
    fn test() {
        let mut meta_db = MetaDb::new(
            String::from("test"),
            Rc::new(DefaultMetaLogger::new(LoggerId::Test)),
        );

        let gi_event = GlobalIndexObject::unit();
        let new_meta_db = meta_db.apply_global_index_event(&gi_event);
    }
}