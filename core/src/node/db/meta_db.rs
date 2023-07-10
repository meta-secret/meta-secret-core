use crate::models::vault_doc::VaultDoc;
use crate::node::db::models::{PublicKeyRecord};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::node::db::events::object_id::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
}

impl Default for MetaDb {
    fn default() -> Self {
        Self {
            vault_store: VaultStore {
                tail_id: None,
                server_pk: None,
                vault: None,
            },
            global_index_store: GlobalIndexStore {
                server_pk: None,
                tail_id: None,
                global_index: HashSet::new(),
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStore {
    pub tail_id: Option<ObjectId>,
    pub server_pk: Option<PublicKeyRecord>,
    pub vault: Option<VaultDoc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexStore {
    pub server_pk: Option<PublicKeyRecord>,
    pub tail_id: Option<ObjectId>,
    pub global_index: HashSet<String>,
}
