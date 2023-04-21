use meta_secret_core::node::db::commit_log::KvLogEvent;

pub struct SyncRequest {
    pub vault_id: Option<String>,
    pub tail_id: Option<String>,
}

pub trait MetaServerEmulator {
    fn sync(&self, request: SyncRequest) -> Vec<KvLogEvent>;
    fn send(&mut self, event: &KvLogEvent);
}

pub mod sqlite_meta_server {
    use meta_secret_core::crypto::keys::KeyManager;
    use rusqlite::{Connection, Result};
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::commit_log::KvLogEvent;

    pub struct SqliteMockServer {
        km: KeyManager,
        //conn: Connection,
    }

    impl SqliteMockServer {
        pub fn new() -> Result<Self> {
            let km = KeyManager::generate();
            let conn = Connection::open_in_memory()?;

            let create_commit_log_table =
                "CREATE TABLE commit_log (
                    id    INTEGER PRIMARY KEY,
                    event  TEXT NOT NULL
                )";
            conn.execute(create_commit_log_table, () /*empty list of parameters.*/)?;

            let genesis_event: KvLogEvent = commit_log::generate_genesis_event(&km.dsa.public_key());
            let genesis_event_json = serde_json::to_string(&genesis_event).unwrap();

            conn.execute(
                "INSERT INTO commit_log (event) VALUES (?1)",
                [genesis_event_json.as_str()],
            )?;

            let mut stmt = conn.prepare("SELECT event FROM commit_log")?;
            let rows = stmt.query_map([], |row| row.get(0))?;

            let mut events: Vec<String> = Vec::new();

            for row in rows {
                events.push(row?);
            }

            println!("{:?}", events);

            Ok(Self { km })
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

    pub struct MockServer {
        km: KeyManager,
        // vault name -> commit log
        db: HashMap<String, Vec<KvLogEvent>>,
        server_events: Vec<KvLogEvent>,
    }

    impl MockServer {
        pub fn new() -> Self {
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
    }

    impl MetaServerEmulator for MockServer {
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

#[cfg(test)]
mod test {
    use crate::server::meta_server::sqlite_meta_server::SqliteMockServer;

    #[test]
    fn test() {
        let server = SqliteMockServer::new();
    }
}