use async_trait::async_trait;

use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, GenericKvLogEventConvertible, ToGenericEvent,
};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::Result;

// https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html

#[async_trait(? Send)]
pub trait SaveCommand {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId>;
}

#[async_trait(? Send)]
pub trait FindOneQuery {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>>;

    async fn find_one_obj<T>(&self, key: ArtifactId) -> Result<Option<T>>
    where
        T: GenericKvLogEventConvertible,
    {
        let maybe_value: Option<GenericKvLogEvent> = self.find_one(key).await?;
        let result = match maybe_value {
            Some(value) => Some(T::try_from_event(value)?),
            None => None,
        };

        Ok(result)
    }

    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>>;
}

#[async_trait(? Send)]
pub trait DeleteCommand {
    async fn delete(&self, key: ArtifactId);
}

#[async_trait(? Send)]
pub trait DbCleanUpCommand {
    async fn db_clean_up(&self);
}

#[async_trait(? Send)]
pub trait KvLogEventRepo:
    FindOneQuery + SaveCommand + DeleteCommand + DbCleanUpCommand + 'static
{
}

pub trait CommitLogDbConfig {
    fn db_name(&self) -> String;
    fn store_name(&self) -> String;
}
