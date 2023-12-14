use anyhow::anyhow;
use async_trait::async_trait;
use indexed_db_futures::IdbDatabase;
use indexed_db_futures::prelude::*;
use js_sys::Object;
use tracing::{Instrument, instrument};
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::repo::generic_db::{
    CommitLogDbConfig, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};

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

    async fn get_store(&self) -> anyhow::Result<IdbObjectStore> {
        let db = open_db(self.db_name.as_str()).await;
        let tx = db.transaction_on_one(self.store_name.as_str())?;
        let store = tx.object_store(self.store_name.as_str())?;
        Ok(store)
    }
}

impl WasmRepo {
    #[instrument(level = Level::DEBUG)]
    pub async fn delete_db(&self) {
        let db = open_db(self.db_name.as_str()).await;
        db.delete().unwrap();
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
    #[instrument(level = Level::DEBUG)]
    async fn save(&self, event: GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let event_js: JsValue = serde_wasm_bindgen::to_value(&event)?;

        let db = open_db(self.db_name.as_str()).in_current_span().await;
        let tx = db
            .transaction_on_one_with_mode(self.store_name.as_str(), IdbTransactionMode::Readwrite)
            .unwrap();

        let store = tx.object_store(self.store_name.as_str()).unwrap();

        let obj_id_js = serde_wasm_bindgen::to_value(&event.obj_id())?;
        store.put_key_val_owned(obj_id_js, &event_js).unwrap();

        tx.in_current_span().await.into_result().unwrap();
        // All of the requests in the transaction have already finished so we can just drop it to
        // avoid the unused future warning, or assign it to _.
        //let _ = tx;

        Ok(event.obj_id().clone())
    }
}

#[async_trait]
impl FindOneQuery for WasmRepo {
    #[instrument(level = Level::DEBUG)]
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let store = self.get_store().await?;

        let maybe_event_js: Option<JsValue> = {
            let maybe_value: OptionalJsValueFuture = store.get_owned(key.id_str().as_str())?;
            maybe_value.in_current_span().await?
        };

        let Some(event_js) = maybe_event_js else {
            Ok(None)
        };

        if event_js.is_undefined() {
            return Ok(None);
        }

        let js_object = Object::from_entries(&event_js)?;
        let obj_js: JsValue = JsValue::from(js_object);

        let obj = serde_wasm_bindgen::from_value(obj_js)?;
        Ok(Some(obj))
    }

    async fn get_key(&self, key: ObjectId) -> anyhow::Result<Option<ObjectId>> {
        let store = self.get_store().await?;
        let maybe_key_js: Option<JsValue> = {
            let maybe_key = store.get_key_owned(key.id_str().as_str())?;
            maybe_key.in_current_span().await?
        };

        let Some(key_js) = maybe_key_js else {
            Ok(None)
        };

        if key_js.is_undefined() {
            return Ok(None);
        }

        let js_object = Object::from_entries(&key_js)?;
        let key_js: JsValue = JsValue::from(js_object);

        let key = serde_wasm_bindgen::from_value(key_js)?;
        Ok(Some(key))

    }
}

#[async_trait(? Send)]
impl DeleteCommand for WasmRepo {
    #[instrument(level = Level::DEBUG)]
    async fn delete(&self, key: ObjectId) {
        let db = open_db(self.db_name.as_str()).in_current_span().await;
        let tx: IdbTransaction = db
            .transaction_on_one_with_mode(self.store_name.as_str(), IdbTransactionMode::Readwrite)
            .unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();
        store.delete_owned(key.id_str().as_str()).unwrap();
        tx.in_current_span().await.into_result().unwrap();
    }
}

#[instrument(level = Level::DEBUG)]
pub async fn open_db(db_name: &str) -> IdbDatabase {
    let mut db_req: OpenDbRequest = IdbDatabase::open_u32(db_name, 1).unwrap();

    let on_upgrade_task = move |evt: &IdbVersionChangeEvent| -> Result<(), JsValue> {
        // Check if the object store exists; create it if it doesn't
        if !evt.db().object_store_names().any(|n| &n == "commit_log") {
            evt.db().create_object_store("commit_log").unwrap();
        }
        Ok(())
    };

    db_req.set_on_upgrade_needed(Some(on_upgrade_task));

    db_req.into_future().in_current_span().await.unwrap()
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
