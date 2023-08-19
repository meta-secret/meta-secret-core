use std::error::Error;
use async_trait::async_trait;
use wasm_bindgen::JsValue;

use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::{CommitLogDbConfig, FindOneQuery, KvLogEventRepo};
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::server::data_sync::{LoggerId, MetaLogger};

use crate::{debug, error, idbGet, idbSave, info, warn};

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
            db_name: String::from("meta-secret-server"),
            store_name: "commit_log".to_string(),
        }
    }

    pub fn virtual_device() -> WasmRepo {
        WasmRepo {
            db_name: String::from("meta-secret-v-device"),
            store_name: String::from("commit_log"),
        }
    }

    pub fn virtual_device_2() -> WasmRepo {
        WasmRepo {
            db_name: String::from("meta-secret-v-device-2"),
            store_name: String::from("commit_log"),
        }
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
    async fn save(&self, key: &ObjectId, event: &GenericKvLogEvent) -> Result<(), Box<dyn Error>> {
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
impl FindOneQuery for WasmRepo {
    async fn find_one(&self, key: &ObjectId) -> Result<Option<GenericKvLogEvent>, Box<dyn Error>> {
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

impl KvLogEventRepo for WasmRepo {}

impl CommitLogDbConfig for WasmRepo {
    fn db_name(&self) -> String {
        self.db_name.clone()
    }

    fn store_name(&self) -> String {
        self.store_name.clone()
    }
}

pub struct WasmMetaLogger {
    pub id: LoggerId
}

impl MetaLogger for WasmMetaLogger {
    fn debug(&self, msg: &str) {
        debug(self.get_message(msg).as_str());
    }

    fn info(&self, msg: &str) {
        info(self.get_message(msg).as_str());
    }

    fn warn(&self, msg: &str) {
        warn(self.get_message(msg).as_str());
    }

    fn error(&self, msg: &str) {
        error(self.get_message(msg).as_str());
    }

    fn id(&self) -> LoggerId {
        self.id.clone()
    }
}

impl WasmMetaLogger {
    fn get_message(&self, msg: &str) -> String {
        format!("[{:?}]: {:?}", self.id, msg)
    }
}