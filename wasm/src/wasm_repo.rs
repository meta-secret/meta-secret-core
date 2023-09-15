use crate::{debug, error, info, warn};
use anyhow::anyhow;
use async_trait::async_trait;
use indexed_db_futures::prelude::*;
use indexed_db_futures::IdbDatabase;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::{
    CommitLogDbConfig, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use meta_secret_core::node::logger::{LoggerId, MetaLogger};
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

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

impl WasmRepo {
    pub async fn delete_db(&self) {
        let db = open_db(self.db_name.as_str()).await;
        db.delete().unwrap();
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
    async fn save(&self, key: &ObjectId, event: &GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let event_js: JsValue = serde_wasm_bindgen::to_value(event)
            .map_err(|err| anyhow!("Error parsing data to save: {:?}", err))?;

        let db = open_db(self.db_name.as_str()).await;
        let tx = db
            .transaction_on_one_with_mode(self.store_name.as_str(), IdbTransactionMode::Readwrite)
            .unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();
        store
            .put_key_val_owned(key.id_str().as_str(), &event_js)
            .unwrap();

        tx.await.into_result().unwrap();
        // All of the requests in the transaction have already finished so we can just drop it to
        // avoid the unused future warning, or assign it to _.
        //let _ = tx;

        Ok(key.clone())
    }
}

#[async_trait(? Send)]
impl FindOneQuery for WasmRepo {
    async fn find_one(&self, key: &ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let db = open_db(self.db_name.as_str()).await;
        let tx = db.transaction_on_one(self.store_name.as_str()).unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();
        let maybe_obj_js: Option<JsValue> = store
            .get_owned(key.id_str().as_str())
            .unwrap()
            .await
            .unwrap();

        if let Some(obj_js) = maybe_obj_js {
            use js_sys::Object;
            let obj_js = Object::from_entries(&obj_js).unwrap();
            let obj_js: JsValue = JsValue::from(obj_js);

            let obj = serde_wasm_bindgen::from_value(obj_js)
                .map_err(|err| anyhow!("Js object error parsing: {}", err))?;
            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for WasmRepo {
    async fn delete(&self, key: &ObjectId) {
        let db = open_db(self.db_name.as_str()).await;
        let tx = db
            .transaction_on_one_with_mode(self.store_name.as_str(), IdbTransactionMode::Readwrite)
            .unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();
        store.delete_owned(key.id_str().as_str()).unwrap();
        tx.await.into_result().unwrap();
    }
}

pub async fn open_db(db_name: &str) -> IdbDatabase {
    let mut db_req: OpenDbRequest = IdbDatabase::open_u32(db_name, 1).unwrap();

    let on_upgrade_task = move |evt: &IdbVersionChangeEvent| -> Result<(), JsValue> {
        // Check if the object store exists; create it if it doesn't
        if let None = evt.db().object_store_names().find(|n| n == "commit_log") {
            evt.db().create_object_store("commit_log").unwrap();
        }
        Ok(())
    };

    db_req.set_on_upgrade_needed(Some(on_upgrade_task));

    db_req.into_future().await.unwrap()
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
