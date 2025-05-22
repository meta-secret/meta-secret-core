use anyhow::{Context, Result};
use async_trait::async_trait;
use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent};
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_secret_core::node::db::repo::generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions, Row};
use meta_secret_core::node::common::model::IdString;
use serde_json::{from_str, to_string};

pub struct IosRepo {
    pub db_name: String,
    pub store_name: String,
    pool: SqlitePool,
}

impl IosRepo {
    pub async fn default() -> Result<Self> {
        let db_name = "meta-secret".to_string();
        let store_name = "commit_log".to_string();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}", db_name))
            .await
            .context("Can't connect to DB SQLite")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS commit_log (
                id TEXT PRIMARY KEY,
                event_data TEXT NOT NULL
            )"
        )
            .execute(&pool)
            .await
            .context("Не удалось создать таблицу")?;

        Ok(Self {
            db_name,
            store_name,
            pool,
        })
    }
}

#[async_trait(?Send)]
impl FindOneQuery for IosRepo {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let id_str = key.id_str();
        
        let record = sqlx::query("SELECT event_data FROM commit_log WHERE id = ?")
            .bind(id_str)
            .fetch_optional(&self.pool)
            .await
            .context("DB request error")?;

        match record {
            Some(row) => {
                let event_data: String = row.try_get("event_data")
                    .context("Failed to get event_data column")?;
                let event: GenericKvLogEvent = from_str(&event_data)
                    .context("Cannot deserialize GenericKvLogEvent")?;
                Ok(Some(event))
            },
            None => Ok(None)
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

#[async_trait(?Send)]
impl SaveCommand for IosRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let generic_event = value.to_generic();
        let obj_id = generic_event.obj_id();

        if let Some(_) = self.get_key(obj_id.clone()).await? {
            anyhow::bail!("Event with this id already exists");
        }

        let json = to_string(&generic_event)
            .context("Can't serialize GenericEvent")?;

        sqlx::query("INSERT INTO commit_log (id, event_data) VALUES (?, ?)")
            .bind(obj_id.clone().id_str())
            .bind(json)
            .execute(&self.pool)
            .await
            .context("Can't save commit log")?;

        Ok(obj_id)
    }
}

#[async_trait(?Send)]
impl DeleteCommand for IosRepo {
    async fn delete(&self, key: ArtifactId) {
        let id_str = key.id_str();

        let _ = sqlx::query("DELETE FROM commit_log WHERE id = ?")
            .bind(id_str)
            .execute(&self.pool)
            .await;
    }
}

#[async_trait(?Send)]
impl DbCleanUpCommand for IosRepo {
    async fn db_clean_up(&self) {
        let _ = sqlx::query("DELETE FROM commit_log")
            .execute(&self.pool)
            .await;
    }
}

impl KvLogEventRepo for IosRepo {}
