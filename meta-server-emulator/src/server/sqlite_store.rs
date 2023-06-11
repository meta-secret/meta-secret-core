use meta_secret_core::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use meta_secret_core::node::db::models::{KvKeyId, KvLogEvent};
use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};
use meta_secret_core::models::Base64EncodedText;
use meta_secret_core::node::server::log_events_service::PersistentObjectRepo;
use meta_secret_core::node::server::meta_server::{MetaServer, MetaServerContext, MetaServerContextState};
use crate::models::DbLogEvent;
use crate::schema::db_commit_log::dsl;
use crate::models::{NewDbLogEvent};
use crate::schema::db_commit_log as schema_log;

pub struct SqlIteServer {
    /// conn_url="file:///tmp/test.db"
    pub conn_url: String,
    pub context: MetaServerContextState
}

#[derive(thiserror::Error, Debug)]
pub enum SqliteDbError {
    #[error(transparent)]
    InvalidBase64Content {
        #[from]
        source: diesel::result::Error
    },
}

#[async_trait(? Send)]
impl SaveCommand<KvLogEvent> for SqlIteServer {
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
impl FindOneQuery<KvLogEvent> for SqlIteServer {
    type Error = SqliteDbError;

    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let db_event: DbLogEvent = dsl::db_commit_log
            .filter(dsl::key_id.eq(key))
            .first::<DbLogEvent>(&mut conn)?;

        Ok(Some(KvLogEvent::from(&db_event)))
    }
}

impl KvLogEventRepo for SqlIteServer {}

impl MetaServerContext for SqlIteServer {
    fn server_pk(&self) -> Base64EncodedText {
        self.context.server_pk()
    }

    fn tail_id(&self) -> Option<KvKeyId> {
        self.context.global_index_tail_id.clone()
    }
}

impl MetaServer for SqlIteServer {

}

impl PersistentObjectRepo for SqlIteServer {

}