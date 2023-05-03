#[cfg(test)]
mod test {
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use meta_secret_core::node::db::commit_log::{AppOperation, AppOperationType, KvLogEvent};
    use meta_server_emulator::server::meta_server::sqlite_meta_server::SqliteMockServer;
    use meta_server_emulator::server::meta_server::{MetaServerEmulator, SyncRequest};

    pub const MIGRATIONS: EmbeddedMigrations =
        embed_migrations!("../meta-server-emulator/migrations");

    #[test]
    fn test_brand_new_client() {
        let db_url = "file:///tmp/test.db";

        let conn = &mut SqliteConnection::establish("file:///tmp/test.db").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();

        let mut server = SqliteMockServer::new(db_url);
        let request = SyncRequest {
            vault_id: None,
            tail_id: None,
        };
        let commit_log = server.sync(request);

        assert_eq!(1, commit_log.len());
        assert_eq!(
            AppOperationType::Update(AppOperation::Genesis),
            commit_log.first().unwrap().cmd_type
        );
    }
}
