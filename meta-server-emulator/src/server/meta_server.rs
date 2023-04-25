use meta_secret_core::node::db::commit_log::KvLogEvent;

pub struct SyncRequest {
    pub vault_id: Option<String>,
    pub tail_id: Option<String>,
}

pub trait MetaServerEmulator {
    fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent>;
    fn send(&mut self, event: &KvLogEvent);
}

pub mod sqlite_meta_server {
    use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl};
    use diesel::sqlite::SqliteConnection;

    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::commit_log::{KvLogEvent, store_names};
    use crate::models::{DbLogEvent, NewDbLogEvent};
    use crate::schema::db_commit_log as schema_log;
    use crate::schema::db_commit_log::dsl;

    use crate::server::meta_server::{MetaServerEmulator, SyncRequest};

    pub struct SqliteMockServer {
        km: KeyManager,
        conn: SqliteConnection,
    }

    impl SqliteMockServer {
        /// conn_url="file:///tmp/test.db"
        pub fn new(conn_url: &str) -> Self {
            let km = KeyManager::generate();
            let mut conn = SqliteConnection::establish(conn_url).unwrap();

            let genesis_event: KvLogEvent = commit_log::generate_genesis_event(&km.dsa.public_key());
            let db_genesis_event = NewDbLogEvent::from(&genesis_event);

            diesel::insert_into(schema_log::table)
                .values(&db_genesis_event)
                .execute(&mut conn)
                .expect("Error saving genesis event");

            Self {
                km,
                conn,
            }
        }
    }

    impl SqliteMockServer {
        fn find_genesis_event(&mut self) -> KvLogEvent {
            let db_genesis_evt = dsl::db_commit_log
                .filter(dsl::store.eq(store_names::GENESIS))
                .first::<DbLogEvent>(&mut self.conn)
                .expect("Genesis event must exists");
            KvLogEvent::from(&db_genesis_evt)
        }

        fn get_vaults_index(&mut self) -> Vec<KvLogEvent> {
            let db_vaults_idx = dsl::db_commit_log
                .filter(dsl::store.eq(store_names::VAULTS_IDX))
                .load::<DbLogEvent>(&mut self.conn)
                .expect("Vaults index not exists");

            let vaults_idx: Vec<KvLogEvent> = db_vaults_idx
                .into_iter()
                .map(|db_evt| KvLogEvent::from(&db_evt))
                .collect();
            vaults_idx
        }

        fn get_empty_vault_commit_log(&mut self) -> Vec<KvLogEvent> {
            let genesis_event = self.find_genesis_event();
            let vaults_idx = self.get_vaults_index();

            let mut commit_log = vec![];
            commit_log.push(genesis_event);
            commit_log.extend(vaults_idx);

            commit_log
        }
    }

    impl MetaServerEmulator for SqliteMockServer {
        fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent> {
            let vault_and_tail = (request.vault_id, request.tail_id);

            match vault_and_tail {
                (Some(request_vault_id), None) => {
                    let vault_events: Vec<DbLogEvent> = dsl::db_commit_log
                        .filter(dsl::vault_id.eq(request_vault_id))
                        .load::<DbLogEvent>(&mut self.conn)
                        .expect("Error loading vault events");

                    if vault_events.is_empty() {
                        self.get_empty_vault_commit_log()
                    } else {
                        let vaults_idx: Vec<KvLogEvent> = vault_events
                            .into_iter()
                            .map(|db_evt| KvLogEvent::from(&db_evt))
                            .collect();
                        vaults_idx
                    }
                }
                _ => {
                    self.get_empty_vault_commit_log()
                }
            }
        }

        fn send(&mut self, event: &KvLogEvent) {
            todo!()
        }
    }
}

pub mod in_mem_meta_server {
    use std::collections::HashMap;
    use std::rc::Rc;

    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::commit_log::{AppOperation, AppOperationType, KvLogEvent};

    use crate::server::meta_server::{MetaServerEmulator, SyncRequest};

    pub struct InMemMockServer {
        km: KeyManager,
        // vault name -> commit log
        db: HashMap<String, Vec<KvLogEvent>>,
        server_events: Vec<KvLogEvent>,
    }

    impl InMemMockServer {
        pub fn new() -> Self {
            let km = KeyManager::generate();
            let genesis_event: KvLogEvent = commit_log::generate_genesis_event(&km.dsa.public_key());

            let server_events = vec![genesis_event];
            let db = HashMap::new();

            Self { km, db, server_events }
        }
    }

    impl MetaServerEmulator for InMemMockServer {
        fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent> {
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
        fn send(&mut self, event: &KvLogEvent) {
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

                                            let vault_meta_db = commit_log::transform(Rc::new(new_commit_log.clone())).unwrap();
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
}
