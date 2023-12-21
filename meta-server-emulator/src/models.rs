use crate::schema::db_commit_log;
use diesel::prelude::*;
use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};

#[derive(Debug, Queryable)]
pub struct DbLogEvent {
    pub id: i32,
    pub key_id: String,
    pub event: String,
}

#[derive(Insertable)]
#[diesel(table_name = db_commit_log)]
pub struct NewDbLogEvent {
    pub key_id: String,
    pub event: String,
}

impl From<&GenericKvLogEvent> for NewDbLogEvent {
    fn from(log_event: &GenericKvLogEvent) -> Self {
        Self {
            key_id: log_event.obj_id().id_str(),
            event: serde_json::to_string(log_event).unwrap(),
        }
    }
}

impl From<&DbLogEvent> for GenericKvLogEvent {
    fn from(db_event: &DbLogEvent) -> Self {
        serde_json::from_str::<GenericKvLogEvent>(db_event.event.as_str()).unwrap()
    }
}
