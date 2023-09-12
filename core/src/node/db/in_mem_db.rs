use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::{DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand};

pub struct InMemKvLogEventRepo {
    pub db: Arc<Mutex<HashMap<ObjectId, GenericKvLogEvent>>>,
}

impl Default for InMemKvLogEventRepo {
    fn default() -> Self {
        InMemKvLogEventRepo {
            db: Arc::new(Mutex::new(HashMap::default()))
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InMemDbError {}

#[async_trait]
impl FindOneQuery for InMemKvLogEventRepo {
    async fn find_one(&self, key: &ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let maybe_value = self.db.lock().unwrap().get(key).cloned();
        Ok(maybe_value)
    }
}

#[async_trait]
impl SaveCommand for InMemKvLogEventRepo {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let mut db = self.db.lock().unwrap();
        db.insert(key.clone(), value.clone());
        Ok(key.clone())
    }
}

#[async_trait]
impl DeleteCommand for InMemKvLogEventRepo {
    async fn delete(&self, key: &ObjectId) {
        let mut db = self.db.lock().unwrap();
        let _ = db.remove(key);
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}
