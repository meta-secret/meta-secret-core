use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use async_trait::async_trait;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;

pub struct InMemKvLogEventRepo {
    pub db: RefCell<HashMap<ObjectId, GenericKvLogEvent>>,
}

impl Default for InMemKvLogEventRepo {
    fn default() -> Self {
        InMemKvLogEventRepo {
            db: RefCell::new(HashMap::new()),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InMemDbError {}

#[async_trait(? Send)]
impl FindOneQuery for InMemKvLogEventRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, Box<dyn Error>> {
        let maybe_value = self.db.borrow().get(key).cloned();
        Ok(maybe_value)
    }
}

#[async_trait(? Send)]
impl SaveCommand for InMemKvLogEventRepo {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), Box<dyn Error>> {
        self.db.borrow_mut().insert(key.clone(), value.clone());
        Ok(())
    }
}

impl KvLogEventRepo for InMemKvLogEventRepo {}
