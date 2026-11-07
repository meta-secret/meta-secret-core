use super::{
    correlation_automatic::interrupted,
    correlation_faults::{Fault, Step},
    correlation_support::finish,
    *,
};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

async fn sqlite(count: usize, step: Step) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("producer-correlation.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let state = interrupted(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        count,
        step,
        Fault::Before,
    )
    .await?;
    finish(Arc::new(SqlIteRepo { conn_url: url }), state).await
}
async fn redb(count: usize, step: Step) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("producer-correlation.redb");
    let state = interrupted(Arc::new(ReDbRepo::new(&path)?), count, step, Fault::Before).await?;
    finish(Arc::new(ReDbRepo::open(&path)?), state).await
}
#[tokio::test]
async fn correlation_auto_sqlite_purge_pending_real_close_reopen() -> Result<()> {
    sqlite(1, Step::Purge).await
}
#[tokio::test]
async fn correlation_auto_redb_purge_pending_real_close_reopen() -> Result<()> {
    redb(2, Step::Purge).await
}
#[tokio::test]
async fn correlation_auto_sqlite_complete_before_raw_real_close_reopen() -> Result<()> {
    sqlite(2, Step::Raw).await
}
#[tokio::test]
async fn correlation_auto_redb_complete_before_raw_real_close_reopen() -> Result<()> {
    redb(1, Step::Raw).await
}
