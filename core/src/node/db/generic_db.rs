use async_trait::async_trait;

use crate::models::{MetaPasswordId, SecretDistributionDocData};
use crate::node::db::models::{GenericKvLogEvent, LogEventKeyBasedRecord};
use crate::node::db::events::object_id::ObjectId;

#[async_trait(? Send)]
pub trait SaveCommand<DbErr: std::error::Error> {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), DbErr>;
    async fn save_event(&self, value: &GenericKvLogEvent) -> Result<(), DbErr> {
        self.save(&value.key().key_id.obj_id(), value).await
    }
}

#[async_trait(? Send)]
pub trait FindOneQuery<DbErr: std::error::Error> {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, DbErr>;
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    type Error: std::error::Error;

    async fn find(&self, key: &ObjectId) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindAllQuery<T> {
    type Error: std::error::Error;

    async fn find_all(&self) -> Result<Vec<T>, Self::Error>;
}

pub trait KvLogEventRepo<DbErr: std::error::Error>: FindOneQuery<DbErr> + SaveCommand<DbErr> {}

pub trait CommitLogDbConfig {
    fn db_name(&self) -> String;
    fn store_name(&self) -> String;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasswordEntity {
    pub meta_pass_id: MetaPasswordId,
    /// Encrypted UserShareDto-s
    pub shares: Vec<SecretDistributionDocData>,
}
