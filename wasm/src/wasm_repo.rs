use anyhow::{anyhow, Context};
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
}

impl WasmRepo {
    #[instrument(skip_all)]
    pub async fn delete_db(&self) {
        let db = open_db(self.db_name.as_str()).await;
        db.delete().unwrap();
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
    #[instrument(skip_all)]
    async fn save(&self, event: GenericKvLogEvent) -> anyhow::Result<ObjectId> {
        let event_js: JsValue = serde_wasm_bindgen::to_value(&event).unwrap();

        let db = open_db(self.db_name.as_str()).in_current_span().await;
        let tx = db
            .transaction_on_one_with_mode(self.store_name.as_str(), IdbTransactionMode::Readwrite)
            .unwrap();

        let store = tx.object_store(self.store_name.as_str()).unwrap();

        let obj_id_js = serde_wasm_bindgen::to_value(&event.obj_id()).unwrap();
        store.put_key_val_owned(obj_id_js, &event_js).unwrap();

        tx.in_current_span().await.into_result().unwrap();
        // All of the requests in the transaction have already finished so we can just drop it to
        // avoid the unused future warning, or assign it to _.
        //let _ = tx;

        Ok(event.obj_id().clone())
    }
}

#[async_trait(? Send)]
impl FindOneQuery for WasmRepo {

    #[instrument(skip_all)]
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let db = open_db(self.db_name.as_str()).await;
        let tx = db.transaction_on_one(self.store_name.as_str()).unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();

        let maybe_event_js: Option<JsValue> = {
            let maybe_value: OptionalJsValueFuture = store.get_owned(key.id_str().as_str()).unwrap();
            maybe_value.await.unwrap()
        };

        match maybe_event_js {
            None => {
                Ok(None)
            }
            Some(event_js) => {
                if event_js.is_undefined() {
                    return Ok(None);
                }

                let js_object = Object::from_entries(&event_js).unwrap();
                let obj_js: JsValue = JsValue::from(js_object);

                let obj = serde_wasm_bindgen::from_value(obj_js).unwrap();
                Ok(Some(obj))
            }
        }
    }

    async fn get_key(&self, key: ObjectId) -> anyhow::Result<Option<ObjectId>> {
        let maybe_event = self.find_one(key).await?;
        match maybe_event {
            None => {
                Ok(None)
            }
            Some(event) => {
                Ok(Some(event.obj_id()))
            }
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for WasmRepo {
    #[instrument(skip_all)]
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

#[instrument(skip_all)]
pub async fn open_db(db_name: &str) -> IdbDatabase {
    let mut db_req = IdbDatabase::open_u32(db_name, 1).unwrap();

    let on_upgrade_task = move |evt: &IdbVersionChangeEvent| -> Result<(), JsValue> {
        // Check if the object store exists; create it if it doesn't
        if !evt.db().object_store_names().any(|n| &n == "commit_log") {
            evt.db().create_object_store("commit_log").unwrap();
        }
        Ok(())
    };

    db_req.set_on_upgrade_needed(Some(on_upgrade_task));

    let idb = db_req.into_future().in_current_span().await.unwrap();
    idb
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
