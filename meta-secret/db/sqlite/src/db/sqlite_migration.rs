use diesel::{Connection, RunQueryDsl, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use meta_secret_core::crypto::utils::UuidUrlEnc;
use meta_secret_core::node::common::model::IdString;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub struct EmbeddedMigrationsTool {
    pub db_url: String,
}

impl EmbeddedMigrationsTool {
    pub fn migrate(&self) {
        let conn = &mut SqliteConnection::establish(self.db_url.as_str()).unwrap();
        diesel::sql_query("PRAGMA busy_timeout = 5000").execute(conn).unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
    }
}

impl Default for EmbeddedMigrationsTool {
    fn default() -> Self {
        let db_name = UuidUrlEnc::generate();

        Self {
            db_url: format!("file:///tmp/{}.db", db_name.id_str()),
        }
    }
}
