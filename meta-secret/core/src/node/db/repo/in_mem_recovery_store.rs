//! Separate repo-owned private map. It is never part of get_db / generic events.
use super::recovery_response_store::*;
use crate::node::common::model::vault::vault::VaultName;
use anyhow::Result;
use async_lock::Mutex;
use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct InMemRecoveryStore {
    rows: Mutex<HashMap<VaultName, PrivateRecoveryResponse>>,
}
impl InMemRecoveryStore {
    pub(crate) async fn clear(&self) {
        self.rows.lock().await.clear();
    }
}
#[async_trait(?Send)]
impl ServerRecoveryResponseStore for InMemRecoveryStore {
    async fn ensure_ready(&self) -> Result<()> {
        Ok(())
    }
    async fn load(&self, vault: &VaultName) -> Result<Option<PrivateRecoveryResponse>> {
        let row = self.rows.lock().await.get(vault).cloned();
        if let Some(row) = &row {
            row.validate(vault)?;
        }
        Ok(row)
    }
    async fn reserve(&self, vault: &VaultName, row: &PrivateRecoveryResponse) -> Result<()> {
        let mut rows = self.rows.lock().await;
        row.compare_reservation(vault, rows.get(vault))?;
        rows.entry(vault.clone()).or_insert_with(|| row.clone());
        Ok(())
    }
    async fn mark_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        let mut rows = self.rows.lock().await;
        let row = rows
            .get(vault)
            .ok_or(RecoveryResponseStoreError::Conflict)?
            .to_committed(vault, source)?;
        rows.insert(vault.clone(), row);
        Ok(())
    }
    async fn try_remove_committed(
        &self,
        vault: &VaultName,
        source: &RecoveryResponseSourceProof,
    ) -> Result<()> {
        let mut rows = self.rows.lock().await;
        if let Some(row) = rows.get(vault) {
            row.ensure_removable(vault, source)?;
        }
        rows.remove(vault);
        Ok(())
    }
}
