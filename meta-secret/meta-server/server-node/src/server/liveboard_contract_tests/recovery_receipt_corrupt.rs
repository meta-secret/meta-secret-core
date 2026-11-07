//! Corruption uses real backend tables, not a fake successful store response.
use super::recovery_receipt_fixture::ReceiptHarness;
use super::recovery_receipt_privacy::cipher_marker;
use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

async fn malformed<R: KvLogEventRepo>(repo: Arc<R>) -> Result<(String, Vec<u8>, String)> {
    let h = ReceiptHarness::new(repo.clone()).await?;
    let packet = h.packet(&h.base.b, true)?;
    let marker = cipher_marker(&packet)?;
    let row = h.prepared(packet, true).await?;
    let store = repo.server_recovery_response_store()?;
    store.ensure_ready().await?;
    store.reserve(&row.vault_name, &row).await?;
    let mut json = serde_json::to_value(&row)?;
    // A normal serde unknown-variant error echoes this marker; the private store must drop it.
    json["phase"] = Value::String(marker.clone());
    Ok((row.vault_name.0, serde_json::to_vec(&json)?, marker))
}

async fn safe_error<R: KvLogEventRepo>(repo: &R, name: &str, marker: &str) -> Result<()> {
    let result = repo
        .server_recovery_response_store()?
        .load(&meta_secret_core::node::common::model::vault::vault::VaultName::from(name))
        .await;
    let error = result.expect_err("corrupt private row must be unavailable");
    let text = format!("{error:?}");
    assert!(
        !text.contains(marker),
        "payload-bearing decoder source leaked through the error chain"
    );
    assert!(
        text.contains("CorruptRecord"),
        "private corruption must have a static typed code"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_sqlite_corrupt_private_blob_has_static_payload_free_error() -> Result<()>
{
    use diesel::{Connection, RunQueryDsl};
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("server.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let repo = Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    });
    let (name, bytes, marker) = malformed(repo.clone()).await?;
    let mut connection = diesel::SqliteConnection::establish(&url)?;
    diesel::sql_query(
        "UPDATE server_recovery_response_private SET record = ? WHERE vault_name = ?",
    )
    .bind::<diesel::sql_types::Binary, _>(bytes)
    .bind::<diesel::sql_types::Text, _>(&name)
    .execute(&mut connection)?;
    drop(connection);
    drop(repo);
    safe_error(&SqlIteRepo { conn_url: url }, &name, &marker).await
}

#[tokio::test]
async fn recovery_receipt_redb_corrupt_private_blob_has_static_payload_free_error() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("server.redb");
    let repo = Arc::new(ReDbRepo::new(&path)?);
    let (name, bytes, marker) = malformed(repo.clone()).await?;
    let transaction = repo.db.begin_write()?;
    {
        let mut table = transaction.open_table(redb::TableDefinition::<String, Vec<u8>>::new(
            "server-recovery-response-private-v1",
        ))?;
        table.insert(name.clone(), bytes)?;
    }
    transaction.commit()?;
    drop(repo);
    safe_error(&ReDbRepo::open(&path)?, &name, &marker).await
}
