use image::EncodableLayout;
use rocksdb::{ColumnFamilyDescriptor, DB, DBWithThreadMode, Error, MultiThreaded, Options, SingleThreaded};

use crate::models::{KvKey, KvLogEvent};
use crate::node::db::{GetCommand, SaveCommand};
use crate::node::meta_db::CommitLogRepo;

pub fn put(log_event: KvLogEvent) {
    let path = "../target/meta_db";
    let mut cf_opts = Options::default();
    cf_opts.set_max_write_buffer_number(16);
    let cf = ColumnFamilyDescriptor::new(log_event.key.store.clone(), cf_opts);

    let mut db_opts = Options::default();
    db_opts.create_missing_column_families(true);
    db_opts.create_if_missing(true);
    {
        let db: DBWithThreadMode<SingleThreaded> = DB::open_cf_descriptors(&db_opts, path, vec![cf]).unwrap();
        let key: &[u8] = log_event.key.id.as_bytes();
        let log_event_str = serde_json::to_string(&log_event).unwrap();
        db.put(key, log_event_str).expect("TODO: panic message");
        db.flush().unwrap();
    }
    //let _ = DB::destroy(&db_opts, path);
}

pub fn get(key: KvKey) -> Option<KvLogEvent> {
    let path = "../target/meta_db";
    let mut cf_opts = Options::default();
    cf_opts.set_max_write_buffer_number(16);
    let cf = ColumnFamilyDescriptor::new(key.store.clone(), cf_opts);

    let mut db_opts = Options::default();
    db_opts.create_missing_column_families(true);
    db_opts.create_if_missing(true);
    {
        let db = DB::open_cf_descriptors(&db_opts, path, vec![cf]).unwrap();
        let key: &[u8] = key.id.as_bytes();
        let log_event: KvLogEvent = match db.get(key) {
            Ok(Some(log_event_bytes)) => {
                let event: KvLogEvent = serde_json::from_slice(log_event_bytes.as_slice()).unwrap();
                event
            }
            _ => {
                panic!("yay")
            }
        };

        db.flush().unwrap();

        return Some(log_event);
    }
}

mod commit_log_repo {
    use async_trait::async_trait;
    use rocksdb::{DBWithThreadMode, SingleThreaded};

    use crate::models::KvLogEvent;
    use crate::node::db::{GetCommand, SaveCommand};
    use crate::node::meta_db::CommitLogRepo;

    #[derive(thiserror::Error, Debug)]
    enum RocksDbError {}

    struct CommitLogRepoRocksDb {}

    impl CommitLogRepo for CommitLogRepoRocksDb {}

    #[async_trait(? Send)]
    impl SaveCommand<KvLogEvent> for CommitLogRepoRocksDb {
        type Error = RocksDbError;

        async fn save(&self, key: &str, value: &KvLogEvent) -> Result<(), Self::Error> {
            todo!("???")
        }
    }

    #[async_trait(? Send)]
    impl GetCommand<KvLogEvent> for CommitLogRepoRocksDb {
        type Error = RocksDbError;

        async fn get(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
            todo!()
        }
    }
}
