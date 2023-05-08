use diesel::{Connection, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub const DEFAULT_DB_URL: &str = "file:///tmp/test.db";

pub const MIGRATIONS: EmbeddedMigrations =
    embed_migrations!("../meta-server-emulator/migrations");


pub struct EmbeddedMigrationsTool<'a> {
    pub db_url: & 'a str
}

impl EmbeddedMigrationsTool<'_> {

    pub fn migrate(&self) {
        let conn = &mut SqliteConnection::establish(self.db_url).unwrap();
        conn.revert_all_migrations(MIGRATIONS).unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
    }
}

impl Default for EmbeddedMigrationsTool<'_> {
    fn default() -> Self {
        Self {
            db_url: DEFAULT_DB_URL,
        }
    }
}