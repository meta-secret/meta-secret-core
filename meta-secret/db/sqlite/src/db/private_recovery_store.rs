//! Single private Recovery row per Vault; never DbLogEvent / ArtifactId storage.
use super::sqlite_store::{SqlIteRepo, establish_connection};
use anyhow::Result;
use async_trait::async_trait;
use diesel::{
    OptionalExtension, RunQueryDsl, SqliteConnection,
    sql_types::{Binary, Text},
};
use meta_secret_core::node::{
    common::model::vault::vault::VaultName,
    db::repo::{recovery_response_store::*, recovery_response_validation::private_store_error},
};
#[derive(diesel::QueryableByName)]
struct RecordBlob {
    #[diesel(sql_type = Binary)]
    record: Vec<u8>,
}
fn load(conn: &mut SqliteConnection, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>> {
    diesel::sql_query("SELECT record FROM server_recovery_response_private WHERE vault_name = ?")
        .bind::<Text, _>(&vault.0)
        .get_result::<RecordBlob>(conn)
        .optional()?
        .map(|r| PrivateRecoveryResponse::decode_private(vault, &r.record))
        .transpose()
}
#[async_trait(?Send)]
impl ServerRecoveryResponseStore for SqlIteRepo {
    async fn ensure_ready(&self) -> Result<()> {
        (|| -> Result<()> {
            let mut conn = establish_connection(&self.conn_url)?;
            diesel::sql_query("CREATE TABLE IF NOT EXISTS server_recovery_response_private (vault_name TEXT PRIMARY KEY NOT NULL, record BLOB NOT NULL)").execute(&mut conn)?;
            Ok(())
        })().map_err(private_store_error)
    }
    async fn load(&self, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>> {
        (|| -> Result<_> { load(&mut establish_connection(&self.conn_url)?, vault) })()
            .map_err(private_store_error)
    }
    async fn reserve(&self, vault: &VaultName, row: &PrivateRecoveryResponse) -> Result<()> {
        (|| -> Result<()> {
            let mut conn = establish_connection(&self.conn_url)?;
            conn.immediate_transaction::<_, anyhow::Error,_>(|conn| {
                let existing = load(conn, vault)?;
                row.compare_reservation(vault, existing.as_ref())?;
                if existing.is_none() {
                    diesel::sql_query("INSERT INTO server_recovery_response_private (vault_name, record) VALUES (?,?)")
                        .bind::<Text,_>(&vault.0).bind::<Binary,_>(row.encode_private(vault)?).execute(conn)?;
                }
                Ok(())
            })
        })().map_err(private_store_error)
    }
    async fn mark_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        (|| -> Result<()> {
            let mut conn = establish_connection(&self.conn_url)?;
            conn.immediate_transaction::<_, anyhow::Error, _>(|conn| {
                let row = load(conn, vault)?
                    .ok_or(RecoveryResponseStoreError::Conflict)?
                    .to_committed(vault, source)?;
                diesel::sql_query(
                    "UPDATE server_recovery_response_private SET record = ? WHERE vault_name = ?",
                )
                .bind::<Binary, _>(row.encode_private(vault)?)
                .bind::<Text, _>(&vault.0)
                .execute(conn)?;
                Ok(())
            })
        })()
        .map_err(private_store_error)
    }
    async fn try_remove_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        (|| -> Result<()> {
            let mut conn = establish_connection(&self.conn_url)?;
            conn.immediate_transaction::<_, anyhow::Error, _>(|conn| {
                if let Some(row) = load(conn, vault)? {
                    row.ensure_removable(vault, source)?;
                    diesel::sql_query(
                        "DELETE FROM server_recovery_response_private WHERE vault_name = ?",
                    )
                    .bind::<Text, _>(&vault.0)
                    .execute(conn)?;
                }
                Ok(())
            })
        })()
        .map_err(private_store_error)
    }
}
