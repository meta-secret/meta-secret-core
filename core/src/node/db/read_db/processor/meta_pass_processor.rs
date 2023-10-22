use crate::node::db::events::common::MetaPassObject;
use crate::node::db::read_db::read_db_view::ReadDb;
use crate::node::db::read_db::store::meta_pass_store::MetaPassStore;

use tracing::{debug, error};

impl ReadDb {
    pub fn apply_meta_pass_event(&mut self, meta_pass_obj: &MetaPassObject) {
        debug!("Apply meta pass event");

        let obj_id = meta_pass_obj.key().obj_id.clone();

        match meta_pass_obj {
            MetaPassObject::Unit { .. } => {
                self.meta_pass_store = match &self.meta_pass_store {
                    MetaPassStore::Empty => MetaPassStore::Unit { tail_id: obj_id },
                    MetaPassStore::Unit { .. } => MetaPassStore::Unit { tail_id: obj_id },
                    _ => {
                        let err_str = format!(
                            "Invalid state. Meta pass. Got a unit event, expected db state is Empty or Unit, actual: {:?}",
                            &self.meta_pass_store
                        );
                        error!(err_str);
                        panic!("Invalid state")
                    }
                }
            }
            MetaPassObject::Genesis { event } => {
                self.meta_pass_store = match &self.meta_pass_store {
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
                            self.meta_pass_store
                        );
                        error!(err_msg);
                        panic!("Invalid state")
                    }
                }
            }
            MetaPassObject::Update { event } => match self.meta_pass_store.clone() {
                MetaPassStore::Genesis { server_pk, .. } => {
                    let passwords = vec![event.value.clone()];

                    self.meta_pass_store = MetaPassStore::Store {
                        tail_id: obj_id,
                        server_pk: server_pk.clone(),
                        passwords,
                    };
                }
                MetaPassStore::Store { mut passwords, .. } => {
                    passwords.push(event.value.clone());
                }
                _ => {
                    let err_msg = format!(
                        "Invalid state. Meta Pass, update event. Actual state: {:?}, expected: genesis or store",
                        self.meta_pass_store
                    );
                    error!(err_msg);
                    panic!("Invalid state")
                }
            },
        }
    }
}
