use async_trait::async_trait;

use crate::models::{MetaPasswordId, SecretDistributionDocData};
use crate::node::db::events::common::LogEventKeyBasedRecord;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::KvKey;
use crate::node::db::events::object_id::ObjectId;

#[async_trait(? Send)]
pub trait SaveCommand {
    async fn save(&self, key: &ObjectId, value: &GenericKvLogEvent) -> Result<(), Box<dyn std::error::Error>>;
    async fn save_event(&self, value: &GenericKvLogEvent) -> Result<ObjectId, Box<dyn std::error::Error>> {
        match &value.key() {
            KvKey::Empty { .. } => {
                panic!("Invalid event. Empty event")
            }
            KvKey::Key{ obj_id, .. } => {
                let _ = self.save(obj_id, value).await;
                Ok(obj_id.clone())
            }
        }
    }
}

#[async_trait(? Send)]
pub trait FindOneQuery {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, Box<dyn std::error::Error>>;
}

#[async_trait(? Send)]
pub trait DeleteCommand {
    async fn delete(&self, key: &ObjectId);
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    async fn find(&self, key: &ObjectId) -> Result<Vec<T>, Box<dyn std::error::Error>>;
}

pub trait KvLogEventRepo: FindOneQuery + SaveCommand + DeleteCommand {}

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
