//! Recovery-only private table, unreachable through generic event queries.
use super::ReDbRepo;
use anyhow::Result;
use async_trait::async_trait;
use meta_secret_core::node::{
    common::model::vault::vault::VaultName,
    db::repo::{recovery_response_store::*, recovery_response_validation::private_store_error},
};
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
pub(super) const PRIVATE_TABLE: TableDefinition<String, Vec<u8>> =
    TableDefinition::new("server-recovery-response-private-v1");

#[async_trait(?Send)]
impl ServerRecoveryResponseStore for ReDbRepo {
    async fn ensure_ready(&self) -> Result<()> {
        (|| -> Result<()> {
            let txn = self.db.begin_write()?;
            txn.open_table(PRIVATE_TABLE)?;
            txn.commit()?;
            Ok(())
        })()
        .map_err(private_store_error)
    }
    async fn load(&self, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>> {
        (|| -> Result<_> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(PRIVATE_TABLE)?;
            table
                .get(vault.0.clone())?
                .map(|bytes| PrivateRecoveryResponse::decode_private(vault, &bytes.value()))
                .transpose()
        })()
        .map_err(private_store_error)
    }
    async fn reserve(&self, vault: &VaultName, row: &PrivateRecoveryResponse) -> Result<()> {
        (|| -> Result<()> {
            let txn = self.db.begin_write()?;
            {
                let mut table = txn.open_table(PRIVATE_TABLE)?;
                let existing = table
                    .get(vault.0.clone())?
                    .map(|bytes| PrivateRecoveryResponse::decode_private(vault, &bytes.value()))
                    .transpose()?;
                row.compare_reservation(vault, existing.as_ref())?;
                if existing.is_none() {
                    table.insert(vault.0.clone(), row.encode_private(vault)?)?;
                }
            }
            txn.commit()?;
            Ok(())
        })()
        .map_err(private_store_error)
    }
    async fn mark_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        (|| -> Result<()> {
            let txn = self.db.begin_write()?;
            {
                let mut table = txn.open_table(PRIVATE_TABLE)?;
                let row = table
                    .get(vault.0.clone())?
                    .ok_or(RecoveryResponseStoreError::Conflict)?
                    .value();
                let committed = PrivateRecoveryResponse::decode_private(vault, &row)?
                    .to_committed(vault, source)?;
                table.insert(vault.0.clone(), committed.encode_private(vault)?)?;
            }
            txn.commit()?;
            Ok(())
        })()
        .map_err(private_store_error)
    }
    async fn try_remove_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        (|| -> Result<()> {
            let txn = self.db.begin_write()?;
            {
                let mut table = txn.open_table(PRIVATE_TABLE)?;
                let row = table
                    .get(vault.0.clone())?
                    .map(|bytes| PrivateRecoveryResponse::decode_private(vault, &bytes.value()))
                    .transpose()?;
                if let Some(row) = row {
                    row.ensure_removable(vault, source)?;
                    table.remove(vault.0.clone())?;
                }
            }
            txn.commit()?;
            Ok(())
        })()
        .map_err(private_store_error)
    }
}
