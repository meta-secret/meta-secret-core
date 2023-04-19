#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::rc::Rc;
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::crypto::utils::to_id;
    use crate::models::DeviceInfo;
    use crate::node::db::commit_log;
    use crate::node::db::commit_log::{AppOperation, AppOperationType, KvLogEvent, transform};

    #[test]
    fn app_full_test() {
        let mut server = MockServer::new();
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

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        if meta_db.meta_store.vaults_index.contains(vault_id.base64_text.as_str()) {
            panic!("The vault already exists");
        }

        // if a vault is not present
        let sign_up_request = commit_log::sign_up_request(&a_user_sig);
        server.request(&sign_up_request);

        let request = SyncRequest { vault_id: Some(vault_id.base64_text.clone()), tail_id: None };
        let commit_log = server.sync(request);
        assert_eq!(3, commit_log.len());

        //find if your vault is already exists
        // - only server can create new vaults
        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        if !meta_db.meta_store.vaults_index.contains(vault_id.base64_text.as_str()) {
            panic!("The vault expected to be in the database")
        }

        let b_s_box = KeyManager::generate_security_box(vault_name.to_string());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let join_request = commit_log::join_cluster_request(&b_user_sig);
        server.request(&join_request);

        let request = SyncRequest { vault_id: Some(vault_id.base64_text.clone()), tail_id: None };
        let commit_log = server.sync(request);
        assert_eq!(5, commit_log.len());

        println!("{}", serde_json::to_string_pretty(&commit_log).unwrap());

        let meta_db = commit_log::transform(Rc::new(commit_log)).unwrap();
        assert_eq!(2, meta_db.meta_store.vault.unwrap().signatures.len());
    }

    struct MockServer {
        km: KeyManager,
        // vault name -> commit log
        db: HashMap<String, Vec<KvLogEvent>>,
        server_events: Vec<KvLogEvent>,
    }

    impl MockServer {
        fn new() -> Self {
            let km = KeyManager::generate();
            let genesis_event: KvLogEvent = commit_log::generate_genesis_event(&km.dsa.public_key());
            //let vault_name = "test";
            //let vault_id = to_id(vault_name.to_string());
            //let vaults_index_event = commit_log::vaults_index_event(&vault_id);
            //db.insert(vault_name.to_string(), test_vault_commit_log);

            let server_events = vec![genesis_event];
            let db = HashMap::new();

            Self { km, db, server_events }
        }

        fn sync(&self, request: SyncRequest) -> Vec<KvLogEvent> {
            let vault_and_tail = (request.vault_id, request.tail_id);
            match vault_and_tail {
                (Some(vault_id), None) => {
                    match self.db.get(vault_id.as_str()) {
                        None => {
                            let mut commit_log = vec![];
                            commit_log.extend(self.server_events.clone());
                            commit_log
                        }
                        Some(vault) => {
                            vault.clone()
                        }
                    }
                }
                (Some(vault_id), Some(tail_id)) => {
                    match self.db.get(vault_id.as_str()) {
                        None => {
                            let mut commit_log = vec![];
                            commit_log.extend(self.server_events.clone());
                            commit_log
                        }
                        Some(vault) => {
                            todo!("find all records older than a tail_id");
                        }
                    }
                }
                (_, _) => {
                    let mut commit_log = vec![];
                    commit_log.extend(self.server_events.clone());
                    commit_log
                }
            }
        }

        /// Handle request: all types of requests will be handled and the actions will be executed accordingly
        fn request(&mut self, event: &KvLogEvent) {
            match event.cmd_type {
                AppOperationType::Request(op) => {
                    match op {
                        AppOperation::Genesis => {
                            panic!("Not allowed");
                        }
                        AppOperation::SignUp => {
                            // Handled by the server. Add a vault to the system
                            let sign_up_events = commit_log::accept_event_sign_up_request(event);
                            let vault_id = event.key.vault_id.clone().unwrap();
                            match self.db.get(vault_id.as_str()) {
                                None => {
                                    let mut commit_log = self.server_events.clone();
                                    commit_log.extend(sign_up_events);
                                    self.db.insert(vault_id, commit_log);
                                }
                                Some(_) => {
                                    panic!("Vault already exists");
                                }
                            }
                        }
                        AppOperation::JoinCluster => {
                            // Just save a request to handle by a vault owner
                            match event.key.vault_id.clone() {
                                None => {
                                    panic!("Invalid event");
                                }
                                Some(vault_id) => {
                                    let maybe_vault_commit_log = self.db.get(vault_id.as_str());
                                    match maybe_vault_commit_log {
                                        None => {
                                            panic!("Vault not found");
                                        }
                                        Some(commit_log) => {

                                            let mut new_commit_log = commit_log.clone();
                                            new_commit_log.push(event.clone());

                                            let vault_meta_db = transform(Rc::new(new_commit_log.clone())).unwrap();
                                            let vault = &vault_meta_db.meta_store.vault.unwrap();
                                            let accept_join_event = commit_log::accept_join_request(event, vault);

                                            new_commit_log.push(accept_join_event);
                                            self.db.insert(vault_id, new_commit_log);
                                        }
                                    }
                                }
                            }
                        }
                        AppOperation::VaultsIndex => {
                            panic!("Not allowed");
                        }
                    }
                }
                //Check validity and just save to the database
                AppOperationType::Update(_) => {
                    match event.key.vault_id.clone() {
                        None => {
                            panic!("Invalid event");
                        }
                        Some(vault_id) => {
                            let maybe_db_vault = self.db.get(vault_id.as_str());
                            match maybe_db_vault {
                                None => {
                                    panic!("Vault not found");
                                }
                                Some(commit_log) => {
                                    let mut new_commit_log = commit_log.clone();
                                    new_commit_log.push(event.clone());
                                    self.db.insert(vault_id, new_commit_log);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    struct SyncRequest {
        vault_id: Option<String>,
        tail_id: Option<String>,
    }
}