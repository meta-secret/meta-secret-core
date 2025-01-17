use std::collections::HashMap;
use std::sync::Arc;

use async_mutex::Mutex;
use async_trait::async_trait;

use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::repo::generic_db::{DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand};
use anyhow::Result;
use tracing::instrument;

pub struct InMemKvLogEventRepo {
    pub db: Arc<Mutex<HashMap<ObjectId, GenericKvLogEvent>>>,
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
    async fn find_one(&self, key: ObjectId) -> Result<Option<GenericKvLogEvent>> {
        let maybe_value = self.db.lock().await.get(&key).cloned();
        Ok(maybe_value)
    }

    async fn get_key(&self, key: ObjectId) -> Result<Option<ObjectId>> {
        let maybe_value = self.db.lock().await.get(&key).cloned();
        Ok(maybe_value.map(|value| value.obj_id()))
    }
}

#[async_trait(? Send)]
impl SaveCommand for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn save(&self, value: GenericKvLogEvent) -> Result<ObjectId> {
        let mut db = self.db.lock().await;

        let key = value.obj_id();
        db.insert(key.clone(), value.clone());
        Ok(key)
    }
}

#[async_trait(? Send)]
impl DeleteCommand for InMemKvLogEventRepo {
    #[instrument(skip_all)]
    async fn delete(&self, key: ObjectId) {
        let mut db = self.db.lock().await;
        let _ = db.remove(&key);
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}

impl InMemKvLogEventRepo {
    pub async fn get_db(&self) -> HashMap<ObjectId, GenericKvLogEvent> {
        let db = self.db.lock().await;
        let cloned_map: HashMap<ObjectId, GenericKvLogEvent> = db.clone();

        cloned_map
    }
}
