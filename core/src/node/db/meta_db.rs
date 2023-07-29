use crate::models::vault_doc::VaultDoc;
use crate::models::MetaPasswordDoc;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::models::PublicKeyRecord;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub vault_store: VaultStore,
    pub global_index_store: GlobalIndexStore,
    pub meta_pass_store: MetaPassStore,
}

impl Default for MetaDb {
    fn default() -> Self {
        Self {
            vault_store: VaultStore::Empty,
            global_index_store: GlobalIndexStore {
                server_pk: None,
                tail_id: ObjectId::global_index_unit(),
                global_index: HashSet::new(),
            },
            meta_pass_store: MetaPassStore::Empty,
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
pub struct GlobalIndexStore {
    pub server_pk: Option<PublicKeyRecord>,
    pub tail_id: ObjectId,
    pub global_index: HashSet<String>,
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
