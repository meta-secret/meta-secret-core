use super::{
    recovery_inflight_durability::{interrupt, reopen},
    recovery_intake_faults::{Fault, Phase},
    *,
};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

#[tokio::test]
async fn recovery_inflight_sqlite_real_close_reopen_finishes_non_current_receiver_source()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("inflight.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let state = interrupt(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        Phase::Raw,
        Fault::Before,
    )
    .await?;
    reopen(Arc::new(SqlIteRepo { conn_url: url }), state).await
}
#[tokio::test]
async fn recovery_inflight_redb_real_close_reopen_finishes_non_current_receiver_source()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("inflight.redb");
    let state = interrupt(Arc::new(ReDbRepo::new(&path)?), Phase::Raw, Fault::Before).await?;
    reopen(Arc::new(ReDbRepo::open(&path)?), state).await
}
