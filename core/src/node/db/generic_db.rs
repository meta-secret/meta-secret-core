use async_trait::async_trait;

use crate::models::{MetaPasswordId, SecretDistributionDocData};
use crate::node::db::models::KvLogEvent;

#[async_trait(? Send)]
pub trait SaveCommand {
    type Error: std::error::Error;
    async fn save(&self, value: &KvLogEvent) -> Result<(), Self::Error>;
}

#[async_trait(? Send)]
pub trait FindOneQuery {
    type Error: std::error::Error;
    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    type Error: std::error::Error;

    async fn find(&self, key: &str) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindByAttrQuery<T> {
    type Error: std::error::Error;

    async fn find_by(&self, attr_name: &str) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindAllQuery<T> {
    type Error: std::error::Error;

    async fn find_all(&self) -> Result<Vec<T>, Self::Error>;
}

pub trait KvLogEventRepo: FindOneQuery + SaveCommand {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasswordEntity {
    pub meta_pass_id: MetaPasswordId,
    /// Encrypted UserShareDto-s
    pub shares: Vec<SecretDistributionDocData>,
}
