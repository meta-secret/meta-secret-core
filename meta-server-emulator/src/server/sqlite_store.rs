use std::sync::Arc;

use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};

use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::repo::generic_db::{
    DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use meta_secret_core::node::server::data_sync::MetaServerContextState;

use crate::models::DbLogEvent;
use crate::models::NewDbLogEvent;
use crate::schema::db_commit_log as schema_log;
use crate::schema::db_commit_log::dsl;

pub struct SqlIteRepo {
    /// conn_url="file:///tmp/test.db"
    pub conn_url: String,
    pub context: Arc<MetaServerContextState>,
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
impl SaveCommand for SqlIteRepo {
    async fn save(&self, value: GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        diesel::insert_into(schema_log::table)
            .values(&NewDbLogEvent::from(&value))
            .execute(&mut conn)?;
        Ok(value.obj_id())
    }
}

#[async_trait(? Send)]
impl FindOneQuery for SqlIteRepo {
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str())?;

        let db_event: DbLogEvent = dsl::db_commit_log
            .filter(dsl::key_id.eq(key.id_str()))
            .first::<DbLogEvent>(&mut conn)?;

        Ok(Some(GenericKvLogEvent::from(&db_event)))
    }
}

#[async_trait(? Send)]
impl DeleteCommand for SqlIteRepo {
    async fn delete(&self, key: ObjectId) {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let event = dsl::db_commit_log.filter(dsl::key_id.eq(key.id_str()));

        diesel::delete(event)
            .execute(&mut conn)
            .expect("Event not found");
    }
}

impl KvLogEventRepo for SqlIteRepo {}
