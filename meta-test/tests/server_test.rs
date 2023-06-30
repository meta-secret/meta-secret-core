#[cfg(test)]
mod test {
    use std::marker::PhantomData;
    use std::rc::Rc;

    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::db::commit_log::MetaDbManager;
    use meta_secret_core::node::db::events::join::join_cluster_request;
    use meta_secret_core::node::db::events::object_id::ObjectId;
    use meta_secret_core::node::db::events::sign_up::SignUpRequest;
    use meta_secret_core::node::db::meta_db::MetaDb;
    use meta_secret_core::node::db::models::{
        GenericKvLogEvent, KvKeyId, KvLogEvent, KvLogEventRequest, KvLogEventUpdate, ObjectCreator, ObjectDescriptor,
    };
    use meta_secret_core::node::server::meta_server::{DataSync, DataSyncApi, DefaultMetaLogger, MetaLogger, MetaServerContext, MetaServerContextState};
    use meta_secret_core::node::server::persistent_object::{PersistentGlobalIndex, PersistentObject};
    use meta_secret_core::node::server::request::{SyncRequest, VaultSyncRequest};
    use meta_server_emulator::server::sqlite_migration::EmbeddedMigrationsTool;
    use meta_server_emulator::server::sqlite_store::{SqliteDbError, SqlIteRepo};

    #[tokio::test]
    async fn test_brand_new_client_with_empty_request() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let data_sync = build_data_sync(migration);

        let request = SyncRequest {
            vault: None,
            global_index: None,
        };
        let commit_log = data_sync.sync_data(request).await.unwrap();
        assert_eq!(1, commit_log.len());

        let expected_global_id_event = KvLogEvent::global_index_formation(&data_sync.context.server_pk());
        let genesis_update = KvLogEventUpdate::Genesis {
            event: expected_global_id_event,
        };
        let expected_global_id_event = GenericKvLogEvent::Update(genesis_update);

        assert_eq!(expected_global_id_event, commit_log[0]);
    }

    #[tokio::test]
    async fn test_sign_up() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let data_sync = build_data_sync(migration);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_desc = ObjectDescriptor::Vault { name: vault_name.to_string() };
        let vault_id = ObjectId::formation(&vault_desc);

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let request = SyncRequest {
            vault: None,
            global_index: None,
        };
        let commit_log = data_sync.sync_data(request).await;
        let meta_db: MetaDb = data_sync.meta_db_manager.transform(commit_log.unwrap()).unwrap();
        if meta_db
            .global_index_store
            .global_index
            .contains(vault_id.id_str().as_str())
        {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let sign_up_request_factory = SignUpRequest {};
        let sign_up_request = sign_up_request_factory.generic_request(&a_user_sig);
        data_sync.send_data(&sign_up_request, &DefaultMetaLogger::new()).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                tail_id: Some(vault_id.clone()),
            }),
            global_index: None,
        };

        let commit_log = data_sync.sync_data(request).await.unwrap();
        assert_eq!(5, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let meta_db = data_sync.meta_db_manager.transform(commit_log).unwrap();

        let global_index = meta_db.global_index_store.global_index;

        if !global_index.contains(vault_id.id_str().as_str()) {
            panic!("The vault expected to be in the database")
        }
    }

    #[tokio::test]
    async fn test_join_cluster() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let data_sync = build_data_sync(migration);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_id = KvKeyId::vault_formation(vault_name);

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        // if a vault is not present
        let obj_utils = SignUpRequest {};
        let sign_up_request = obj_utils.generic_request(&a_user_sig);
        data_sync.send_data(&sign_up_request, &DefaultMetaLogger::new()).await;

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                tail_id: Some(vault_id.obj_id().genesis_id()),
            }),
            global_index: None,
        };

        let commit_log = data_sync.sync_data(request).await.unwrap();
        let meta_db = data_sync.meta_db_manager.transform(commit_log).unwrap();

        //println!("tail id {:?}", &meta_db.vault_store.tail_id.clone().unwrap());

        let join_request = join_cluster_request(&meta_db.vault_store.tail_id.unwrap(), &b_user_sig);
        let join_request = GenericKvLogEvent::Request(KvLogEventRequest::JoinCluster {
            event: join_request,
        });

        data_sync.send_data(&join_request, &DefaultMetaLogger::new()).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                tail_id: Some(vault_id.obj_id().genesis_id()),
            }),
            global_index: None,
        };

        let commit_log = data_sync.sync_data(request).await.unwrap();
        for log_event in &commit_log {
            println!("commit_log: {}", serde_json::to_string(log_event).unwrap());
        }
        //println!("commit_log: {}", serde_json::to_string(&commit_log).unwrap());
        assert_eq!(7, commit_log.len());

        let meta_db = data_sync.meta_db_manager.transform(commit_log).unwrap();
        assert_eq!(2, meta_db.vault_store.vault.unwrap().signatures.len());
    }

    fn build_data_sync(migration: EmbeddedMigrationsTool) -> DataSync<SqlIteRepo, SqliteDbError> {
        let context_rc = Rc::new(MetaServerContextState::default());

        let sqlite_repo = SqlIteRepo {
            conn_url: migration.db_url,
            context: context_rc.clone(),
        };

        let sqlite_repo_rc = Rc::new(sqlite_repo);

        let persistent_object = PersistentObject {
            repo: sqlite_repo_rc.clone(),
            global_index: PersistentGlobalIndex {
                repo: sqlite_repo_rc.clone(),
                _phantom: PhantomData,
            },
        };

        let persistent_object_rc = Rc::new(persistent_object);

        DataSync {
            persistent_obj: persistent_object_rc.clone(),
            repo: sqlite_repo_rc,
            context: context_rc,
            meta_db_manager: Rc::from(MetaDbManager {
                persistent_obj: persistent_object_rc,
            }),
        }
    }
}
