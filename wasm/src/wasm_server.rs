use std::rc::Rc;
use std::time::Duration;

use meta_secret_core::node::app::meta_manager::UserCredentialsManager;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::db::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::meta_db::meta_db_service::MetaDbService;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::logger::{LoggerId, MetaLogger};
use meta_secret_core::node::server::data_sync::{DataSync, MetaServerContextState};
use meta_secret_core::node::server::server_app::ServerApp;

use crate::wasm_repo::{WasmMetaLogger, WasmRepo};

pub struct WasmServer<Repo: KvLogEventRepo> {
    pub server: Rc<ServerApp<Repo, WasmMetaLogger>>,
}

impl<Repo: KvLogEventRepo> WasmServer<Repo> {
    
    pub async fn new(
        repo: Rc<Repo>,
        data_transfer: Rc<MpscDataTransfer>, meta_db_service: Rc<MetaDbService<Repo, WasmMetaLogger>>,
        persistent_obj: Rc<PersistentObject<Repo, WasmMetaLogger>>,
    ) -> WasmServer<Repo> {
        let logger = Rc::new(WasmMetaLogger {
            id: LoggerId::Server
        });

        logger.info("New wasm server");

        let server_creds = repo.get_or_generate_user_creds(
            String::from("q"),
            String::from("server"),
        ).await;

        let data_sync = DataSync {
            persistent_obj: persistent_obj.clone(),
            repo: repo.clone(),
            context: Rc::new(MetaServerContextState::from(&server_creds)),
            logger: logger.clone(),
        };

        let server = ServerApp {
            timeout: Duration::from_secs(1),
            data_sync,
            data_transfer: data_transfer.mpsc_client.clone(),
            logger: logger.clone(),
            meta_db_service,
        };

        WasmServer {
            server: Rc::new(server)
        }
    }
}