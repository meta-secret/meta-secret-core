use meta_secret_core::node::db::models::KvLogEvent;

pub struct SyncRequest {
    pub vault: Option<VaultSyncRequest>,
    pub vaults_index: Option<String>
}

pub struct VaultSyncRequest {
    pub vault_id: Option<String>,
    pub tail_id: Option<String>,
}

pub trait MetaServerEmulator {
    fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent>;
    fn send(&mut self, event: &KvLogEvent);
}

pub mod sqlite_meta_server {
    use diesel::{Connection, ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};
    use diesel::sqlite::SqliteConnection;

    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::Base64EncodedText;
    use meta_secret_core::node::db::commit_log::store_names;
    use meta_secret_core::node::db::events::{genesis, index};
    use meta_secret_core::node::db::events::sign_up::accept_event_sign_up_request;
    use meta_secret_core::node::db::models::{AppOperation, AppOperationType, KeyIdGen, KvKeyId, KvLogEvent};

    use crate::models::{DbLogEvent, NewDbLogEvent};
    use crate::schema::db_commit_log as schema_log;
    use crate::schema::db_commit_log::dsl;
    use crate::server::meta_server::{MetaServerEmulator, SyncRequest, VaultSyncRequest};

    pub struct SqliteMockServer {
        km: KeyManager,
        conn: SqliteConnection,
    }

    impl SqliteMockServer {
        /// conn_url="file:///tmp/test.db"
        pub fn new(conn_url: &str) -> Self {
            let km = KeyManager::generate();
            let conn = SqliteConnection::establish(conn_url).unwrap();
            Self { km, conn }
        }

        pub fn server_pk(&self) -> Base64EncodedText {
            self.km.dsa.public_key()
        }
    }

    impl SqliteMockServer {
        /*
        fn find_genesis_event(&mut self) -> KvLogEvent {
            let db_genesis_evt = dsl::db_commit_log
                .filter(dsl::store.eq(store_names::GENESIS))
                .first::<DbLogEvent>(&mut self.conn)
                .expect("Genesis event must exists");
            KvLogEvent::from(&db_genesis_evt)
        }
         */

        fn get_vaults_index_from_beginning(&mut self) -> Vec<KvLogEvent> {
            let genesis_id = index::generate_vaults_genesis_key_id();
            self.get_vaults_index(genesis_id.key_id.as_str())
        }

