use meta_secret_core::node::db::commit_log::{KvLogEvent, KvKey};


pub fn put(log_event: KvLogEvent) {

}

pub fn get(key: KvKey) -> Option<KvLogEvent> {
    None
}

mod commit_log_repo {
    use async_trait::async_trait;
    use meta_secret_core::node::db::commit_log::KvLogEvent;
    use meta_secret_core::node::db::db::{GetCommand, SaveCommand};
    use meta_secret_core::node::db::meta_db::CommitLogRepo;

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
