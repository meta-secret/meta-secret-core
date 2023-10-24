use async_trait::async_trait;

use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;

#[async_trait(? Send)]
pub trait SaveCommand {
    async fn save(&self, value: GenericKvLogEvent) -> anyhow::Result<ObjectId>;
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
