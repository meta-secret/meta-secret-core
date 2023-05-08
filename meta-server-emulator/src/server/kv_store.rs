use meta_secret_core::node::db::models::{KvKey, KvLogEvent};

mod commit_log_repo {
    use async_trait::async_trait;
    use meta_secret_core::node::db::db::{FindOneQuery, FindQuery, SaveCommand};
    use meta_secret_core::node::db::meta_db::CommitLogRepo;
    use meta_secret_core::node::db::models::KvLogEvent;

    #[derive(thiserror::Error, Debug)]
    enum RocksDbError {}

    struct CommitLogRepoRocksDb {}

    #[async_trait(? Send)]
    impl FindQuery<KvLogEvent> for CommitLogRepoRocksDb {
        type Error = RocksDbError;

        async fn find(&self, key: &str) -> Result<Vec<KvLogEvent>, Self::Error> {
            todo!()
        }
    }

    #[async_trait(? Send)]
    impl FindOneQuery<KvLogEvent> for CommitLogRepoRocksDb {
        type Error = RocksDbError;

        async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
            todo!()
        }
    }

    impl CommitLogRepo for CommitLogRepoRocksDb {}

    #[async_trait(? Send)]
    impl SaveCommand<KvLogEvent> for CommitLogRepoRocksDb {
        type Error = RocksDbError;

        async fn save(&self, key: &str, value: &KvLogEvent) -> Result<(), Self::Error> {
            todo!("???")
        }
    }
}
