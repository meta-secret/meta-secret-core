use diesel::prelude::*;
use crate::schema::db_commit_log;
use meta_secret_core::node::db::commit_log::KvLogEvent;

#[derive(Debug, Queryable)]
pub struct DbLogEvent {
    pub id: i32,
    pub store: String,
    pub vault_id: Option<String>,
    pub event: String,
}

#[derive(Insertable)]
#[diesel(table_name = db_commit_log)]
pub struct NewDbLogEvent {
    pub store: String,
    pub vault_id: Option<String>,
    pub event: String,
}

impl From<&KvLogEvent> for NewDbLogEvent {
    fn from(log_event: &KvLogEvent) -> Self {
        Self {
            store: log_event.key.store.clone(),
            vault_id: log_event.key.vault_id.clone(),
            event: serde_json::to_string(log_event).unwrap(),
        }
    }
}

impl From<&DbLogEvent> for KvLogEvent {
    fn from(db_event: &DbLogEvent) -> Self {
        serde_json::from_str::<KvLogEvent>(db_event.event.as_str()).unwrap()
    }
}
