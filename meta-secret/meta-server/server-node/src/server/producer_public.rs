//! Private producer proof stays durable; actual public clones never expose it.
use super::server_data_sync::ServerSyncGateway;
use anyhow::{Result, anyhow, ensure};
use meta_secret_core::node::{
    api::SyncRequest,
    common::model::vault::{vault::VaultName, vault_data::VaultData},
    db::{
        actions::vault::automatic_delete_application::AutomaticDeleteAction,
        events::{
            generic_log_event::GenericKvLogEvent, vault::producer_metadata::VerifiedProducerKind,
        },
        objects::{
            persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};

/// Existing tombstones plus exact admitted source keys are the durable repair
/// predicate. A raw-save lost reply needs neither another command nor a journal.
pub(super) async fn repair_raw_cleanup<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    name: &VaultName,
) -> Result<()> {
    let result: Result<()> = async {
        if let Some(vault) = cleanup_context(gateway, name).await? {
            Box::pin(
                PersistentSharedSecret::from(gateway.p_obj.clone()).purge_deleted_secrets(&vault),
            )
            .await?;
        }
        Ok(())
    }
    .await;
    result.map_err(|_| anyhow!("ProducerMetadata.Unavailable"))
}

async fn cleanup_context<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    name: &VaultName,
) -> Result<Option<VaultData>> {
    let p = PersistentVault::from(gateway.p_obj.clone());
    let Some(log) = p.vault_log(name.clone()).await? else {
        return Ok(None);
    };
    if !log.0.value.producer_source_refs.iter().any(|source| {
        matches!(
            source.kind,
            VerifiedProducerKind::RemoveRequest | VerifiedProducerKind::LocalRemoveAccept
        )
    }) {
        return Ok(None);
    }
    let vault = p.get_vault(name.clone()).await?.to_data();
    Ok((!vault.deleted_secrets.is_empty()).then_some(vault))
}

pub(super) fn public_event(event: GenericKvLogEvent) -> GenericKvLogEvent {
    match event {
        GenericKvLogEvent::VaultLog(mut log) => {
            log.0.value = log.0.value.without_private_producer_metadata();
            GenericKvLogEvent::VaultLog(log)
        }
        other => other,
    }
}

/// Inspect all affected contexts before the first maintenance effect. Competing
/// prepared owners cannot be guessed away by repairing whichever runs first.
pub(super) async fn preflight<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    names: &[VaultName],
    initial: &[VaultName],
) -> Result<()> {
    let p = PersistentVault::from(gateway.p_obj.clone());
    for name in names {
        let Some(log) = p
            .vault_log(name.clone())
            .await
            .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?
        else {
            continue;
        };
        if log.0.value.automatic_delete_application.is_some() {
            ensure!(
                !initial.contains(name) && log.0.value.liveboard_removal_applications.is_empty(),
                "ProducerMetadata.Unavailable"
            );
            let store = gateway
                .p_obj
                .repo
                .server_recovery_response_store()
                .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?;
            ensure!(
                store
                    .load(name)
                    .await
                    .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?
                    .is_none(),
                "ProducerMetadata.Unavailable"
            );
        }
    }
    Ok(())
}

pub(super) async fn ensure_readable<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    request: &SyncRequest,
) -> Result<()> {
    let names = super::recovery_affected_vaults::affected_vaults(&gateway.p_obj, request)
        .await
        .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?;
    let action = AutomaticDeleteAction {
        p_obj: gateway.p_obj.clone(),
    };
    for name in names {
        action.ensure_readable(name.clone()).await?;
        if let Some(vault) = cleanup_context(gateway, &name)
            .await
            .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?
        {
            PersistentSharedSecret::from(gateway.p_obj.clone())
                .ensure_deleted_removal_transfers_clean(&vault)
                .await
                .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))?;
        }
    }
    Ok(())
}
