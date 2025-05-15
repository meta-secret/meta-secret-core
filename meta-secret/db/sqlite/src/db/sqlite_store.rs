use async_trait::async_trait;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection,
};
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_secret_core::node::db::repo::generic_db::{
    DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use tracing::{error, instrument};

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

impl KvLogEventRepo for SqlIteRepo {}

#[async_trait(? Send)]
impl SaveCommand for SqlIteRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> anyhow::Result<ArtifactId> {
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
    async fn find_one(&self, key: ArtifactId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str())?;

        let maybe_db_event = dsl::db_commit_log
            .filter(dsl::key_id.eq(key.id_str()))
            .first::<DbLogEvent>(&mut conn)
            .optional()?;

        match maybe_db_event {
            None => Ok(None),
            Some(db_event) => Ok(Some(GenericKvLogEvent::from(&db_event))),
        }
    }

    async fn get_key(&self, key: ArtifactId) -> anyhow::Result<Option<ArtifactId>> {
        let maybe_event = self.find_one(key).await?;
        match maybe_event {
            None => Ok(None),
            Some(event) => Ok(Some(event.obj_id())),
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for SqlIteRepo {
    async fn delete(&self, key: ArtifactId) {
        let mut conn = SqliteConnection::establish(self.conn_url.as_str()).unwrap();

        let event = dsl::db_commit_log.filter(dsl::key_id.eq(key.id_str()));

        diesel::delete(event)
            .execute(&mut conn)
            .expect("Event not found");
    }
}

#[async_trait(? Send)]
impl DbCleanUpCommand for SqlIteRepo {
    #[instrument(skip_all)]
    async fn db_clean_up(&self) {
        let mut conn = match SqliteConnection::establish(self.conn_url.as_str()) {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to establish SQLite connection: {:?}", err);
                return;
            }
        };

        // This deletes all records but preserves the table structure
        match diesel::delete(schema_log::table).execute(&mut conn) {
            Ok(count) => {
                tracing::info!("Deleted {} records from commit log table", count);
            }
            Err(err) => {
                error!("Failed to clean up database: {:?}", err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite_migration::EmbeddedMigrationsTool;
    use meta_secret_core::node::common::model::device::common::DeviceName;
    use meta_secret_core::node::common::model::device::device_creds::{DeviceCredsBuilder, SecureDeviceCreds};
    use meta_secret_core::node::db::descriptors::object_descriptor::{ObjectFqdn, ToObjectDescriptor};
    use meta_secret_core::node::db::events::local_event::DeviceCredsObject;
    use meta_secret_core::node::db::events::object_id::{ArtifactId, Next};
    use std::clone::Clone;
    use tempfile::tempdir;
    use meta_secret_core::crypto::key_pair::{KeyPair, TransportDsaKeyPair};
    use meta_secret_core::node::db::descriptors::creds::DeviceCredsDescriptor;
    use meta_secret_core::node::db::events::kv_log_event::{KvKey, KvLogEvent};

    #[tokio::test]
    async fn test_sqlite_repo_with_migrations() -> anyhow::Result<()> {
        // Create a temporary directory to store the database file
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test_db.db");
        let conn_url = format!("file:{}", db_path.to_string_lossy());

        // Apply migrations directly before creating the repo
        let migration_tool = EmbeddedMigrationsTool {
            db_url: conn_url.clone(),
        };
        migration_tool.migrate();

        // Create the SqlIteRepo instance
        let repo = SqlIteRepo { conn_url };

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

        let saved_id = repo.save(test_event).await?;
        assert_eq!(saved_id.id_str(), id.clone().id_str());

        // Test that we can find the event
        let found = repo.find_one(id.clone()).await?;
        assert!(found.is_some());

        // Test that we can delete the event
        repo.delete(id.clone()).await;
        let found_after_delete = repo.find_one(id.clone()).await?;
        assert!(found_after_delete.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_db_clean_up() -> anyhow::Result<()> {
        // Create a temporary directory for the database
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("clean_up_test.db");
        let conn_url = format!("file:{}", db_path.to_string_lossy());

        // Initialize migrations
        let migration_tool = EmbeddedMigrationsTool {
            db_url: conn_url.clone(),
        };
        migration_tool.migrate();

        // Create the repository
        let repo = SqlIteRepo { conn_url };

        // Create multiple test events to populate the database
        let device_creds = DeviceCredsBuilder::generate()
            .build(DeviceName::client())
            .creds;
        let master_pk = TransportDsaKeyPair::generate().sk().pk()?;
        
        let secure_device_creds = SecureDeviceCreds::build(device_creds, master_pk)?;

        let creds_desc = DeviceCredsDescriptor;
        let initial_id = ArtifactId::from(creds_desc.clone());
        let mut id = initial_id.clone();

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

        // Execute the db_clean_up method
        repo.db_clean_up().await;

        // Verify all records have been deleted
        let mut id = initial_id.clone();
        for i in 1..=5 {
            let found = repo.find_one(id.clone()).await?;
            assert!(found.is_none(), "Failed to clean up event {}", i);
            id = id.next();
        }

        Ok(())
    }
}
