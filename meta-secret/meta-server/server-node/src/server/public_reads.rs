//! Public descriptor checks precede discovery, maintenance and generic traversal.
use anyhow::{Result, ensure};
use meta_secret_core::node::{
    api::{ReadSyncRequest, SyncRequest, WriteSyncRequest},
    common::model::vault::vault::VaultName,
    db::{
        descriptors::{
            object_descriptor::{ObjectDescriptor, ToObjectDescriptor},
            shared_secret_descriptor::SsLogDescriptor,
            vault_descriptor::{VaultDescriptor, VaultLogDescriptor, VaultStatusDescriptor},
        },
        events::{generic_log_event::*, object_id::ArtifactId, shared_secret_event::SsLogObject},
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
    device_transport_noop::{next_transport_id, validate_transport_id},
};

fn requested_streams(request: &SyncRequest) -> Vec<(ArtifactId, ObjectDescriptor)> {
    match request {
        SyncRequest::Read(read) => match read.as_ref() {
            ReadSyncRequest::Vault(r) => vec![
                (
                    r.tail.vault_status.clone(),
                    VaultStatusDescriptor::from(r.sender.user_id()).to_obj_desc(),
                ),
                (
                    r.tail.vault_log.clone(),
                    VaultLogDescriptor::from(r.sender.vault_name()).to_obj_desc(),
                ),
                (
                    r.tail.vault.clone(),
                    VaultDescriptor::from(r.sender.vault_name()).to_obj_desc(),
                ),
            ],
            ReadSyncRequest::SsRequest(r) => vec![(
                r.ss_log.clone(),
                SsLogDescriptor::from(r.sender.vault_name()).to_obj_desc(),
            )],
            _ => vec![],
        },
        _ => vec![],
    }
}

pub(super) fn validate_request(request: &SyncRequest) -> Result<()> {
    for (id, descriptor) in requested_streams(request) {
        validate_transport_id(&id)?;
        ensure!(id.fqdn == descriptor.fqdn(), "InvalidPublicReadTail");
    }
    if let SyncRequest::Write(write) = request
        && let WriteSyncRequest::DeviceTransportNoop(auth) = write.as_ref()
    {
        auth.validate_shape()?;
    }
    Ok(())
}

/// Explicit typed projections of the addressed public streams. Wrong stored
/// kinds/keys/owners are corruption, not skipped records or public serializers.
pub(super) async fn read_stream<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    from: ArtifactId,
    descriptor: ObjectDescriptor,
) -> Result<Vec<GenericKvLogEvent>> {
    validate_transport_id(&from)?;
    ensure!(from.fqdn == descriptor.fqdn(), "InvalidPublicReadTail");
    let mut id = from;
    let mut result = vec![];
    while let Some(event) = p.repo.find_one(id.clone()).await? {
        ensure!(
            event.obj_id() == id && event.key().obj_desc == descriptor,
            "InvalidPublicReadRecord"
        );
        validate_event(&event, &descriptor)?;
        result.push(event);
        id = next_transport_id(&id)?;
    }
    Ok(result)
}

pub(super) async fn addressed<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    request: &SyncRequest,
) -> Result<Vec<GenericKvLogEvent>> {
    validate_request(request)?;
    let mut events = vec![];
    for (id, descriptor) in requested_streams(request) {
        events.extend(read_stream(p, id, descriptor).await?);
    }
    Ok(events)
}

/// Fallback/discovery/audit must use the same typed context checks even when a
/// caller's requested tail is already beyond the last canonical record.
pub(super) async fn ss_history<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    name: &VaultName,
) -> Result<Vec<SsLogObject>> {
    let descriptor = SsLogDescriptor::from(name.clone());
    read_stream(
        p,
        ArtifactId::from(descriptor.clone()),
        descriptor.to_obj_desc(),
    )
    .await?
    .into_iter()
    .map(|event| event.ss_log())
    .collect()
}

fn validate_event(event: &GenericKvLogEvent, descriptor: &ObjectDescriptor) -> Result<()> {
    let name = descriptor.fqdn().obj_instance;
    let valid = match (event, descriptor) {
        (GenericKvLogEvent::VaultStatus(v), ObjectDescriptor::VaultStatus(_)) => {
            VaultStatusDescriptor::from(v.0.value.user().user_id()).to_obj_desc() == *descriptor
        }
        (GenericKvLogEvent::Vault(v), ObjectDescriptor::Vault(_)) => v.0.value.vault_name.0 == name,
        (GenericKvLogEvent::VaultLog(v), ObjectDescriptor::VaultLog(_)) => {
            v.0.value.requests.iter().all(|r| r.vault_name().0 == name)
                && v.0.value.updates.iter().all(|r| r.vault_name().0 == name)
                && v.0
                    .value
                    .join_results
                    .iter()
                    .all(|r| r.request.candidate.vault_name.0 == name)
                && v.0
                    .value
                    .deletion_results
                    .iter()
                    .all(|r| r.request.sender.user_data.vault_name.0 == name)
        }
        (GenericKvLogEvent::SsLog(v), ObjectDescriptor::SsLog(_)) => {
            v.0.value.claims.values().all(|c| c.vault_name.0 == name)
        }
        _ => false,
    };
    ensure!(valid, "InvalidPublicReadRecord");
    Ok(())
}

pub(super) fn guard_response(events: &[GenericKvLogEvent]) -> Result<()> {
    ensure!(
        events.iter().all(|event| matches!(
            event,
            GenericKvLogEvent::VaultStatus(_)
                | GenericKvLogEvent::VaultLog(_)
                | GenericKvLogEvent::Vault(_)
                | GenericKvLogEvent::SsLog(_)
                | GenericKvLogEvent::SsWorkflow(_)
        )),
        "InvalidPublicReadRecord"
    );
    Ok(())
}
