#[cfg(test)]
mod test {
    use std::rc::Rc;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::crypto::utils::to_id;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::commit_log::{AppOperation, AppOperationType};
    use meta_server_emulator::server::meta_server::in_mem_meta_server::InMemMockServer;
    use meta_server_emulator::server::meta_server::{MetaServerEmulator, SyncRequest};
    use meta_server_emulator::server::meta_server::sqlite_meta_server::SqliteMockServer;

    #[test]
    fn app_full_test() {
        let mut server = InMemMockServer::new();
        let request = SyncRequest { vault_id: None, tail_id: None };
        let commit_log = server.sync(request);

        assert_eq!(1, commit_log.len());
        assert_eq!(AppOperationType::Update(AppOperation::Genesis), commit_log.first().unwrap().cmd_type);

        //println!("Very first commit log: {:?}", commit_log);

        //check whether the vault you are going to use already exists.
        // We need to have meta_db to be able to check if the vault exists
        let vault_name = "test";
        let vault_id = to_id(vault_name.to_string());

        let a_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let meta_db = commit_log::transform(Rc::new(commit_log.clone())).unwrap();
        if meta_db.meta_store.vaults_index.contains(vault_id.base64_text.as_str()) {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let sign_up_request = commit_log::sign_up_request(&commit_log.last().unwrap().key, &a_user_sig);
        server.send(&sign_up_request);

        let request = SyncRequest { vault_id: Some(vault_id.base64_text.clone()), tail_id: None };
        let commit_log = &server.sync(request);
        assert_eq!(3, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let commit_log_rc = Rc::new(commit_log.clone());
        let meta_db = commit_log::transform(commit_log_rc).unwrap();
        if !meta_db.meta_store.vaults_index.contains(vault_id.base64_text.as_str()) {
            panic!("The vault expected to be in the database")
        }

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let latest_event = commit_log.last();
        let join_request = commit_log::join_cluster_request(&latest_event.unwrap().key, &b_user_sig);
        server.send(&join_request);

        let request = SyncRequest { vault_id: Some(vault_id.base64_text.clone()), tail_id: None };
        let commit_log = server.sync(request);
        assert_eq!(5, commit_log.len());

        println!("{}", serde_json::to_string_pretty(&commit_log).unwrap());

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        assert_eq!(2, meta_db.meta_store.vault.unwrap().signatures.len());
    }

}