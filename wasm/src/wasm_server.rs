use std::rc::Rc;
use std::time::Duration;

use meta_secret_core::node::app::meta_app::UserCredentialsManager;
use meta_secret_core::node::db::commit_log::MetaDbManager;
use meta_secret_core::node::db::persistent_object::PersistentObject;
use meta_secret_core::node::server::data_sync::{DataSync, MetaServerContextState};
use meta_secret_core::node::server::server_app::{MpscDataTransfer, ServerApp};

use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::db::WasmDbError;
use crate::log;

pub struct WasmServer {
    server: ServerApp<WasmRepo, WasmMetaLogger, WasmDbError>,
}

impl WasmServer {
    pub async fn run(data_transfer: Rc<MpscDataTransfer>) -> WasmServer {
        log("Run server!!!!!!!!!!!!!!!!!11");

        let repo = Rc::new(WasmRepo::server());
        let logger = Rc::new(WasmMetaLogger {});
        let persistent_obj = {
            let obj = PersistentObject::new(repo.clone(), logger.clone());
            Rc::new(obj)
        };
        let meta_db_manager = MetaDbManager::from(persistent_obj.clone());

        let server_creds = repo.get_or_generate_user_creds(
            String::from("q"),
            String::from("server"),
        ).await;

        let data_sync = DataSync {
            persistent_obj: persistent_obj.clone(),
            repo: repo.clone(),
            context: Rc::new(MetaServerContextState::from(&server_creds)),
            meta_db_manager: Rc::new(meta_db_manager),
            logger: logger.clone(),
        };

        let server = ServerApp {
            timeout: Duration::from_secs(1),
            data_sync,
            data_transfer: data_transfer.mpsc_receiver.clone(),
            logger: logger.clone(),
        };

        server.run().await;

        WasmServer {
            server
        }
    }
}