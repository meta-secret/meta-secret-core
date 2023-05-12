#[cfg(test)]
mod test {
    use std::rc::Rc;

    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::crypto::utils::to_id;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::events::global_index;
    use meta_secret_core::node::db::events::sign_up::sign_up_request;
    use meta_server_emulator::server::meta_server::sqlite_meta_server::SqliteMockServer;
    use meta_server_emulator::server::meta_server::{
        MetaServerEmulator, SyncRequest, VaultSyncRequest,
    };
    use meta_server_emulator::server::slite_db::EmbeddedMigrationsTool;

    #[test]
    fn test_brand_new_client_with_empty_request() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();
        let mut server = SqliteMockServer::new(migration.db_url.as_str());

        let request = SyncRequest {
            vault: None,
            global_index: None,
        };
        let commit_log = server.sync(request);
        assert_eq!(1, commit_log.len());

        let expected_global_idx_formation_event =
            global_index::generate_global_index_formation_event(&server.server_pk());
        assert_eq!(expected_global_idx_formation_event, commit_log[0]);
    }

    #[test]
    fn test_sign_up() {
        let migration = EmbeddedMigrationsTool::default();
        migration.migrate();
        let mut server = SqliteMockServer::new(migration.db_url.as_str());

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_id = to_id(vault_name);

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
        let commit_log = server.sync(request);
        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        if meta_db
            .global_index_store
            .global_index
            .contains(vault_id.base64_text.as_str())
        {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let sign_up_request = sign_up_request(&a_user_sig);
        server.send(&sign_up_request);

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.base64_text.clone()),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = &server.sync(request);
        assert_eq!(5, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();

        let global_index = meta_db.global_index_store.global_index;

        if !global_index.contains(vault_id.base64_text.as_str()) {
            panic!("The vault expected to be in the database")
        }
    }

    #[test]
    fn test_join_cluster() {
        // same as sign_up and:
        /*
        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let latest_event = commit_log.last();
        let join_request = join_cluster_request(&latest_event.unwrap().key, &b_user_sig);
        server.send(&join_request);

        let request = SyncRequest {
            vault: Some(VaultSyncRequest {
                vault_id: Some(vault_id.base64_text),
                tail_id: None,
            }),
            global_index: None,
        };

        let commit_log = server.sync(request);
        assert_eq!(6, commit_log.len());

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        assert_eq!(2, meta_db.vault_store.vault.unwrap().signatures.len());
         */
    }
}
