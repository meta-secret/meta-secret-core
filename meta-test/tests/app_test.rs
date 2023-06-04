#[cfg(test)]
mod test {
    use std::rc::Rc;

    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::events::global_index;
    use meta_secret_core::node::db::events::join::join_cluster_request;
    use meta_secret_core::node::db::events::sign_up::sign_up_request;
    use meta_secret_core::node::db::models::{KeyIdGen, KvKeyId, ObjectType, VaultId};
    use meta_server_emulator::server::meta_server::MetaServerNode;
    use meta_server_emulator::server::meta_server::{
        MetaServer, SyncRequest, VaultSyncRequest,
    };
    use meta_server_emulator::server::slite_migration::EmbeddedMigrationsTool;
    use meta_server_emulator::server::sqlite_store::SqlIteStore;

    #[tokio::test]
    async fn test_brand_new_client_with_empty_request() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let store = SqlIteStore {
            conn_url: migration.db_url,
        };
        let server = MetaServerNode::new(store);

        let request = SyncRequest {
            vault: None,
            global_index: None,
        };
        let commit_log = server.sync(request).await;
        assert_eq!(1, commit_log.len());

        let expected_global_idx_formation_event =
            global_index::generate_global_index_formation_event(&server.server_pk());
        assert_eq!(expected_global_idx_formation_event, commit_log[0]);
    }

    #[tokio::test]
    async fn test_sign_up() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();

        let store = SqlIteStore {
            conn_url: migration.db_url,
        };
        let server = MetaServerNode::new(store);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_id = VaultId::build(vault_name, ObjectType::Vault);

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
        let commit_log = server.sync(request).await;
        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        if meta_db
            .global_index_store
            .global_index
            .contains(vault_id.vault_id.as_str())
        {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let sign_up_request = sign_up_request(&a_user_sig);
        server.send(&sign_up_request).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.vault_id.clone()),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = &server.sync(request).await;
        assert_eq!(5, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();

        let global_index = meta_db.global_index_store.global_index;

        if !global_index.contains(vault_id.vault_id.as_str()) {
            panic!("The vault expected to be in the database")
        }
    }

    #[tokio::test]
    async fn test_join_cluster() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();
        let store = SqlIteStore {
            conn_url: migration.db_url,
        };
        let server = MetaServerNode::new(store);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_id = KvKeyId::object_foundation(vault_name, ObjectType::Vault);

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo {
            device_id: "a".to_string(),
            device_name: "a".to_string(),
        };
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        // if a vault is not present
        let sign_up_request = sign_up_request(&a_user_sig);
        server.send(&sign_up_request).await;

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.key_id.clone()),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = &server.sync(request).await;
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();

        //println!("tail id {:?}", &meta_db.vault_store.tail_id.clone().unwrap());

        let join_request = join_cluster_request(
            &meta_db.vault_store.tail_id.unwrap(),
            &b_user_sig
        );

        server.send(&join_request).await;

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.key_id),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = server.sync(request).await;
        for log_event in &commit_log {
            println!("commit_log: {}", serde_json::to_string(&log_event).unwrap());
        }
        //println!("commit_log: {}", serde_json::to_string(&commit_log).unwrap());
        assert_eq!(7, commit_log.len());

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        assert_eq!(2, meta_db.vault_store.vault.unwrap().signatures.len());
    }
}
