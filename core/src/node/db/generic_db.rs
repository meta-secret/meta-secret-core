use async_trait::async_trait;

use crate::models::{MetaPasswordId, SecretDistributionDocData};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::models::{GenericKvLogEvent, LogEventKeyBasedRecord};

#[async_trait(? Send)]
pub trait SaveCommand {

    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), Box<dyn std::error::Error>>;
    async fn save_event(&self, value: &GenericKvLogEvent) -> Result<(), Box<dyn std::error::Error>> {
        self.save(&value.key().obj_id, value).await
    }
}

#[async_trait(? Send)]
pub trait FindOneQuery {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, Box<dyn std::error::Error>>;
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    async fn find(&self, key: &ObjectId) -> Result<Vec<T>, Box<dyn std::error::Error>>;
}

pub trait KvLogEventRepo: FindOneQuery + SaveCommand {

}

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
