use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

/// CODEC ONLY: the exact produced compact and index are persisted, not a second business lifecycle.
async fn store_close<R: KvLogEventRepo>(
    repo: Arc<R>,
    record: &LocalGuardedPreparationObject,
    index: &LocalGuardedPreparationIndexObject,
) -> Result<()> {
    let p = Arc::new(PersistentObject::new(repo.clone()));
    p.repo.save(record.clone()).await?;
    p.repo.save(index.clone()).await?;
    ensure!(
        p.repo.find_one(record.obj_id()).await? == Some(record.clone().to_generic())
            && p.repo.find_one(index.obj_id()).await? == Some(index.clone().to_generic()),
        "actual codec writes/readbacks differ"
    );
    drop(p);
    drop(repo);
    Ok(())
}
async fn reopened<R: KvLogEventRepo>(
    repo: Arc<R>,
    record: &LocalGuardedPreparationObject,
    index: &LocalGuardedPreparationIndexObject,
) -> Result<()> {
    ensure!(
        repo.find_one(record.obj_id()).await? == Some(record.clone().to_generic())
            && repo.find_one(index.obj_id()).await? == Some(index.clone().to_generic())
            && repo.find_one(record.obj_id().next()).await?.is_none()
            && repo.find_one(index.obj_id().next()).await?.is_none(),
        "actual SAMEfile nested compact/index codec or no-next-key differs"
    );
    Ok(())
}
pub(super) async fn table(
    record: &LocalGuardedPreparationObject,
    index: &LocalGuardedPreparationIndexObject,
) -> Result<()> {
    let encoded = serde_json::to_vec(&record.clone().to_generic())?;
    ensure!(
        serde_json::from_slice::<GenericKvLogEvent>(&encoded)? == record.clone().to_generic(),
        "actual nested typed serde differs"
    );
    let dir = tempfile::tempdir()?;
    let sqlite = async {
        let url = format!("file:{}", dir.path().join("actual-nested.sqlite").display());
        EmbeddedMigrationsTool { db_url: url.clone() }.migrate();
        let repo = Arc::new(SqlIteRepo { conn_url: url.clone() });
        let weak = Arc::downgrade(&repo);
        store_close(repo, record, index).await?;
        ensure!(weak.upgrade().is_none(), "actual SQLite owners still live");
        reopened(Arc::new(SqlIteRepo { conn_url: url }), record, index).await?;
        eprintln!("OWNED_LIFETIME CODEC SQLite actual_Weak_None=true SAMEfile_nested_readback=true noNextKey=true CODEC_ONLY=true");
        Ok::<(), anyhow::Error>(())
    }.await;
    let redb = async {
        let path = dir.path().join("actual-nested.redb");
        let repo = Arc::new(ReDbRepo::new(&path)?);
        let weak = Arc::downgrade(&repo);
        store_close(repo, record, index).await?;
        ensure!(weak.upgrade().is_none(), "actual ReDB owners still live");
        reopened(Arc::new(ReDbRepo::open(&path)?), record, index).await?;
        eprintln!("OWNED_LIFETIME CODEC ReDB actual_Weak_None=true SAMEfile_nested_readback=true noNextKey=true CODEC_ONLY=true");
        Ok::<(), anyhow::Error>(())
    }.await;
    ensure!(
        sqlite.is_ok() && redb.is_ok(),
        "nested CODEC table SQLite_ok={} ReDB_ok={}",
        sqlite.is_ok(),
        redb.is_ok()
    );
    Ok(())
}