        fn get_vaults_index(&mut self, tail_id: &str) -> Vec<KvLogEvent> {

            let mut curr_tail_id = tail_id.to_string();
            let mut vaults_idx = vec![];
            loop {
                let db_vaults_idx_result = dsl::db_commit_log
                    .filter(dsl::key_id.eq(curr_tail_id.clone()))
                    .first::<DbLogEvent>(&mut self.conn);

                match db_vaults_idx_result {
                    Ok(db_vaults_idx) => {
                        let ids_record = KvLogEvent::from(&db_vaults_idx);
                        vaults_idx.push(ids_record);
                        curr_tail_id = KvKeyId::generate_next(curr_tail_id.as_str()).key_id.clone();
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            //check if genesis event exists for vaults index
            if vaults_idx.is_empty() {
                //create a genesis event and save into the database
                let vaults_genesis_event = index::generate_vaults_genesis_event(&self.km.dsa.public_key());
                diesel::insert_into(schema_log::table)
                    .values(&NewDbLogEvent::from(&vaults_genesis_event))
                    .execute(&mut self.conn)
                    .expect("Error saving vaults genesis event");

                vaults_idx.push(vaults_genesis_event);
            }

            vaults_idx
        }

        fn get_empty_vault_commit_log(&mut self, vault_id: &str) -> Vec<KvLogEvent> {
            let genesis_key = genesis::generate_genesis_key_from_vault_id(vault_id);
            let genesis_event = genesis::generate_genesis_event_with_key(
                &genesis_key, &self.km.dsa.public_key(),
            );
            let db_genesis_event = NewDbLogEvent::from(&genesis_event);

            diesel::insert_into(schema_log::table)
                .values(&db_genesis_event)
                .execute(&mut self.conn)
                .expect("Error saving genesis event");

            let vaults_idx = self.get_vaults_index_from_beginning();

            let mut commit_log = vec![];
            commit_log.push(genesis_event);
            commit_log.extend(vaults_idx);

            commit_log
        }
    }

    impl MetaServerEmulator for SqliteMockServer {
        fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent> {
            let mut commit_log = vec![];

            match request.vaults_index {
                None => {
                    commit_log.extend(self.get_vaults_index_from_beginning());
                    return commit_log;
                }
                Some(index_id) => {
                    commit_log.extend(self.get_vaults_index(index_id.as_str()));
                    return commit_log;
                }
            }

            match request.vault {
                None => {

                }
                Some(vault_request) => {
                    let vault_and_tail = (vault_request.vault_id, vault_request.tail_id);

                    match vault_and_tail {
                        (Some(request_vault_id), None) => {
                            let vault_events: Vec<DbLogEvent> = dsl::db_commit_log
                                .filter(dsl::vault_id.eq(request_vault_id.clone()))
                                .load::<DbLogEvent>(&mut self.conn)
                                .expect("Error loading vault events");

                            if vault_events.is_empty() {
                                let empty_commit_log = self.get_empty_vault_commit_log(request_vault_id.as_str());
                                commit_log.extend(empty_commit_log)
                            } else {
                                let vaults_idx: Vec<KvLogEvent> = vault_events
                                    .into_iter()
                                    .map(|db_evt| KvLogEvent::from(&db_evt))
                                    .collect();
                                commit_log.extend(vaults_idx);
                            }
                        }
                        _ => {
                            println!("no need to do any actions");
                        }
                    }
                }
            }

            commit_log
        }

        /// Handle request: all types of requests will be handled and the actions will be executed accordingly
        fn send(&mut self, event: &KvLogEvent) {
            match event.cmd_type {
                AppOperationType::Request(op) => {
                    match op {
                        AppOperation::Genesis => {
                            panic!("Not allowed");
                        }

                        AppOperation::VaultsIndex => {
                            panic!("Not allowed");
                        }

                        AppOperation::SignUp => {
                            // Handled by the server. Add a vault to the system
                            let sign_up_events = accept_event_sign_up_request(event);
                            let vault_id = event.key.vault_id.clone().unwrap();

                            let sign_up_key_id = KvKeyId::object_foundation(vault_id.as_str(), store_names::USER_VAULT);

                            let sign_up_event_result = dsl::db_commit_log
                                .filter(dsl::key_id.eq(sign_up_key_id.key_id))
                                .first::<DbLogEvent>(&mut self.conn);

                            match sign_up_event_result {
                                Err(_) => {
                                    //vault not found, we can create our new vault
                                    //let mut commit_log = self.get_empty_vault_commit_log(vault_id.as_str());
                                    //commit_log.extend(sign_up_events);

                                    let sign_up_db_events: Vec<NewDbLogEvent> = sign_up_events
                                        .into_iter()
                                        .map(|log_event| NewDbLogEvent::from(&log_event))
                                        .collect();

                                    diesel::insert_into(schema_log::table)
                                        .values(&sign_up_db_events)
                                        .execute(&mut self.conn)
                                        .expect("Error saving genesis event");
                                }
                                Ok(sign_up_event) => {
                                    panic!("Vault already exists");
                                    //send reject response
                                }
                            }
                        }

                        AppOperation::JoinCluster => {
                            // Just save a request to handle by a vault owner
                            todo!("not implemented yet");
                            /*match event.key.vault_id.clone() {
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

                                            let rc_log = Rc::new(new_commit_log.clone());
                                            let vault_meta_db = commit_log::transform(rc_log).unwrap();
                                            let vault = &vault_meta_db.meta_store.vault.unwrap();
                                            let accept_join_event = accept_join_request(event, vault);

                                            new_commit_log.push(accept_join_event);
                                            self.db.insert(vault_id, new_commit_log);
                                        }
                                    }
                                }
                            }*/
                        }
                    }
                }
                //Check validity and just save to the database
                AppOperationType::Update(_) => match event.key.vault_id.clone() {
                    None => {
                        panic!("Invalid event");
                    }
                    Some(vault_id) => {
                        todo!("not implemented yet")
                        /*
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
                        }*/
                    }
                },
            }
        }
    }
}
