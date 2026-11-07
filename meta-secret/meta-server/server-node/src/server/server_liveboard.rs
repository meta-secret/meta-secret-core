use anyhow::{Result, bail, ensure};
use meta_secret_core::node::api::{
    ReadSyncRequest, SignedAction, SignedActionPayload, SyncRequest, WriteSyncRequest,
};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::actions::vault::removal_application::RemovalApplication;
use meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
use meta_secret_core::node::db::events::object_id::ArtifactId;
use meta_secret_core::node::db::events::vault::vault_log_event::{
    VaultActionEvent, VaultActionUpdateEvent,
};
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use std::sync::Arc;

pub async fn before_request<R: KvLogEventRepo>(
    gateway: &super::server_data_sync::ServerSyncGateway<R>,
    request: &SyncRequest,
) -> Result<()> {
    super::public_reads::validate_request(request)?;
    let audit = super::recovery_intake_audit::inspect(gateway, request).await?;
    let initial: Vec<_> = audit
        .prepared
        .iter()
        .map(|prepared| VaultName::from(prepared.record.obj_id().fqdn.obj_instance))
        .collect();
    super::producer_public::preflight(gateway, &audit.names, &initial).await?;
    for prepared in audit.prepared {
        gateway.finish_initial_recovery_intake(prepared).await?;
    }
    let application = RemovalApplication {
        p_obj: gateway.p_obj.clone(),
    };
    for name in audit.names {
        gateway.repair_recovery_response(&name).await?;
        meta_secret_core::node::db::actions::vault::automatic_delete_application::AutomaticDeleteAction {
            p_obj: gateway.p_obj.clone(),
        }.repair(name.clone()).await?;
        application.repair(name.clone()).await?;
        if matches!(request, SyncRequest::Read(_)) {
            super::producer_public::repair_raw_cleanup(gateway, &name).await?;
            application.initialize_pending(name).await?;
        }
    }
    Ok(())
}

pub(super) fn request_vault(request: &SyncRequest) -> Option<VaultName> {
    match request {
        SyncRequest::Read(read) => match read.as_ref() {
            ReadSyncRequest::Vault(r) => Some(r.sender.vault_name()),
            ReadSyncRequest::SsRequest(r) => Some(r.sender.vault_name()),
            ReadSyncRequest::ServerTail(r) => Some(r.sender.vault_name()),
            ReadSyncRequest::StateEventsSubscription(r) => Some(r.vault_name.clone()),
            ReadSyncRequest::JoinResultSubscription(r) => Some(r.vault_name.clone()),
            ReadSyncRequest::SsRecoveryCompletion(r) => {
                r.completion().ok().map(|r| r.vault_name.clone())
            }
            ReadSyncRequest::LiveboardCapabilities(_) | ReadSyncRequest::JoinAdmission(_) => None,
        },
        SyncRequest::Write(write) => match write.as_ref() {
            WriteSyncRequest::DeviceTransportNoop(authorization) => {
                Some(authorization.vault_name.clone())
            }
            WriteSyncRequest::Event(action) => match &action.payload {
                SignedActionPayload::GuardedRemovalDecision(r) => {
                    Some(r.event.0.value.vault_name())
                }
                SignedActionPayload::Event(event) => {
                    super::server_data_sync::event_vault_name(event).ok()
                }
                SignedActionPayload::RecoveryCompletion(_) => None,
            },
        },
    }
}

/// Called after the entire original envelope has passed signature/nonce/actor
/// validation. Never obtain a legacy event by discarding this signed guard.
pub async fn validate_guard<R: KvLogEventRepo>(
    p_obj: Arc<PersistentObject<R>>,
    action: &SignedAction,
) -> Result<()> {
    let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
        return Ok(());
    };
    ensure!(
        guard.version == 1 && uuid_is_valid(&guard.request_id),
        "unsupported guarded removal contract"
    );
    let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
        &guard.event.0.value
    else {
        bail!("guarded action is not a removal decision");
    };
    let p = PersistentVault::from(p_obj);
    let name = update.request.sender.user_data.vault_name();
    let vault = p.get_vault(name.clone()).await?;
    ensure!(
        vault.0.value.is_member(&action.signer),
        "guarded removal signer is not a current member"
    );
    let expected = ArtifactId::from(DeviceLogDescriptor::from(update.sender.user_data.user_id()));
    ensure!(
        guard.event.obj_id().fqdn == expected.fqdn,
        "guarded removal uses another device stream"
    );
    let log = p.get_vault_log_artifact(name).await?.0.value;
    log.validate_removal_bindings(true)?;
    let original_source = super::guarded_removal::source_proof(action)?;
    ensure!(
        log.liveboard_removal_pending
            .iter()
            .any(|b| b.request_id == guard.request_id && b.request == update.request)
            || log.liveboard_removal_results.iter().any(|result| result.request_id == guard.request_id
                && result.application_state == meta_secret_core::node::db::events::vault::liveboard_metadata::RemovalApplicationState::Applied
                && result.source.as_ref() == Some(&original_source)),
        "guarded removal reference is stale or mismatched"
    );
    Ok(())
}

fn uuid_is_valid(id: &str) -> bool {
    // Keep UUID parsing in Core so the server does not gain another dependency.
    meta_secret_core::node::api::is_liveboard_request_id(id)
}
