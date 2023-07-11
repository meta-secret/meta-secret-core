use std::rc::Rc;

use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};

use meta_secret_core::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use meta_secret_core::node::db::models::{GenericKvLogEvent};
use meta_secret_core::node::db::events::object_id::{ObjectId};
use meta_secret_core::node::server::data_sync::{MetaServerContextState};

use crate::models::DbLogEvent;
use crate::models::NewDbLogEvent;
use crate::schema::db_commit_log as schema_log;
use crate::schema::db_commit_log::dsl;

pub struct SqlIteRepo {
    /// conn_url="file:///tmp/test.db"
    pub conn_url: String,
    pub context: Rc<MetaServerContextState>,
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
impl SaveCommand<SqliteDbError> for SqlIteRepo {
    async fn save(&self, _key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), SqliteDbError> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        diesel::insert_into(schema_log::table)
            .values(&NewDbLogEvent::from(value))
            .execute(&mut conn)?;
        Ok(())
    }
}

#[async_trait(? Send)]
impl FindOneQuery<SqliteDbError> for SqlIteRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, SqliteDbError> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let db_event: DbLogEvent = dsl::db_commit_log
            .filter(dsl::key_id.eq(key.id_str()))
            .first::<DbLogEvent>(&mut conn)?;

        Ok(Some(GenericKvLogEvent::from(&db_event)))
    }
}

impl KvLogEventRepo<SqliteDbError> for SqlIteRepo {}