use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use async_mutex::Mutex;
use tracing::Level;

use meta_secret_core::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use meta_secret_core::node::db::events::common::{MemPoolObject, ObjectCreator};
use meta_secret_core::node::db::events::db_tail::DbTailObject;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::events::local::KvLogEventLocal;
use meta_secret_core::node::db::events::object_descriptor::ObjectDescriptor;
use meta_secret_core::node::db::events::object_id::{IdGen, ObjectId};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;

use crate::common::native_app_state_manager::NativeApplicationStateManager;

mod common;

fn setup_logging() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).pretty().init();
}

#[tokio::test]
async fn server_test() {
    setup_logging();

    let client_repo = Arc::new(InMemKvLogEventRepo {
        db: Arc::new(Mutex::new(HashMap::default())),
    });

    let server_repo = Arc::new(InMemKvLogEventRepo {
        db: Arc::new(Mutex::new(HashMap::default())),
    });

    let device_repo = Arc::new(InMemKvLogEventRepo {
        db: Arc::new(Mutex::new(HashMap::default())),
    });

    let app_manager =
        NativeApplicationStateManager::init(client_repo.clone(), server_repo.clone(), device_repo.clone()).await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    let sign_up_request = GenericAppStateRequest::SignUp(SignUpRequest {
        vault_name: String::from("q"),
        device_name: String::from("client"),
    });

    app_manager
        .meta_client_proxy
        .send_request(sign_up_request.clone())
        .await;

    async_std::task::sleep(Duration::from_secs(1)).await;

    app_manager
        .meta_client_proxy
        .send_request(sign_up_request.clone())
        .await;

    async_std::task::sleep(Duration::from_secs(1)).await;

    app_manager
        .meta_client_proxy
        .send_request(sign_up_request.clone())
        .await;

    async_std::task::sleep(Duration::from_secs(3)).await;

    {
        let events = server_repo.as_ref().db.as_ref().clone().lock().await.deref().clone();

        let verifier = MetaAppTestVerifier {
            vault_name: String::from("q"),
            events,
        };

        verifier.server_verification();
    };

    {
        let events = client_repo.as_ref().db.as_ref().clone().lock().await.deref().clone();

        let verifier = MetaAppTestVerifier {
            vault_name: String::from("q"),
            events,
        };

        verifier.client_verification();
    };

    {
        let events = device_repo.as_ref().db.as_ref().clone().lock().await.deref().clone();

        let verifier = MetaAppTestVerifier {
            vault_name: String::from("q"),
            events,
        };

        verifier.device_verification();
    };
}

struct MetaAppTestVerifier {
    vault_name: String,
    events: HashMap<ObjectId, GenericKvLogEvent>,
}

impl MetaAppTestVerifier {
    fn device_verification(&self) {
        assert_eq!(13, self.events.len());
        self.common_verification();
    }

    fn server_verification(&self) {
        assert_eq!(12, self.events.len());
        self.common_verification();
    }

    fn client_verification(&self) {
        assert_eq!(14, self.events.len());

        self.common_verification();

        self.verify_db_tail();
        self.verify_mem_pool();
    }

    fn common_verification(&self) {
        self.verify_meta_vault();
        self.verify_user_creds();

        self.verify_global_index();
        self.verify_meta_pass();
        self.verify_vault();
    }

    fn verify_vault(&self) {
        let vault_unit = ObjectId::unit(&ObjectDescriptor::Vault {
            vault_name: self.vault_name.clone(),
        });
        let vault_genesis = vault_unit.next();
        let vault_sign_up_update = vault_genesis.next();
        let vault_join_request = vault_sign_up_update.next();
        let vault_join_update = vault_join_request.next();

        assert!(self.events.contains_key(&vault_unit));
        assert!(self.events.contains_key(&vault_genesis));
        assert!(self.events.contains_key(&vault_sign_up_update));
        assert!(self.events.contains_key(&vault_join_request));
        assert!(self.events.contains_key(&vault_join_update));
    }

    fn verify_user_creds(&self) {
        let creds_unit_id = ObjectId::unit(&ObjectDescriptor::UserCreds);
        assert!(self.events.contains_key(&creds_unit_id));
    }

    fn verify_meta_pass(&self) {
        let meta_pass_unit = ObjectId::unit(&ObjectDescriptor::MetaPassword {
            vault_name: self.vault_name.clone(),
        });
        assert!(self.events.contains_key(&meta_pass_unit));
        assert!(self.events.contains_key(&meta_pass_unit.next()));
    }

    fn verify_global_index(&self) {
        let gi_unit = ObjectId::unit(&ObjectDescriptor::GlobalIndex);
        let gi_genesis = gi_unit.next();
        let gi_vault_record = gi_genesis.next();

        assert!(self.events.contains_key(&gi_unit));
        assert!(self.events.contains_key(&gi_genesis));
        assert!(self.events.contains_key(&gi_vault_record));
    }

    fn verify_meta_vault(&self) {
        let meta_vault_unit_id = ObjectId::unit(&ObjectDescriptor::MetaVault);
        assert!(self.events.contains_key(&meta_vault_unit_id));
    }

    fn verify_db_tail(&self) {
        let db_tail_unit = ObjectId::unit(&ObjectDescriptor::DbTail);
        assert!(self.events.contains_key(&db_tail_unit));

        let db_tail_event = self.events.get(&db_tail_unit).unwrap();
        if let GenericKvLogEvent::LocalEvent(KvLogEventLocal::DbTail { event }) = db_tail_event {
            if let Some(ObjectId::Regular { unit_id, id, prev_id }) = &event.value.maybe_global_index_id {
                assert_eq!(String::from("GlobalIndex:index::0"), unit_id.clone());
                assert_eq!(String::from("GlobalIndex:index::1"), prev_id.clone());
                assert_eq!(String::from("GlobalIndex:index::2"), id.clone());
            } else {
                panic!("Invalid Global Index Event");
            }

            if let DbTailObject::Id { tail_id } = &event.value.meta_pass_id {
                let meta_pass_id = ObjectId::genesis(&ObjectDescriptor::MetaPassword {
                    vault_name: String::from("q"),
                });
                assert_eq!(meta_pass_id, tail_id.clone());
            } else {
                panic!("Invalid Meta Pass Id");
            }

            //TODO add verification for vault_id, maybe_mem_pool_id
        } else {
            panic!("Invalid DbTail event");
        }
    }
    fn verify_mem_pool(&self) {
        let mem_pool_unit_id = ObjectId::mempool_unit();
        let mem_pool_event = &self.events[&mem_pool_unit_id];
        if let GenericKvLogEvent::MemPool(MemPoolObject::JoinRequest { event }) = mem_pool_event {
            assert_eq!(String::from("client"), event.value.vault.device.device_name);
        } else {
            panic!("Invalid mem pool event");
        }
    }
}
