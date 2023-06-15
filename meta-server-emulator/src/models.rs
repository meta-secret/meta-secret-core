use crate::schema::db_commit_log;
use diesel::prelude::*;
use meta_secret_core::node::db::models::KvLogEvent;

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

impl From<&KvLogEvent> for NewDbLogEvent {
    fn from(log_event: &KvLogEvent) -> Self {
        Self {
            key_id: log_event.key.key_id.obj_id.id.clone(),
            event: serde_json::to_string(log_event).unwrap(),
        }
    }
}

impl From<&DbLogEvent> for KvLogEvent {
    fn from(db_event: &DbLogEvent) -> Self {
        serde_json::from_str::<KvLogEvent>(db_event.event.as_str()).unwrap()
    }
}
