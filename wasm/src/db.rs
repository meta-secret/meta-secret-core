use wasm_bindgen::JsValue;
use web_sys::DomException;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use meta_secret_core::node::server::persistent_object_repo::PersistentObjectRepo;
use crate::{idbGet, idbSave};
use crate::commit_log::CommitLogWasmRepo;

pub const DB_NAME: &str = "meta_secret_db";

#[derive(thiserror::Error, Debug)]
pub enum WasmDbError {
    #[error("IndexedDb error")]
    JsIndexedDbError(DomException),

    #[error(transparent)]
    SerdeWasmBindGenError {
        #[from]
        source: serde_wasm_bindgen::Error,
    },

    #[error("JsValue error")]
    JsValueError(JsValue),

    #[error("Db error: {0}")]
    DbCustomError(String),
}


impl KvLogEventRepo<WasmDbError> for CommitLogWasmRepo {

}

pub struct MetaDbManagerWasm {

}

//impl MetaDbManager<WasmDbError> for CommitLogWasmRepo {

//}

//impl PersistentObjectRepo<WasmDbError> for MetaDbManagerWasm {

//}