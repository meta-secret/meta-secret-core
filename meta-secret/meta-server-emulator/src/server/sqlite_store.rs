use async_trait::async_trait;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};

use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::repo::generic_db::{
    DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};

use crate::models::DbLogEvent;
use crate::models::NewDbLogEvent;
use crate::schema::db_commit_log as schema_log;
use crate::schema::db_commit_log::dsl;

pub struct SqlIteRepo {
    /// conn_url="file:///tmp/test.db"
    pub conn_url: String,
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
    async fn save<T: ToGenericEvent>(&self, value: T) -> anyhow::Result<ObjectId> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str())?;

        let generic_value = value.to_generic();
        diesel::insert_into(schema_log::table)
            .values(&NewDbLogEvent::from(&generic_value))
            .execute(&mut conn)?;
        Ok(generic_value.obj_id())
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

    async fn get_key(&self, key: ObjectId) -> anyhow::Result<Option<ObjectId>> {
        let maybe_event = self.find_one(key).await?;
        match maybe_event {
            None => Ok(None),
            Some(event) => Ok(Some(event.obj_id())),
        }
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
