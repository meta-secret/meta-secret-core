use async_mutex::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use meta_secret_core::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use meta_secret_core::node::db::events::common::ObjectCreator;
use meta_secret_core::node::db::events::object_descriptor::ObjectDescriptor;
use meta_secret_core::node::db::events::object_id::{IdGen, ObjectId};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;

use crate::common::native_app_state_manager::NativeApplicationStateManager;

mod common;

#[tokio::test]
async fn server_test() {
    let server_db = Arc::new(Mutex::new(HashMap::default()));
    let server_repo = Arc::new(InMemKvLogEventRepo { db: server_db.clone() });

    let app_manager = NativeApplicationStateManager::init(server_db).await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    println!("MUST HAPPEN 3 SEC AFTER VD setup");

    let sign_up_request = GenericAppStateRequest::SignUp(SignUpRequest {
        vault_name: String::from("q"),
        device_name: String::from("client"),
    });

    app_manager
        .meta_client_proxy
        .send_request(sign_up_request.clone())
        .await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    app_manager
        .meta_client_proxy
        .send_request(sign_up_request.clone())
        .await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    print_events(server_repo).await;
}

async fn print_events(server_repo: Arc<InMemKvLogEventRepo>) {
    let vault_name = String::from("q");

    let events = server_repo.as_ref().db.as_ref().clone().lock().await;

    assert_eq!(10, events.len());

    {
        let meta_vault_unit_id = ObjectId::unit(&ObjectDescriptor::MetaVault);
        assert!(events.contains_key(&meta_vault_unit_id));
    }

    {
        let creds_unit_id = ObjectId::unit(&ObjectDescriptor::UserCreds);
        assert!(events.contains_key(&creds_unit_id));
    }

    {
        // Global Index
        let gi_unit = ObjectId::unit(&ObjectDescriptor::GlobalIndex);
        let gi_genesis = gi_unit.next();

        assert!(events.contains_key(&gi_unit));
        assert!(events.contains_key(&gi_genesis));
        assert!(events.contains_key(&gi_genesis.next()));
    }
    {
        //Vault
        let vault_unit = ObjectId::unit(&ObjectDescriptor::Vault {
            vault_name: vault_name.clone(),
        });
        let vault_genesis = vault_unit.next();
        let vault_regular = vault_genesis.next();

        assert!(events.contains_key(&vault_unit));
        assert!(events.contains_key(&vault_genesis));
        assert!(events.contains_key(&vault_regular));
    }

    //TODO check server_pk for genesis events

    {
        let meta_pass_unit = ObjectId::unit(&ObjectDescriptor::MetaPassword { vault_name });
        assert!(events.contains_key(&meta_pass_unit));
        assert!(events.contains_key(&meta_pass_unit.next()));
    }
}
