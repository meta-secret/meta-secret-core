use anyhow::bail;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{error, instrument};

use meta_secret_core::node::db::events::generic_log_event::{
    GenericKvLogEvent, ObjIdExtractor, ToGenericEvent,
};
use meta_secret_core::node::db::repo::generic_db::{
    CommitLogDbConfig, DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo, SaveCommand,
};

use anyhow::Result;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_secret_core::node::app::sync::sync_protocol::SyncProtocol;
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_server_node::server::server_app::ServerApp;
use rexie::*;

pub struct WasmRepo {
    pub db_name: String,
    pub store_name: String,

    rexie: Rexie,
}

impl WasmRepo {
    pub async fn default() -> Self {
        let db_name = "meta-secret".to_string();
        let store_name = "commit_log".to_string();

        let rexie = Self::build_rexie(db_name.as_str(), store_name.as_str()).await;

        Self {
            db_name,
            store_name,
            rexie,
        }
    }

    async fn build_rexie(db_name: &str, store_name: &str) -> Rexie {
        Rexie::builder(db_name)
            .version(1)
            .add_object_store(ObjectStore::new(store_name))
            .build()
            .await
            .expect("Failed to create REXie")
    }
}

impl WasmRepo {
    pub async fn server() -> WasmRepo {
        let db_name = String::from("meta-secret-server");
        let store_name = "commit_log".to_string();

        let rexie = Self::build_rexie(db_name.as_str(), store_name.as_str()).await;

        WasmRepo {
            db_name,
            store_name,
            rexie,
        }
    }

    pub async fn virtual_device() -> WasmRepo {
        let db_name = String::from("meta-secret-v-device");
        let store_name = String::from("commit_log");

        let rexie = Self::build_rexie(db_name.as_str(), store_name.as_str()).await;

        WasmRepo {
            db_name,
            store_name,
            rexie,
        }
    }

    pub async fn virtual_device_2() -> WasmRepo {
        let db_name = String::from("meta-secret-v-device-2");
        let store_name = String::from("commit_log");

        let rexie = Self::build_rexie(db_name.as_str(), store_name.as_str()).await;

        WasmRepo {
            db_name,
            store_name,
            rexie,
        }
    }
}

#[async_trait(? Send)]
impl SaveCommand for WasmRepo {
    #[instrument(skip_all)]
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        let generic_event = event.to_generic();
        let maybe_key = self.get_key(generic_event.obj_id()).await?;
        if maybe_key.is_some() {
            bail!(
                "Wrong behaviour. Event already exists: {:?}",
                &generic_event
            );
        };

        let store_name = self.store_name.as_str();

        let tx = self
            .rexie
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .unwrap();

        let store = tx.store(store_name).unwrap();

        let js_value = serde_wasm_bindgen::to_value(&generic_event).unwrap();
        let id_str = generic_event.obj_id().id_str();
        let obj_id_js = serde_wasm_bindgen::to_value(id_str.as_str()).unwrap();

        let op_result = store.add(&js_value, Some(&obj_id_js)).await;
        if op_result.is_err() {
            error!("Failed to save event: {:?}", &generic_event);
        }

        op_result.unwrap();

        // Waits for the transaction to complete
        tx.done().await.unwrap();

        Ok(generic_event.obj_id())
    }
}

#[async_trait(? Send)]
impl FindOneQuery for WasmRepo {
    #[instrument(skip_all)]
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let store_name = self.store_name.as_str();

        let tx = self
            .rexie
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .unwrap();

        let store = tx.store(store_name).unwrap();

        // Convert it to `JsValue`
        let js_key = serde_wasm_bindgen::to_value(key.id_str().as_str()).unwrap();

        // Add the employee to the store
        let maybe_event_js = store.get(js_key).await.unwrap();

        match maybe_event_js {
            None => Ok(None),
            Some(event_js) => {
                if event_js.is_undefined() {
                    return Ok(None);
                }

                let obj = serde_wasm_bindgen::from_value(event_js).unwrap();
                Ok(Some(obj))
            }
        }
    }

    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        let maybe_event = self.find_one(key).await?;
        match maybe_event {
            None => Ok(None),
            Some(event) => Ok(Some(event.obj_id())),
        }
    }
}

#[async_trait(? Send)]
impl DeleteCommand for WasmRepo {
    #[instrument(skip_all)]
    async fn delete(&self, key: ArtifactId) {
        let store_name = self.store_name.as_str();

        let tx = self
            .rexie
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .unwrap();

        let store = tx.store(store_name).unwrap();

        let js_key = serde_wasm_bindgen::to_value(key.id_str().as_str()).unwrap();
        store.delete(js_key).await.unwrap();
    }
}

#[async_trait(? Send)]
impl DbCleanUpCommand for WasmRepo {
    async fn db_clean_up(&self) {
        // In this implementation, we'll delete the entire store

        let store_name = self.store_name.as_str();

        // First create a transaction to clear the store
        let tx = self
            .rexie
            .transaction(&[store_name], TransactionMode::ReadWrite)
            .unwrap();

        let store = tx.store(store_name).unwrap();

        // Clear all records in the store
        let clear_result = store.clear().await;
        if clear_result.is_err() {
            error!(
                "Failed to clear store {}: {:?}",
                store_name,
                clear_result.err()
            );
        }

        // Ensure transaction completes
        if let Err(e) = tx.done().await {
            error!("Failed to complete clear transaction: {:?}", e);
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

pub struct WasmSyncProtocol<Repo: KvLogEventRepo> {
    pub server: Arc<ServerApp<Repo>>,
}

impl<Repo: KvLogEventRepo> SyncProtocol for WasmSyncProtocol<Repo> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.server.handle_client_request(request).await
    }
}
