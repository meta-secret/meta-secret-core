//! Read-only whole-scope audit; never repair a non-head or addressed orphan receipt.
use super::{
    recovery_intake_validation::{PreparedIntake, require, safe, unavailable, validate},
    server_data_sync::ServerSyncGateway,
};
use anyhow::Result;
use meta_secret_core::node::{
    api::{ReadSyncRequest, SyncRequest},
    common::model::vault::vault::VaultName,
    db::{
        events::{
            generic_log_event::{GenericKvLogEvent, ObjIdExtractor},
            object_id::ArtifactId,
            shared_secret_event::SsLogObject,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use std::collections::HashMap;

pub(super) struct IntakeAudit {
    pub names: Vec<VaultName>,
    pub prepared: Vec<PreparedIntake>,
}
fn remember(records: &mut HashMap<ArtifactId, SsLogObject>, log: SsLogObject) -> Result<()> {
    if let Some(previous) = records.insert(log.obj_id(), log.clone()) {
        require(previous == log)?;
    }
    Ok(())
}
fn owner(log: &SsLogObject) -> VaultName {
    VaultName::from(log.obj_id().fqdn.obj_instance)
}
async fn addressed<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    request: &SyncRequest,
) -> Result<HashMap<ArtifactId, SsLogObject>> {
    let mut records = HashMap::new();
    if let SyncRequest::Read(read) = request {
        for event in safe(super::public_reads::addressed(&gateway.p_obj, request).await)? {
            if let GenericKvLogEvent::SsLog(log) = event {
                remember(&mut records, log)?;
            }
        }
        if let ReadSyncRequest::SsRequest(r) = read.as_ref()
            && let Some(log) =
                safe(super::public_reads::ss_history(&gateway.p_obj, &r.sender.vault_name()).await)?
                    .pop()
        {
            remember(&mut records, log)?;
        }
    }
    Ok(records)
}

#[tracing::instrument(skip_all)]
pub(super) async fn inspect<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    request: &SyncRequest,
) -> Result<IntakeAudit> {
    super::public_reads::validate_request(request)?;
    let mut names =
        safe(super::recovery_affected_vaults::affected_vaults(&gateway.p_obj, request).await)?;
    let mut records = addressed(gateway, request).await?;
    names.extend(records.values().map(owner));
    names.sort_by(|a, b| a.0.cmp(&b.0));
    names.dedup();
    let mut heads = HashMap::new();
    for name in &names {
        let history = safe(super::public_reads::ss_history(&gateway.p_obj, name).await)?;
        if let Some(head) = history.last() {
            heads.insert(name.clone(), head.clone());
        }
        for record in history {
            remember(&mut records, record)?;
        }
    }
    let mut candidates: HashMap<VaultName, Vec<SsLogObject>> = HashMap::new();
    for record in records
        .into_values()
        .filter(|log| log.0.value.pending_recovery_request_intake.is_some())
    {
        candidates.entry(owner(&record)).or_default().push(record);
    }
    let mut prepared = vec![];
    let requested = super::server_liveboard::request_vault(request);
    // Validate every Vault before any writer is allowed to repair even the first one.
    for name in &names {
        if let Some(candidates) = candidates.remove(name) {
            // An addressed intermediate record cannot authorize a different
            // caller Vault's continuation. Clean legacy reads are unchanged.
            require(requested.as_ref() == Some(name))?;
            let [record] = candidates.as_slice() else {
                return Err(unavailable());
            };
            require(heads.get(name) == Some(record))?;
            prepared.push(validate(gateway, record).await?);
        }
    }
    require(candidates.is_empty())?;
    Ok(IntakeAudit { names, prepared })
}

pub(super) async fn ensure_clean<R: KvLogEventRepo>(
    gateway: &ServerSyncGateway<R>,
    request: &SyncRequest,
) -> Result<()> {
    require(inspect(gateway, request).await?.prepared.is_empty())
}
pub(super) fn guard_log(log: &SsLogObject) -> Result<()> {
    require(log.0.value.pending_recovery_request_intake.is_none())
}
pub(super) fn guard_events(events: &[GenericKvLogEvent]) -> Result<()> {
    for event in events {
        if let GenericKvLogEvent::SsLog(log) = event {
            guard_log(log)?;
        }
    }
    Ok(())
}
