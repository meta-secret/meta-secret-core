#[cfg(test)]
mod test {
    use std::rc::Rc;

    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::events::join::join_cluster_request;
    use meta_secret_core::node::db::events::sign_up::SignUpRequest;
    use meta_secret_core::node::db::models::{
        Descriptors, GenericKvLogEvent, KvKeyId, KvLogEventRequest, KvLogEventUpdate,
        ObjectCreator, ObjectDescriptor, ObjectId,
    };
    use meta_secret_core::node::server::meta_server::{
        DataSync, DataTransport, MetaServerContext, MetaServerContextState,
    };
    use meta_secret_core::node::server::persistent_object_repo::ObjectFormation;
    use meta_secret_core::node::server::request::{SyncRequest, VaultSyncRequest};
    use meta_server_emulator::server::sqlite_migration::EmbeddedMigrationsTool;
    use meta_server_emulator::server::sqlite_store::SqlIteServer;

    struct ObjectUtilsStruct {}

    impl ObjectFormation for ObjectUtilsStruct {}

    impl SignUpRequest for ObjectUtilsStruct {}

    #[tokio::test]
    async fn test_brand_new_client_with_empty_request() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let server = get_sqlite_server(migration);

        let request = SyncRequest {
            vault: None,
            global_index: None,
        };
        let commit_log = server.sync_data(request).await;
        assert_eq!(1, commit_log.len());

        let obj_formation = ObjectUtilsStruct {};
        let expected_global_id_event =
            obj_formation.formation_event(&Descriptors::global_index(), &server.server_pk());
        let genesis_update = KvLogEventUpdate::Genesis {
            event: expected_global_id_event,
        };
        let expected_global_id_event = GenericKvLogEvent::Update(genesis_update);

        assert_eq!(expected_global_id_event, commit_log[0]);
    }

    fn get_sqlite_server(migration: EmbeddedMigrationsTool) -> SqlIteServer {
        SqlIteServer {
            conn_url: migration.db_url,
            context: MetaServerContextState {
                km: KeyManager::generate(),
                global_index_tail_id: None,
            },
        }
    }

    #[tokio::test]
    async fn test_sign_up() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let server = get_sqlite_server(migration);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_desc = ObjectDescriptor::vault(vault_name);
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
        let commit_log = server.sync_data(request).await;
        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        if meta_db
            .global_index_store
            .global_index
            .contains(vault_id.id.as_str())
        {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let obj_utils = ObjectUtilsStruct {};
        let sign_up_request = obj_utils.sign_up_generic_request(&a_user_sig);
        server.send_data(&sign_up_request).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.id.clone()),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = &server.sync_data(request).await;
        assert_eq!(5, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();

        let global_index = meta_db.global_index_store.global_index;

        if !global_index.contains(vault_id.id.as_str()) {
            panic!("The vault expected to be in the database")
        }
    }

    #[tokio::test]
    async fn test_join_cluster() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();
        let server = get_sqlite_server(migration);

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
        let obj_utils = ObjectUtilsStruct {};
        let sign_up_request = obj_utils.sign_up_generic_request(&a_user_sig);
        server.send_data(&sign_up_request).await;

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.obj_id.genesis_id.clone()),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = &server.sync_data(request).await;
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();

        //println!("tail id {:?}", &meta_db.vault_store.tail_id.clone().unwrap());

        let join_request = join_cluster_request(&meta_db.vault_store.tail_id.unwrap(), &b_user_sig);
        let join_request = GenericKvLogEvent::Request(KvLogEventRequest::JoinCluster {
            event: join_request,
        });

        server.send_data(&join_request).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.obj_id.genesis_id),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = server.sync_data(request).await;
        for log_event in &commit_log {
            println!("commit_log: {}", serde_json::to_string(&log_event).unwrap());
        }
        //println!("commit_log: {}", serde_json::to_string(&commit_log).unwrap());
        assert_eq!(7, commit_log.len());

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        assert_eq!(2, meta_db.vault_store.vault.unwrap().signatures.len());
    }
}
