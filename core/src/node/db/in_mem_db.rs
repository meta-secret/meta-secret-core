use async_mutex::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{instrument};
use tracing::Level;

use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::repo::generic_db::{DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand};

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

    #[instrument(level = Level::DEBUG)]
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let maybe_value = self.db.lock().await.get(&key).cloned();
        Ok(maybe_value)
    }
}

#[async_trait(? Send)]
impl SaveCommand for InMemKvLogEventRepo {
    
    #[instrument]
    async fn save(&self, value: GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let mut db = self.db.lock().await;

        let key = value.obj_id();
        db.insert(key.clone(), value.clone());
        Ok(key)
    }
}

#[async_trait(? Send)]
impl DeleteCommand for InMemKvLogEventRepo {
    
    #[instrument]
    async fn delete(&self, key: ObjectId) {
        let mut db = self.db.lock().await;
        let _ = db.remove(&key);
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}
