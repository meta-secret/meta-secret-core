use crate::models::DbLogEvent;
use crate::models::NewDbLogEvent;
use crate::schema::db_commit_log as schema_log;
use crate::schema::db_commit_log::dsl;
use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};
use meta_secret_core::models::Base64EncodedText;
use meta_secret_core::node::db::events::global_index::GlobalIndexAction;
use meta_secret_core::node::db::events::sign_up::SignUpAction;
use meta_secret_core::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use meta_secret_core::node::db::models::{KvKeyId, KvLogEvent};
use meta_secret_core::node::server::meta_server::{
    MetaServer, MetaServerContext, MetaServerContextState,
};
use meta_secret_core::node::server::persistent_object_repo::ObjectFormation;

pub struct SqlIteServer {
    /// conn_url="file:///tmp/test.db"
    pub conn_url: String,
    pub context: MetaServerContextState,
}

#[derive(thiserror::Error, Debug)]
pub enum SqliteDbError {
    #[error(transparent)]
    InvalidBase64Content {
        #[from]
        source: diesel::result::Error,
    },
}

#[async_trait(? Send)]
impl SaveCommand<SqliteDbError> for SqlIteServer {
    async fn save(&self, value: &KvLogEvent) -> Result<(), SqliteDbError> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        diesel::insert_into(schema_log::table)
            .values(&NewDbLogEvent::from(value))
            .execute(&mut conn)?;
        Ok(())
    }
}

#[async_trait(? Send)]
impl FindOneQuery<SqliteDbError> for SqlIteServer {
    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, SqliteDbError> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let db_event: DbLogEvent = dsl::db_commit_log
            .filter(dsl::key_id.eq(key))
            .first::<DbLogEvent>(&mut conn)?;

        Ok(Some(KvLogEvent::from(&db_event)))
    }
}

impl MetaServerContext for SqlIteServer {
    fn server_pk(&self) -> Base64EncodedText {
        self.context.server_pk()
    }

    fn tail_id(&self) -> Option<KvKeyId> {
        self.context.global_index_tail_id.clone()
    }
}

impl KvLogEventRepo<SqliteDbError> for SqlIteServer {}

impl GlobalIndexAction for SqlIteServer {}

impl MetaServer<SqliteDbError> for SqlIteServer {}

impl ObjectFormation for SqlIteServer {}

impl SignUpAction for SqlIteServer {}
