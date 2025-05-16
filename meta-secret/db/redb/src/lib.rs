use anyhow::Result;
use async_trait::async_trait;
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_secret_core::node::db::repo::generic_db::{
    DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use redb::{Database, TableDefinition};
use std::path::Path;
use tracing::{error, instrument};

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

#[async_trait(? Send)]
impl DbCleanUpCommand for ReDbRepo {
    #[instrument(skip_all)]
    async fn db_clean_up(&self) {
        let write_txn = match self.db.begin_write() {
            Ok(txn) => txn,
            Err(err) => {
                error!("Failed to begin transaction for cleanup: {:?}", err);
                return;
            }
        };

        // Try to clear the table by recreating it
        if let Err(err) = write_txn.delete_table(LOG_EVENTS_TABLE) {
            error!("Failed to delete table during cleanup: {:?}", err);
            return;
        }

        if let Err(err) = write_txn.open_table(LOG_EVENTS_TABLE) {
            error!("Failed to recreate table during cleanup: {:?}", err);
            return;
        }

        if let Err(err) = write_txn.commit() {
            error!("Failed to commit transaction during cleanup: {:?}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meta_secret_core::node::common::model::device::common::DeviceName;
    use meta_secret_core::node::common::model::device::device_creds::{DeviceCredsBuilder, SecureDeviceCreds};
    use meta_secret_core::node::db::descriptors::object_descriptor::{ObjectFqdn, ToObjectDescriptor};
    use meta_secret_core::node::db::events::local_event::DeviceCredsObject;
    use meta_secret_core::node::db::events::object_id::{ArtifactId, Next};
    use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
    use meta_secret_core::node::db::events::kv_log_event::{KvKey, KvLogEvent};
    use tempfile::tempdir;
    use meta_secret_core::crypto::key_pair::{KeyPair, TransportDsaKeyPair};

    fn create_test_db() -> (ReDbRepo, tempfile::TempDir) {
        // Create a temporary directory for the database file
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_redb.db");
        let repo = ReDbRepo::new(db_path).unwrap();
        
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_redb_repo_basic_operations() -> Result<()> {
        // Create a temporary ReDbRepo for testing
        let (repo, _temp_dir) = create_test_db();

        // Create a test event
        let fqdn = ObjectFqdn {
            obj_type: "DeviceCreds".to_string(),
            obj_instance: "index".to_string(),
        };
        let id = ArtifactId::from(fqdn);
        let device_creds = DeviceCredsBuilder::generate()
            .build(DeviceName::client())
            .creds;
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        
        let secure_device_creds = SecureDeviceCreds::build(device_creds, master_pk)?;
        let creds_obj = DeviceCredsObject::from(secure_device_creds);
        let test_event = creds_obj.to_generic();

        // Test save operation
        let saved_id = repo.save(test_event).await?;
        assert_eq!(saved_id.id_str(), id.clone().id_str());

        // Test find_one operation
        let found = repo.find_one(id.clone()).await?;
        assert!(found.is_some());

        // Test get_key operation
        let found_key = repo.get_key(id.clone()).await?;
        assert!(found_key.is_some());
        assert_eq!(found_key.unwrap().id_str(), id.clone().id_str());

        // Test delete operation
        repo.delete(id.clone()).await;
        let found_after_delete = repo.find_one(id.clone()).await?;
        assert!(found_after_delete.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_redb_repo_db_clean_up() -> Result<()> {
        // Create a temporary ReDbRepo for testing
        let (repo, _temp_dir) = create_test_db();

        // Create multiple test events to populate the database
        let device_creds = DeviceCredsBuilder::generate()
            .build(DeviceName::client())
            .creds;
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        
        let secure_device_creds = SecureDeviceCreds::build(device_creds.clone(), master_pk)?;
        
        let creds_desc = DeviceCredsDescriptor;
        let initial_id = ArtifactId::from(creds_desc.clone());
        let mut id = initial_id.clone();

        // Insert multiple events
        for i in 1..=5 {
            let kv_event = KvLogEvent {
                key: KvKey::artifact(creds_desc.clone().to_obj_desc(), id.clone()),
                value: secure_device_creds.clone(),
            };
            
            let creds_obj = DeviceCredsObject(kv_event);
            let test_event = creds_obj.to_generic();

            repo.save(test_event).await?;
            
            // Verify the event was saved
            let found = repo.find_one(id.clone()).await?;
            assert!(found.is_some(), "Failed to save event {}", i);
            
            id = id.next();
        }

        // Since redb doesn't provide a direct way to get the path after the database is created,
        // we need to create a new repo with the same path for testing persistence
        let db_path = _temp_dir.path().join("test_redb.db");
        drop(repo);
        let repo = ReDbRepo::open(&db_path)?;
        
        // Verify events are still there after reopening
        let mut id = initial_id.clone();
        for i in 1..=5 {
            let found = repo.find_one(id.clone()).await?;
            assert!(found.is_some(), "Event {} not found after reopen", i);
            id = id.next();
        }

        // Execute the db_clean_up method
        repo.db_clean_up().await;

        // Verify all records have been deleted
        let mut id = initial_id;
        for i in 1..=5 {
            let found = repo.find_one(id.clone()).await?;
            assert!(found.is_none(), "Failed to clean up event {}", i);
            id = id.next();
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_redb_repo_reopen() -> Result<()> {
        // Create and use a repo
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("reopen_test.db");
        
        // Create initial repo and add one item
        {
            let repo = ReDbRepo::new(&db_path)?;
            
            let fqdn = ObjectFqdn {
                obj_type: "DeviceCreds".to_string(),
                obj_instance: "index".to_string(),
            };
            let _id = ArtifactId::from(fqdn);
            let device_creds = DeviceCredsBuilder::generate()
                .build(DeviceName::client())
                .creds;
            let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
            
            let secure_device_creds = SecureDeviceCreds::build(device_creds.clone(), master_pk)?;
            
            let creds_obj = DeviceCredsObject::from(secure_device_creds);
            
            repo.save(creds_obj).await?;
            
            // Let the repo go out of scope and close
        }
        
        // Reopen the same database file
        let reopened_repo = ReDbRepo::open(&db_path)?;
        
        // Check if the data is still there
        let fqdn = ObjectFqdn {
            obj_type: "DeviceCreds".to_string(),
            obj_instance: "index".to_string(),
        };
        // Fix warning: use _id for unused variable
        let _id = ArtifactId::from(fqdn.clone());
        
        // Use fqdn to create id for test
        let id = ArtifactId::from(fqdn);
        
        let found = reopened_repo.find_one(id).await?;
        assert!(found.is_some(), "Data should persist after reopening the database");
        
        Ok(())
    }
}
