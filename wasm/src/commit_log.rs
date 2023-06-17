use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use meta_secret_core::node::db::generic_db::{CommitLogDbConfig, FindAllQuery, FindOneQuery};
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::models::KvLogEvent;

use crate::{idbFindAll, idbGet, idbSave};
use crate::db::WasmDbError;
use crate::objects::ToJsValue;

pub struct CommitLogWasmRepo {
    pub db_name: String,
    pub store_name: String,
}

impl Default for CommitLogWasmRepo {
    fn default() -> Self {
        Self {
            db_name: "meta-secret".to_string(),
            store_name: "commit-log".to_string(),
        }
    }
}

#[async_trait(? Send)]
impl FindAllQuery<KvLogEvent> for CommitLogWasmRepo {
    type Error = WasmDbError;

    async fn find_all(&self) -> Result<Vec<KvLogEvent>, Self::Error> {
        let events_js = idbFindAll(self.db_name.as_str(), self.store_name.as_str()).await;
        let events: Vec<KvLogEvent> = serde_wasm_bindgen::from_value(events_js)?;
        Ok(events)
    }
}

#[async_trait(? Send)]
impl SaveCommand<WasmDbError> for CommitLogWasmRepo {
    async fn save(&self, event: &KvLogEvent) -> Result<(), Self::Error> {
        let event_js = event.to_js()?;
        idbSave(
            self.db_name.as_str(),
            self.store_name.as_str(),
            event.key.key_id.obj_id.id.as_str(),
            event_js,
        )
            .await;
        Ok(())
    }
}

impl FindOneQuery<WasmDbError> for CommitLogWasmRepo {

    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, WasmDbError> {
        let vault_js = idbGet(self.db_name.as_str(), self.store_name.as_str(), key).await;
        if vault_js.is_undefined() {
            Ok(None)
        } else {
            let vault = serde_wasm_bindgen::from_value(vault_js)?;
            Ok(Some(vault))
        }
    }
}

impl CommitLogDbConfig for CommitLogWasmRepo {
    fn db_name(&self) -> String {
        self.db_name.clone()
    }

    fn store_name(&self) -> String {
        self.store_name.clone()
    }
}