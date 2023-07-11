use async_trait::async_trait;

use meta_secret_core::node::db::events::object_id::ObjectId;
use wasm_bindgen::JsValue;

use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::generic_db::{CommitLogDbConfig, FindOneQuery, KvLogEventRepo};
use meta_secret_core::node::db::models::{GenericKvLogEvent};
use meta_secret_core::node::server::data_sync::MetaLogger;

use crate::db::WasmDbError;
use crate::{idbGet, idbSave};

use crate::log;

pub struct WasmRepo {
    pub db_name: String,
    pub store_name: String,
}

impl Default for WasmRepo {
    fn default() -> Self {
        Self {
            db_name: "meta-secret".to_string(),
            store_name: "commit_log".to_string(),
        }
    }
}

impl WasmRepo {
    pub fn server() -> WasmRepo {
        WasmRepo {
            db_name: "meta-secret-server".to_string(),
            store_name: "commit_log".to_string(),
        }
    }
}

#[async_trait(? Send)]
impl SaveCommand<WasmDbError> for WasmRepo {
    async fn save(&self, key: &ObjectId, event: &GenericKvLogEvent) -> Result<(), WasmDbError> {
        let event_js: JsValue = serde_wasm_bindgen::to_value(event)?;

        idbSave(
            self.db_name.as_str(),
            self.store_name.as_str(),
            key.id_str().as_str(),
            event_js,
        )
        .await;
        Ok(())
    }
}

#[async_trait(? Send)]
impl FindOneQuery<WasmDbError> for WasmRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, WasmDbError> {

        let obj_js = idbGet(
            self.db_name.as_str(),
            self.store_name.as_str(),
            key.id_str().as_str(),
        )
        .await;

        if obj_js.is_undefined() {
            Ok(None)
        } else {
            let obj = serde_wasm_bindgen::from_value(obj_js)?;
            Ok(Some(obj))
        }
    }
}

impl KvLogEventRepo<WasmDbError> for WasmRepo {}

impl CommitLogDbConfig for WasmRepo {
    fn db_name(&self) -> String {
        self.db_name.clone()
    }

    fn store_name(&self) -> String {
        self.store_name.clone()
    }
}
pub struct WasmMetaLogger {

}

impl MetaLogger for WasmMetaLogger {
    fn log(&self, msg: &str) {
        log(msg);
    }
}