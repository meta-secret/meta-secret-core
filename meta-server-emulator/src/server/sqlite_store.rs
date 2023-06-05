use meta_secret_core::node::db::db::{FindOneQuery, SaveCommand};
use meta_secret_core::node::db::meta_db::CommitLogStore;
use meta_secret_core::node::db::models::KvLogEvent;
use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};
use crate::models::DbLogEvent;
use crate::schema::db_commit_log::dsl;
use crate::models::{NewDbLogEvent};
use crate::schema::db_commit_log as schema_log;

pub struct SqlIteStore {
    pub conn_url: String,
}

#[derive(thiserror::Error, Debug)]
pub enum SqliteDbError {
    #[error(transparent)]
    InvalidBase64Content {
        #[from]
        source: diesel::result::Error
    },
}

impl SqlIteStore {
    /// conn_url="file:///tmp/test.db"
    pub fn new(conn_url: String) -> Self {
        Self {
            conn_url
        }
    }
}

#[async_trait(? Send)]
impl SaveCommand<KvLogEvent> for SqlIteStore {
    type Error = SqliteDbError;

    async fn save(&self, _key: &str, value: &KvLogEvent) -> Result<(), Self::Error> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        diesel::insert_into(schema_log::table)
            .values(&NewDbLogEvent::from(value))
            .execute(&mut conn)?;
        Ok(())
    }
}

#[async_trait(? Send)]
impl FindOneQuery<KvLogEvent> for SqlIteStore {
    type Error = SqliteDbError;

    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let db_event: DbLogEvent = dsl::db_commit_log
            .filter(dsl::key_id.eq(key))
            .first::<DbLogEvent>(&mut conn)?;

        Ok(Some(KvLogEvent::from(&db_event)))
    }
}

#[async_trait(? Send)]
impl CommitLogStore for SqlIteStore {

}
