use async_trait::async_trait;
use tracing::Instrument;

use crate::node::db::events::common::LogEventKeyBasedRecord;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::events::object_id::ObjectId;

#[async_trait(? Send)]
pub trait SaveCommand {
    async fn save(&self, key: ObjectId, value: GenericKvLogEvent) -> anyhow::Result<ObjectId>;

    async fn save_event(&self, value: GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        match &value.key() {
            KvKey { obj_id, .. } => {
                let _ = self.save(obj_id.clone(), value.clone()).in_current_span().await;
                Ok(obj_id.clone())
            }
        }
    }
}

#[async_trait(? Send)]
pub trait FindOneQuery: Send {
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>>;
}

#[async_trait(? Send)]
pub trait DeleteCommand {
    async fn delete(&self, key: ObjectId);
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    async fn find(&self, key: ObjectId) -> anyhow::Result<Vec<T>>;
}

#[async_trait(? Send)]
pub trait KvLogEventRepo: FindOneQuery + SaveCommand + DeleteCommand + 'static {}

pub trait CommitLogDbConfig {
    fn db_name(&self) -> String;
    fn store_name(&self) -> String;
}
