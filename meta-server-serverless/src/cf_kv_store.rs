use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand};
use meta_secret_core::node::db::models::KvLogEvent;
use async_trait::async_trait;
use worker::kv::{KvError, KvStore};

pub struct CfKvStore {
    pub kv_store: KvStore
}

#[derive(thiserror::Error, Debug)]
pub enum CfKvDbError {
    #[error(transparent)]
    CfKvError {
        #[from]
        source: KvError
    },
}

#[async_trait(? Send)]
impl FindOneQuery for CfKvStore {
    type Error = CfKvDbError;

    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
        let maybe_log_event = self.kv_store.get(key).json::<KvLogEvent>().await?;
        Ok(maybe_log_event)
    }
}

#[async_trait(? Send)]
impl SaveCommand for CfKvStore {
    type Error = CfKvDbError;

    async fn save(&self, value: &KvLogEvent) -> Result<(), Self::Error> {
        let key_id = value.key.key_id.obj_id.clone();
        self.kv_store
            .put(key_id.id.as_str(), value)
            .unwrap()
            .execute()
            .await?;
        Ok(())
    }
}
