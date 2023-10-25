use anyhow::anyhow;
use async_trait::async_trait;
use indexed_db_futures::prelude::*;
use indexed_db_futures::IdbDatabase;
use js_sys::Object;
use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor};
use meta_secret_core::node::db::events::object_id::ObjectId;
use meta_secret_core::node::db::generic_db::{
    CommitLogDbConfig, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};
use tracing::{Instrument, instrument};
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;
use tracing::Level;

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
        let db = open_db(self.db_name.as_str()).in_current_span().await;
        db.delete().unwrap();
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
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

#[async_trait(? Send)]
impl FindOneQuery for WasmRepo {

    #[instrument(level = Level::DEBUG)]
    async fn find_one(&self, key: ObjectId) -> anyhow::Result<Option<GenericKvLogEvent>> {
        let db = open_db(self.db_name.as_str()).in_current_span().await;
        let tx = db.transaction_on_one(self.store_name.as_str()).unwrap();
        let store = tx.object_store(self.store_name.as_str()).unwrap();
        let future: OptionalJsValueFuture = store.get_owned(key.id_str().as_str()).unwrap();
        let maybe_obj_js: Option<JsValue> = future.in_current_span().await.unwrap();

        if let Some(obj_js) = maybe_obj_js {
            if obj_js.is_undefined() {
                return Ok(None);
            }

            match Object::from_entries(&obj_js) {
                Ok(obj_js) => {
                    let obj_js: JsValue = JsValue::from(obj_js);

                    let obj = serde_wasm_bindgen::from_value(obj_js)?;
                    Ok(Some(obj))
                }
                Err(_) => Err(anyhow!("IndexedDb object error parsing")),
            }
        } else {
            Ok(None)
        }
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
