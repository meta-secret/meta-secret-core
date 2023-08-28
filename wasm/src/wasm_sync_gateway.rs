use std::rc::Rc;
use meta_secret_core::models::{UserSignature, VaultDoc};

use meta_secret_core::node::app::sync_gateway::SyncGateway;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::logger::MetaLogger;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;

use crate::commit_log::{WasmRepo};

pub enum WasmSyncGateway {
    WasmGateway { gateway: SyncGateway }
}

impl WasmSyncGateway {
    pub fn new(data_transfer: Rc<MpscDataTransfer>, gateway_id: String, logger: Rc<dyn MetaLogger>) -> WasmSyncGateway {
        let repo = Rc::new(WasmRepo::default());
        WasmSyncGateway::new_with_custom_repo(repo, data_transfer, gateway_id, logger)
    }

    pub fn new_with_custom_repo(
        repo: Rc<WasmRepo>,
        data_transfer: Rc<MpscDataTransfer>,
        gateway_id: String,
        logger: Rc<dyn MetaLogger>
    ) -> WasmSyncGateway {
        logger.info("Create new wasm sync gateway instance");

        let persistent_object = {
            let obj = PersistentObject::new(repo.clone(), logger.clone());
            Rc::new(obj)
        };

        WasmSyncGateway::WasmGateway {
            gateway: SyncGateway {
                id: gateway_id,
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

    pub async fn sync_shared_secrets(&self, vault: &VaultDoc) {
        match self {
            WasmSyncGateway::WasmGateway { gateway } => {
                gateway.send_shared_secrets(vault).await;
            }
        }
    }
}
