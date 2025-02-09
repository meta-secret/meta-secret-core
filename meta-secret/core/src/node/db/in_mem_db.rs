use std::collections::HashMap;
use std::sync::Arc;

use async_mutex::Mutex;
use async_trait::async_trait;

use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::repo::generic_db::{DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand};
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

impl KvLogEventRepo for InMemKvLogEventRepo {}

impl InMemKvLogEventRepo {
    pub async fn get_db(&self) -> HashMap<ArtifactId, GenericKvLogEvent> {
        let db = self.db.lock().await;
        let cloned_map: HashMap<ArtifactId, GenericKvLogEvent> = db.clone();

        cloned_map
    }
}
