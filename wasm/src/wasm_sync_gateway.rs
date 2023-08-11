use std::rc::Rc;

use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::db::persistent_object::PersistentObject;
use meta_secret_core::node::server::server_app::MpscDataTransfer;

use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::log;

pub enum WasmSyncGateway {
    WasmGateway { gateway: SyncGateway<WasmRepo, WasmMetaLogger, WasmDbError> }
}

impl WasmSyncGateway {
    pub fn new(data_transfer: Rc<MpscDataTransfer>) -> WasmSyncGateway {
        let repo = Rc::new(WasmRepo::default());
        WasmSyncGateway::new_with_custom_repo(repo, data_transfer)
    }

    pub fn new_with_custom_repo(repo: Rc<WasmRepo>, data_transfer: Rc<MpscDataTransfer>) -> WasmSyncGateway {
        let logger = Rc::new(WasmMetaLogger {});
        let persistent_object = {
            let obj = PersistentObject::new(repo.clone(), logger.clone());
            Rc::new(obj)
        };

        WasmSyncGateway::WasmGateway {
            gateway: SyncGateway {
                logger,
                repo,
                persistent_object,
                data_transfer: data_transfer.mpsc_sender.clone(),
            }
        }
    }

    pub async fn sync(&self) {
        match self {
            WasmSyncGateway::WasmGateway { gateway } => {
                gateway.sync().await;
            }
        }
    }
}
