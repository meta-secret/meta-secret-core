use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::meta_db::meta_db_view::MetaDb;
use crate::node::db::meta_db::store::vault_store::VaultStore;

use tracing::{debug, error, info};

impl MetaDb {
    pub fn apply_vault_event(&mut self, vault_obj: &VaultObject) {
        debug!("Apply vault event: {:?}", vault_obj);

        let KvKey::Key { obj_id, .. } = vault_obj.key().clone() else {
            panic!("Invalid event. Empty key")
        };

        match vault_obj {
            VaultObject::Unit { .. } => match self.vault_store {
                VaultStore::Empty => self.vault_store = VaultStore::Unit { tail_id: obj_id },
                VaultStore::Unit { .. } => self.vault_store = VaultStore::Unit { tail_id: obj_id },
                _ => {
                    let msg_str = format!("Unit event. Invalid vault store state: {:?}", self.vault_store);
                    error!(msg_str);
                }
            },
            VaultObject::Genesis { event } => {
                match &self.vault_store {
                    VaultStore::Unit { .. } => {
                        self.vault_store = VaultStore::Genesis {
                            tail_id: obj_id,
                            server_pk: event.value.clone(),
                        }
                    }
                    _ => {
                        let msg_error = format!("Genesis event. Invalid vault store state: {:?}", self.vault_store);
                        error!(msg_error);
                    }
                };
            }
            VaultObject::SignUpUpdate { event } => {
                match &self.vault_store {
                    VaultStore::Genesis { server_pk, .. } => {
                        self.vault_store = VaultStore::Store {
                            tail_id: obj_id,
                            server_pk: server_pk.clone(),
                            vault: event.value.clone(),
                        }
                    }
                    _ => {
                        let err_msg = format!("SignUp event. Invalid vault store state: {:?}", self.vault_store);
                        //panic!("{}", err_msg);
                    }
                };
            }
            VaultObject::JoinUpdate { event } => {
                match &self.vault_store {
                    VaultStore::Store { server_pk, .. } => {
                        self.vault_store = VaultStore::Store {
                            tail_id: obj_id,
                            server_pk: server_pk.clone(),
                            vault: event.value.clone(),
                        }
                    }
                    _ => {
                        let err_msg = format!("JoinUpdate event. Invalid vault store state: {:?}", self.vault_store);
                        info!(err_msg);
                    }
                };
            }
            VaultObject::JoinRequest { .. } => {
                match &self.vault_store {
                    VaultStore::Store { server_pk, vault, .. } => {
                        self.vault_store = VaultStore::Store {
                            tail_id: obj_id,
                            server_pk: server_pk.clone(),
                            vault: vault.clone(),
                        }
                    }
                    _ => {
                        let err_msg = format!("JoinRequest event. Invalid vault store state: {:?}", self.vault_store);
                        error!(err_msg);
                    }
                };
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::crypto::keys::KeyManager;
    use crate::models::DeviceInfo;
    use crate::node::db::events::object_id::ObjectId;
    use crate::node::db::events::vault_event::VaultObject;
    use crate::node::db::meta_db::meta_db_view::MetaDb;
    use crate::node::db::meta_db::store::vault_store::VaultStore;

    #[test]
    fn test() {
        let mut meta_db = MetaDb::new(String::from("test"));

        let vault_name = String::from("test_vault");
        let s_box = KeyManager::generate_security_box(vault_name.clone());
        let device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let user_sig = s_box.get_user_sig(&device);

        let vault_obj = VaultObject::unit(&user_sig);
        meta_db.apply_vault_event(&vault_obj);
        match meta_db.vault_store {
            VaultStore::Unit { tail_id } => {
                assert_eq!(ObjectId::vault_unit(vault_name.as_str()), tail_id);
            }
            _ => {
                panic!("Invalid state");
            }
        }
    }
}
