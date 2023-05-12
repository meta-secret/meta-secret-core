use meta_secret_core::node::db::models::KvLogEvent;

pub struct SyncRequest {
    pub vault: Option<VaultSyncRequest>,
    pub global_index: Option<String>,
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
    use diesel::sqlite::SqliteConnection;
    use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl};

    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::Base64EncodedText;
    use meta_secret_core::node::db::events::global_index;
    use meta_secret_core::node::db::events::global_index::{
        generate_global_index_formation_event, new_global_index_record_created_event,
    };
    use meta_secret_core::node::db::events::sign_up::accept_event_sign_up_request;
    use meta_secret_core::node::db::models::{
        AppOperation, AppOperationType, KeyIdGen, KvKeyId, KvLogEvent, ObjectType,
    };

    use crate::models::{DbLogEvent, NewDbLogEvent};
    use crate::schema::db_commit_log as schema_log;
    use crate::schema::db_commit_log::dsl;
    use crate::server::meta_server::{MetaServerEmulator, SyncRequest};

    pub struct SqliteMockServer {
        km: KeyManager,
        conn: SqliteConnection,
        global_index_tail_id: Option<KvKeyId>,
    }

    impl SqliteMockServer {
        /// conn_url="file:///tmp/test.db"
        pub fn new(conn_url: &str) -> Self {
            let km = KeyManager::generate();
            let conn = SqliteConnection::establish(conn_url).unwrap();
            let global_index_tail_id = None;
            Self {
                km,
                conn,
                global_index_tail_id,
            }
        }

        pub fn server_pk(&self) -> Base64EncodedText {
            self.km.dsa.public_key()
        }
    }

    impl SqliteMockServer {
        fn get_next_free_global_index_id(&mut self) -> KvKeyId {
            let formation_id = global_index::generate_global_index_formation_key_id();

            let mut existing_id = formation_id.clone();
            let mut curr_tail_id = formation_id;
            loop {
                let global_idx_result = dsl::db_commit_log
                    .filter(dsl::key_id.eq(curr_tail_id.key_id.as_str()))
                    .first::<DbLogEvent>(&mut self.conn);

                match global_idx_result {
                    Ok(_) => {
                        existing_id = curr_tail_id.clone();
                        curr_tail_id = curr_tail_id.next();
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            existing_id
        }

        fn get_global_index_from_beginning(&mut self) -> Vec<KvLogEvent> {
            let formation_id = global_index::generate_global_index_formation_key_id();
            self.get_global_index(formation_id.key_id.as_str())
        }

        fn get_global_index(&mut self, tail_id: &str) -> Vec<KvLogEvent> {
            let mut curr_tail_id = tail_id.to_string();
            let mut global_idx = vec![];
            loop {
                let db_vaults_idx_result = dsl::db_commit_log
                    .filter(dsl::key_id.eq(curr_tail_id.clone()))
                    .first::<DbLogEvent>(&mut self.conn);

                match db_vaults_idx_result {
                    Ok(db_vaults_idx) => {
                        let ids_record = KvLogEvent::from(&db_vaults_idx);
                        global_idx.push(ids_record);
                        curr_tail_id = KvKeyId::generate_next(curr_tail_id.as_str()).key_id.clone();
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            //check if genesis event exists for vaults index
            if global_idx.is_empty() {
                //create a genesis event and save into the database
                let formation_event =
                    generate_global_index_formation_event(&self.km.dsa.public_key());
                diesel::insert_into(schema_log::table)
                    .values(&NewDbLogEvent::from(&formation_event))
                    .execute(&mut self.conn)
                    .expect("Error saving vaults genesis event");

                global_idx.push(formation_event);
            }

            global_idx
        }
    }

    impl MetaServerEmulator for SqliteMockServer {
        fn sync(&mut self, request: SyncRequest) -> Vec<KvLogEvent> {
            let mut commit_log = vec![];

            match request.global_index {
                None => {
                    let meta_g = self.get_global_index_from_beginning();
                    commit_log.extend(meta_g);
                }
                Some(index_id) => {
                    let meta_g = self.get_global_index(index_id.as_str());
                    commit_log.extend(meta_g);
                }
            }

            match request.vault {
                None => {
                    // Ignore empty requests
                }
                Some(vault_request) => {
                    let vault_and_tail = (vault_request.vault_id, vault_request.tail_id);

                    match vault_and_tail {
                        (Some(request_vault_id), None) => {
                            let vault_events: Vec<DbLogEvent> = dsl::db_commit_log
                                .filter(dsl::vault_id.eq(request_vault_id.clone()))
                                .load::<DbLogEvent>(&mut self.conn)
                                .expect("Error loading vault events");

                            let global_idx: Vec<KvLogEvent> = vault_events
                                .into_iter()
                                .map(|db_evt| KvLogEvent::from(&db_evt))
                                .collect();

                            commit_log.extend(global_idx);
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
                        AppOperation::ObjectFormation => {
                            panic!("Not allowed");
                        }

                        AppOperation::GlobalIndex => {
                            panic!("Not allowed");
                        }

                        AppOperation::SignUp => {
                            // Handled by the server. Add a vault to the system
                            let vault_id = event.key.vault_id.clone().unwrap();
                            let vault_formation_id =
                                KvKeyId::object_foundation(vault_id.as_str(), ObjectType::Vault);

                            let vault_formation_event_result = dsl::db_commit_log
                                .filter(dsl::key_id.eq(vault_formation_id.key_id))
                                .first::<DbLogEvent>(&mut self.conn);

                            match vault_formation_event_result {
                                Err(_) => {
                                    //vault not found, we can create our new vault
                                    let sign_up_events = accept_event_sign_up_request(
                                        event.clone(),
                                        self.server_pk(),
                                    );

                                    //find the latest global_index_id???
                                    let global_index_tail_id =
                                        match self.global_index_tail_id.clone() {
                                            None => {
                                                let tail_id = self.get_next_free_global_index_id();
                                                self.global_index_tail_id = Some(tail_id.clone());
                                                tail_id
                                            }
                                            Some(tail_id) => {
                                                //we already have latest global index id
                                                tail_id
                                            }
                                        };

                                    let sign_up_db_events: Vec<NewDbLogEvent> = sign_up_events
                                        .into_iter()
                                        .map(|log_event| NewDbLogEvent::from(&log_event))
                                        .collect();

                                    diesel::insert_into(schema_log::table)
                                        .values(&sign_up_db_events)
                                        .execute(&mut self.conn)
                                        .expect("Error saving genesis event");

                                    //update global index
                                    let global_index_event = new_global_index_record_created_event(
                                        &global_index_tail_id,
                                        vault_id.as_str(),
                                    );

                                    diesel::insert_into(schema_log::table)
                                        .values(&NewDbLogEvent::from(&global_index_event))
                                        .execute(&mut self.conn)
                                        .expect("Error saving vaults genesis event");
                                }
                                Ok(sign_up_event) => {
                                    panic!("Vault already exists");
                                    //save a reject response in the database
                                }
                            }
                        }

                        AppOperation::JoinCluster => match event.key.vault_id.clone() {
                            None => {
                                panic!("Invalid JoinCluster request: vault is not set");
                            }
                            Some(_vault_id) => {
                                let join_db_event = NewDbLogEvent::from(event);

                                diesel::insert_into(schema_log::table)
                                    .values(&join_db_event)
                                    .execute(&mut self.conn)
                                    .expect("Error saving genesis event");
                            }
                        },
                    }
                }

                //Check validity and just save to the database
                AppOperationType::Update(op) => match op {
                    AppOperation::JoinCluster => {}
                    AppOperation::GlobalIndex => {}

                    AppOperation::ObjectFormation => {
                        //Skip an update operation (an update generated by server)
                    }
                    AppOperation::SignUp => {
                        //Skip an update operation (an update generated by server)
                    }
                },
            }
        }
    }
}
