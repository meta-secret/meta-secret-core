//! Validate the complete addressed packet set before any delivery side effect.
use anyhow::{Result, ensure};
use meta_secret_core::node::{
    api::SsRequest,
    common::model::{
        device::common::DeviceId,
        secret::{ClaimId, SecretDistributionType, SsClaim, SsDistributionId, SsLogData},
    },
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SsWorkflowDescriptor,
        },
        events::{
            generic_log_event::*, object_id::ArtifactId, shared_secret_event::SsWorkflowObject,
        },
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
    device_transport_noop::next_transport_id,
};
use std::collections::HashSet;

pub(super) struct Delivery {
    pub event: SsWorkflowObject,
    pub split_claim: Option<ClaimId>,
}

pub(super) async fn addressed<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    request: &SsRequest,
    log: &SsLogData,
    server_device: &DeviceId,
) -> Result<Vec<Delivery>> {
    let mut result = vec![];
    // Sequential delivery consumed this physical packet on its first visit.
    // Preserve that claim association while validating the whole set first.
    let mut physical = HashSet::new();
    for claim in log.claims.values() {
        ensure!(claim.sender != *server_device, "InvalidPublicReadRecord");
        ensure!(
            claim.vault_name == request.sender.vault_name,
            "InvalidPublicReadRecord"
        );
        let receiver = &request.sender.device.device_id;
        match claim.distribution_type {
            SecretDistributionType::Split => {
                // Existing current-value semantics intentionally do not depend
                // on historical claim generations or stale receiver lists.
                let descriptor = SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: claim.dist_claim_id.pass_id.clone(),
                    receiver: receiver.clone(),
                });
                if let Some(event) = latest(p, descriptor, claim).await?
                    && physical.insert(event.obj_id())
                {
                    result.push(Delivery {
                        event,
                        split_claim: Some(claim.id.clone()),
                    });
                }
            }
            SecretDistributionType::Recover => {
                for id in claim.recovery_db_ids() {
                    if &id.sender != receiver {
                        continue;
                    }
                    if let Some(event) =
                        latest(p, SsWorkflowDescriptor::Recovery(id), claim).await?
                        && physical.insert(event.obj_id())
                    {
                        result.push(Delivery {
                            event,
                            split_claim: None,
                        });
                    }
                }
            }
        }
    }
    Ok(result)
}

async fn latest<R: KvLogEventRepo>(
    p: &PersistentObject<R>,
    descriptor: SsWorkflowDescriptor,
    claim: &SsClaim,
) -> Result<Option<SsWorkflowObject>> {
    let expected = descriptor.clone().to_obj_desc();
    let mut id = ArtifactId::from(descriptor.clone());
    let mut latest = None;
    while let Some(raw) = p.repo.find_one(id.clone()).await? {
        ensure!(
            raw.obj_id() == id && raw.key().obj_desc == expected,
            "InvalidPublicReadRecord"
        );
        let GenericKvLogEvent::SsWorkflow(event) = raw else {
            anyhow::bail!("InvalidPublicReadRecord");
        };
        validate(&event, &descriptor, claim)?;
        latest = Some(event);
        id = next_transport_id(&id)?;
    }
    Ok(latest)
}

fn validate(
    event: &SsWorkflowObject,
    descriptor: &SsWorkflowDescriptor,
    claim: &SsClaim,
) -> Result<()> {
    match (descriptor, event) {
        (SsWorkflowDescriptor::Distribution(id), SsWorkflowObject::Distribution(packet)) => {
            ensure!(
                packet.value.vault_name == claim.vault_name
                    && packet.value.claim_id.pass_id == id.pass_id
                    && packet
                        .value
                        .secret_message
                        .cipher_text()
                        .channel
                        .receiver()
                        .to_device_id()
                        == id.receiver,
                "InvalidPublicReadRecord"
            );
            Ok(())
        }
        (SsWorkflowDescriptor::Recovery(id), SsWorkflowObject::Recovery(_)) => {
            super::recovery_response::validate_workflow(event, claim, &id.distribution_id.receiver)
        }
        _ => anyhow::bail!("InvalidPublicReadRecord"),
    }
}
