use diesel::{Connection, SqliteConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use meta_secret_core::crypto::{encoding::base64::Base64Text, utils};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../meta-server-emulator/migrations");

pub struct EmbeddedMigrationsTool {
    pub db_url: String,
}

impl EmbeddedMigrationsTool {
    pub fn migrate(&self) {
        let conn = &mut SqliteConnection::establish(self.db_url.as_str()).unwrap();
        conn.revert_all_migrations(MIGRATIONS).unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
    }
}

impl Default for EmbeddedMigrationsTool {
    fn default() -> Self {
        let Base64Text(db_name) = utils::rand_uuid_b64_url_enc();

        Self {
            db_url: format!("file:///tmp/{}.db", db_name),
        }
    }
}
