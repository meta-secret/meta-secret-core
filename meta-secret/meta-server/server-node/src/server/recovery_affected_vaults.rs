//! Side-effect-free discovery follows the SAME public tails that reads address.
use super::server_liveboard::request_vault;
use anyhow::Result;
use meta_secret_core::node::{
    api::{ReadSyncRequest, SyncRequest},
    common::model::vault::vault::VaultName,
    db::{
        events::generic_log_event::{GenericKvLogEvent, ObjIdExtractor},
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};

pub(super) async fn affected_vaults<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    request: &SyncRequest,
) -> Result<Vec<VaultName>> {
    super::public_reads::validate_request(request)?;
    let mut names: Vec<_> = request_vault(request).into_iter().collect();
    if let SyncRequest::Read(read) = request {
        for event in super::public_reads::addressed(p, request).await? {
            owning_names(&event, &mut names);
        }
        if let ReadSyncRequest::SsRequest(r) = read.as_ref()
            && let Some(latest) = super::public_reads::ss_history(p, &r.sender.vault_name())
                .await?
                .pop()
        {
            owning_names(&GenericKvLogEvent::SsLog(latest), &mut names);
        }
    }
    names.sort_by(|a, b| a.0.cmp(&b.0));
    names.dedup();
    Ok(names)
}
fn owning_names(event: &GenericKvLogEvent, names: &mut Vec<VaultName>) {
    match event {
        GenericKvLogEvent::Vault(v) => names.push(v.0.value.vault_name.clone()),
        GenericKvLogEvent::VaultStatus(v) => names.push(v.0.value.user().vault_name()),
        GenericKvLogEvent::VaultLog(_) => {
            names.push(VaultName::from(event.obj_id().fqdn.obj_instance))
        }
        GenericKvLogEvent::SsLog(v) => {
            names.push(VaultName::from(event.obj_id().fqdn.obj_instance));
            names.extend(v.0.value.claims.values().map(|c| c.vault_name.clone()));
        }
        _ => {
            if let Ok(name) = super::server_data_sync::event_vault_name(event) {
                names.push(name);
            }
        }
    }
}
