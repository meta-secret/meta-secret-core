use async_trait::async_trait;

use crate::node::db::events::common::LogEventKeyBasedRecord;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::events::object_id::ObjectId;

#[async_trait]
pub trait SaveCommand: Send + Sync {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> anyhow::Result<ObjectId>;

    async fn save_event(&self, value: &GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        match &value.key() {
            KvKey::Empty { .. } => {
                panic!("Invalid event. Empty event")
            }
            KvKey::Key { obj_id, .. } => {
                let _ = self.save(obj_id, value).await;
                Ok(obj_id.clone())
            }
        }
    }
}

#[async_trait]
pub trait FindOneQuery: Send + Sync {
    async fn find_one(&self, key: &ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>>;
}

#[async_trait]
pub trait DeleteCommand: Send + Sync {
    async fn delete(&self, key: &ObjectId);
}

#[async_trait]
pub trait FindQuery<T> {
    async fn find(&self, key: &ObjectId) -> anyhow::Result<Vec<T>>;
}

#[async_trait]
pub trait KvLogEventRepo: FindOneQuery + SaveCommand + DeleteCommand + Send + Sync {}

pub trait CommitLogDbConfig {
    fn db_name(&self) -> String;
    fn store_name(&self) -> String;
}
