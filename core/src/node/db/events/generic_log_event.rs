use anyhow::anyhow;

use crate::node::db::events::common::SharedSecretObject;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::error::ErrorMessage;
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::local::{CredentialsObject, DbTailObject};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{ArtifactId, ObjectId};
use crate::node::db::events::vault_event::VaultObject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__generic_event_type")]
pub enum GenericKvLogEvent {
    GlobalIndex(GlobalIndexObject),
    Vault(VaultObject),
    SharedSecret(SharedSecretObject),
    Credentials(CredentialsObject),
    DbTail(DbTailObject),
    Error { event: KvLogEvent<ArtifactId, ErrorMessage> },
}

impl GenericKvLogEvent {
    pub fn to_db_tail(self) -> anyhow::Result<DbTail> {
        if let GenericKvLogEvent::DbTail(DbTailObject { event }) = self {
            Ok(event.value)
        } else {
            Err(anyhow!("DbTail. Invalid event type: {:?}", self))
        }
    }
}

pub trait ToGenericEvent {
    fn to_generic(self) -> GenericKvLogEvent;
}

pub trait UnitEvent<T> {
    fn unit(value: T) -> Self;
}

pub trait UnitEventWithEmptyValue {
    fn unit() -> Self;
}

pub trait ObjIdExtractor {
    fn obj_id(&self) -> ObjectId;
}

pub trait KeyExtractor {
    fn key(&self) -> GenericKvKey;
}

impl ObjIdExtractor for GenericKvLogEvent {
    fn obj_id(&self) -> ObjectId {
        match self {
            GenericKvLogEvent::GlobalIndex(obj) => obj.obj_id(),
            GenericKvLogEvent::Vault(obj) => obj.obj_id(),
            GenericKvLogEvent::SharedSecret(obj) => obj.obj_id(),
            GenericKvLogEvent::Credentials(obj) => obj.obj_id(),
            GenericKvLogEvent::DbTail(obj) => obj.obj_id(),
            GenericKvLogEvent::Error { event } => event.key.obj_id.clone(),
        }
    }
}

impl KeyExtractor for GenericKvLogEvent {
    fn key(&self) -> GenericKvKey {
        match self {
            GenericKvLogEvent::GlobalIndex(obj) => obj.key(),
            GenericKvLogEvent::Vault(obj) => obj.key(),
            GenericKvLogEvent::MetaPass(obj) => obj.key(),
            GenericKvLogEvent::SharedSecret(obj) => obj.key(),
            GenericKvLogEvent::Credentials(obj) => obj.key(),
            GenericKvLogEvent::DbTail(obj) => obj.key(),
            GenericKvLogEvent::Error { event } => event.key.obj_desc.clone(),
        }
    }
}

impl GenericKvLogEvent {

    pub fn db_tail(db_tail: DbTail) -> GenericKvLogEvent {
        let event = KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::DbTail),
            value: db_tail,
        };
        GenericKvLogEvent::DbTail(DbTailObject { event })
    }
}