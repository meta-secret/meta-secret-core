use std::any::TypeId;
use wasm_bindgen::prelude::*;
use meta_secret_core::crypto::keys::KeyManager;
use meta_secret_core::models::UserCredentials;
use meta_secret_core::node::app::meta_app::{MetaVaultManager, UserCredentialsManager};
use meta_secret_core::node::db::models::GenericKvLogEvent;
use meta_secret_core::node::server::meta_server::DataSyncApi;
use crate::commit_log::{WasmMetaLogger, WasmRepo};
use crate::log;
use crate::wasm_app::get_data_sync;

#[wasm_bindgen]
pub struct WasmMetaServer {

}

#[wasm_bindgen]
impl WasmMetaServer {
    pub async fn run_server() {
        let logger = Some(WasmMetaLogger {});

        let server_repo = WasmRepo::server();

        let maybe_server_creds = server_repo.find_user_creds()
            .await
            .unwrap();

        match maybe_server_creds {
            Some(creds) => {
                log("Wasm::run_server()");

                let client_events: Vec<GenericKvLogEvent> = vec![];

                let client_repo = WasmRepo::default();
                let client_data_sync = get_data_sync(client_repo, &creds);
                for client_event in client_events {
                    client_data_sync.send_data(&client_event, &logger).await;
                }
            }
            None => {
                let logger = WasmMetaLogger {};

                let meta_vault = server_repo
                    .create_meta_vault("meta-server-vault".to_string(), "meta-server-device".to_string(), &logger)
                    .await
                    .unwrap();

                let security_box = KeyManager::generate_security_box(meta_vault.name);
                let user_sig = security_box.get_user_sig(&meta_vault.device);
                let creds = UserCredentials::new(security_box, user_sig);
                server_repo.save_user_creds(creds).await.unwrap();
            }
        };
    }
}

/*
let a_s_box = KeyManager::generate_security_box("qwe".to_string());
    let a_device = DeviceInfo {
        device_id: "a".to_string(),
        device_name: "a".to_string(),
    };
    let user_sig = a_s_box.get_user_sig(&a_device);

    let event = KvLogEvent {
        key: KvKey::formation(&ObjectDescriptor::Tail),
        value: DbTail {
            vault: ObjectId::formation(&ObjectDescriptor::Vault { name: "test_vault".to_string() }),
            global_index: ObjectId::formation(&ObjectDescriptor::GlobalIndex),
        },
    };
    let generic_evt = GenericKvLogEvent::Local(KvLogEventLocal::Tail { event });

    alert("yay!!!");

    meta_vault_manager.save_event(&generic_evt).await;
    meta_vault_manager.save_event(&generic_evt).await;
*/