use anyhow::Result;
use async_trait::async_trait;
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_secret_core::node::db::repo::generic_db::{
    DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use redb::{Database, TableDefinition};
use serde_json;
use std::path::Path;

const TABLE_NAME: &str = "meta-secret-db";
type KeyType = String;
type ValueType = Vec<u8>;

// Table definition for storing KV log events
const LOG_EVENTS_TABLE: TableDefinition<KeyType, ValueType> = TableDefinition::new(TABLE_NAME);

#[derive(thiserror::Error, Debug)]
pub enum ReDbError {
    #[error(transparent)]
    DatabaseError(#[from] redb::Error),

    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
}

pub struct ReDbRepo {
    pub db: Database,
}

impl ReDbRepo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::create(path)?;
        let repo = ReDbRepo { db };
        repo.init_table()?;
        Ok(repo)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::open(path)?;
        let repo = ReDbRepo { db };
        repo.init_table()?;
        Ok(repo)
    }

    fn init_table(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        // Just open the table, which will create it if it doesn't exist
        write_txn.open_table(LOG_EVENTS_TABLE)?;
        write_txn.commit()?;
        Ok(())
    }
}

#[async_trait(? Send)]
impl SaveCommand for ReDbRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let generic_value = value.to_generic();
        let key = generic_value.obj_id();

        let serialized = serde_json::to_vec(&generic_value)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LOG_EVENTS_TABLE)?;
            table.insert(key.clone().id_str(), serialized)?;
        }
        write_txn.commit()?;

        Ok(key)
    }
}

#[async_trait(? Send)]
impl FindOneQuery for ReDbRepo {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LOG_EVENTS_TABLE)?;

        match table.get(key.clone().id_str())? {
            Some(value) => {
                let data = value.value();
                let event: GenericKvLogEvent = serde_json::from_slice(data.as_slice())?;
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        let maybe_event = self.find_one(key).await?;
        match maybe_event {
            None => Ok(None),
            Some(event) => Ok(Some(event.obj_id())),
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for ReDbRepo {
    async fn delete(&self, key: ArtifactId) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(LOG_EVENTS_TABLE).unwrap();
            let _ = table.remove(key.clone().id_str());
        }
        let _ = write_txn.commit();
    }
}

impl KvLogEventRepo for ReDbRepo {}
