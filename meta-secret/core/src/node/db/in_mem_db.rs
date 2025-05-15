use std::collections::HashMap;
use std::sync::Arc;

use async_mutex::Mutex;
use async_trait::async_trait;

use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::repo::generic_db::{
    DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use anyhow::Result;
use tracing::instrument;

pub struct InMemKvLogEventRepo {
    pub db: Arc<Mutex<HashMap<ArtifactId, GenericKvLogEvent>>>,
}

impl Default for InMemKvLogEventRepo {
    fn default() -> Self {
        InMemKvLogEventRepo {
            db: Arc::new(Mutex::new(HashMap::default())),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InMemDbError {}

#[async_trait(? Send)]
impl FindOneQuery for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let maybe_value = self.db.lock().await.get(&key).cloned();
        Ok(maybe_value)
    }

    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        let maybe_value = self.db.lock().await.get(&key).cloned();
        Ok(maybe_value.map(|value| value.obj_id()))
    }
}

#[async_trait(? Send)]
impl SaveCommand for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let mut db = self.db.lock().await;

        let key = value.clone().to_generic().obj_id();
        db.insert(key.clone(), value.to_generic());
        Ok(key)
    }
}

#[async_trait(? Send)]
impl DeleteCommand for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn delete(&self, key: ArtifactId) {
        let mut db = self.db.lock().await;
        let _ = db.remove(&key);
    }
}

#[async_trait(? Send)]
impl DbCleanUpCommand for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn db_clean_up(&self) {
        let mut db = self.db.lock().await;
        db.clear();
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}

impl InMemKvLogEventRepo {
    pub async fn get_db(&self) -> HashMap<ArtifactId, GenericKvLogEvent> {
        let db = self.db.lock().await;
        let cloned_map: HashMap<ArtifactId, GenericKvLogEvent> = db.clone();

        cloned_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::common::model::IdString;
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::device::device_creds::{DeviceCredsBuilder, SecureDeviceCreds};
    use crate::node::db::descriptors::object_descriptor::{ObjectFqdn, ToObjectDescriptor};
    use crate::node::db::events::local_event::DeviceCredsObject;
    use crate::node::db::events::object_id::{ArtifactId, Next};
    use crate::node::db::descriptors::creds::DeviceCredsDescriptor;
    use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};

    #[tokio::test]
    async fn test_in_mem_repo_basic_operations() -> anyhow::Result<()> {
        // Create the InMemKvLogEventRepo instance
        let repo = InMemKvLogEventRepo::default();

        // Create a test event
        let fqdn = ObjectFqdn {
            obj_type: "DeviceCreds".to_string(),
            obj_instance: "index".to_string(),
        };
        let id = ArtifactId::from(fqdn);
        let device_creds = DeviceCredsBuilder::generate()
            .build(DeviceName::client())
            .creds;
        let secure_device_creds = SecureDeviceCreds::try_from(device_creds.clone())?;
        
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
    async fn test_db_clean_up() -> anyhow::Result<()> {
        // Create the repository
        let repo = InMemKvLogEventRepo::default();

        // Create multiple test events to populate the in-memory database
        let device_creds = DeviceCredsBuilder::generate()
            .build(DeviceName::client())
            .creds;
        let secure_device_creds = SecureDeviceCreds::try_from(device_creds)?;
        
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

        // Check that the database has content (using get_db method)
        let db_content = repo.get_db().await;
        assert_eq!(db_content.len(), 5, "Should have 5 events in the database");

        // Execute the db_clean_up method
        repo.db_clean_up().await;

        // Verify all records have been deleted
        let db_content_after_cleanup = repo.get_db().await;
        assert_eq!(db_content_after_cleanup.len(), 0, "Database should be empty after cleanup");

        // Double-check specific events are gone
        let mut id = initial_id.clone();
        for i in 1..=5 {
            let found = repo.find_one(id.clone()).await?;
            assert!(found.is_none(), "Failed to clean up event {}", i);
            id = id.next();
        }

        Ok(())
    }
}
