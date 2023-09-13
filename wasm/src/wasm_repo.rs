use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use wasm_bindgen::JsValue;

use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::{CommitLogDbConfig, DeleteCommand, FindOneQuery, KvLogEventRepo};
use meta_secret_core::node::db::generic_db::SaveCommand;
use meta_secret_core::node::logger::{LoggerId, MetaLogger};

use crate::{debug, error, idbDelete, idbGet, idbSave, info, warn};

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
    async fn save(&self, key: &ObjectId, event: &GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let event_js: JsValue = serde_wasm_bindgen::to_value(event)
            .map_err(|err| anyhow!("Error parsing data to save: {:?}", err))?;

        idbSave(self.db_name.clone(), self.store_name.clone(), key.id_str(), event_js).await;

        Ok(key.clone())
    }
}

pub struct JsDb<F: Future<Output=anyhow::Result<ObjectId>> + Send> {
    pub db_name: String,
    pub store_name: String,
    _phantom: PhantomData<F>,
}

#[async_trait(? Send)]
impl FindOneQuery for WasmRepo {
    async fn find_one(&self, key: &ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let self_arc = Arc::new(self.clone());
        let obj_js = idbGet(self_arc.db_name.clone(), self_arc.store_name.clone(), key.id_str().as_str().to_string()).await;

        if obj_js.is_undefined() {
            Ok(None)
        } else {
            let obj = serde_wasm_bindgen::from_value(obj_js)
                .map_err(|err| anyhow!("Error parsing found value: {:?}", err))?;
            Ok(Some(obj))
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for WasmRepo {
    async fn delete(&self, key: &ObjectId) {
        idbDelete(self.db_name.as_str(), self.store_name.as_str(), key.id_str().as_str()).await
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
    pub id: LoggerId,
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