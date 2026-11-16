//! Public canonical content ends obsolete publication; it is not an envelope proof.
use super::*;
use crate::node::{
    common::model::{
        liveboard_producer::producer_source_commitment,
        user::common::{UserDataOutsiderStatus, UserMembership},
    },
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::vault::{
            device_log_event::DeviceLogObject,
            liveboard_metadata::{
                DecisionOutcome, RemovalApplicationReceipt, RemovalApplicationState,
            },
            vault_log_event::{RemoveMemberDecision, VaultActionEvent},
        },
        objects::{
            device_transport::DeviceTransportSlot,
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_device_log::PersistentDeviceLog,
        },
    },
    device_transport_noop::next_transport_id,
};

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(super) async fn prove_later_singleton(
        &self,
        user: &UserData,
        object: &LocalGuardedPreparationObject,
        current: &VaultObject,
        original: &RemovalResultRecord,
    ) -> Result<()> {
        let value = &object.0.value;
        ensure!(
            value.phase == GuardedPreparationPhase::AcceptedPublished
                && value.payload.is_none()
                && value.owner == user.user_id()
                && value.survivors.len() == 2
                && value.survivors.contains(user)
                && current.0.value.members().len() == 1
                && matches!(current.0.value.find_user(&user.device.device_id),
                Some(UserMembership::Member(m)) if m.user_data == *user),
            UNAVAILABLE
        );
        let other = value
            .survivors
            .iter()
            .find(|u| *u != user)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(
            matches!(current.0.value.find_user(&other.device.device_id),
            Some(UserMembership::Outsider(o)) if o.user_data == *other
                && o.status == UserDataOutsiderStatus::Declined),
            UNAVAILABLE
        );
        let descriptor = VaultLogDescriptor::from(value.owner.vault_name.clone());
        let persistent = PersistentVault::from(self.p_obj.clone());
        let head = persistent
            .get_vault_log_artifact(value.owner.vault_name.clone())
            .await?;
        let logs: Vec<VaultLogObject> = self
            .p_obj
            .get_object_events_from_beginning(descriptor.clone())
            .await?;
        let mut expected = ArtifactId::from(descriptor.clone());
        for log in &logs {
            ensure!(
                log.key() == KvKey::artifact(descriptor.clone().to_obj_desc(), expected.clone()),
                UNAVAILABLE
            );
            log.0.value.validate_removal_bindings(true)?;
            expected = expected.next();
        }
        ensure!(logs.last() == Some(&head), UNAVAILABLE);
        let old_at = applied_at(&logs, original)?;
        let mut candidates = vec![];
        for result in &head.0.value.liveboard_removal_results {
            if result.request_id != original.request_id
                && result.requester_id == user.device.device_id
                && result.responder_id == user.device.device_id
                && result.target_id == other.device.device_id
                && result.outcome == DecisionOutcome::Accepted
                && result.application_state == RemovalApplicationState::Applied
            {
                let at = applied_at(&logs, result)?;
                if at > old_at {
                    candidates.push((result, at));
                }
            }
        }
        let [(later, applied)] = candidates.as_slice() else {
            anyhow::bail!(UNAVAILABLE)
        };
        let (old, old_prepared, _) = prepared(&logs, &original.request_id)?;
        let (receipt, prepared_at, pending_at) = prepared(&logs, &later.request_id)?;
        ensure!(
            old_prepared < old_at
                && old_at < pending_at
                && pending_at < prepared_at
                && prepared_at < *applied
                && old.base_vault_artifact_id == value.base_vault
                && old.target_vault_artifact_id == value.base_vault.clone().next()
                && old.update.sender.user_data == *user
                && old.update.request.sender.user_data == *other
                && old.update.target.user_data.device.device_id == value.target_id,
            UNAVAILABLE
        );
        let update = &receipt.update;
        ensure!(
            update.decision == RemoveMemberDecision::Accepted
                && update.sender == update.request.sender
                && update.sender.user_data == *user
                && update.target == update.request.target
                && update.target.user_data == *other
                && matches!(&update.new_status, UserMembership::Outsider(o)
                if o.user_data == *other && o.status == UserDataOutsiderStatus::Declined)
                && receipt.target_vault_artifact_id == current.obj_id()
                && receipt.target_vault_artifact_id
                    == receipt.base_vault_artifact_id.clone().next(),
            UNAVAILABLE
        );
        let vault_descriptor = VaultDescriptor::from(value.owner.vault_name.clone());
        let vaults: Vec<VaultObject> = self
            .p_obj
            .get_object_events_from_beginning(vault_descriptor.clone())
            .await?;
        let mut expected = ArtifactId::from(vault_descriptor.clone());
        for vault in &vaults {
            ensure!(
                vault.key()
                    == KvKey::artifact(vault_descriptor.clone().to_obj_desc(), expected.clone())
                    && vault.0.value.vault_name == value.owner.vault_name,
                UNAVAILABLE
            );
            expected = expected.next();
        }
        ensure!(vaults.last() == Some(current), UNAVAILABLE);
        let start = vaults
            .iter()
            .position(|v| v.obj_id() == old.target_vault_artifact_id)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let base = vaults
            .iter()
            .position(|v| v.obj_id() == receipt.base_vault_artifact_id)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(
            start <= base
                && vaults[start..=base].iter().all(|v| {
                    let members = v.0.value.members();
                    members.len() == 2
                        && members
                            .iter()
                            .all(|m| value.survivors.contains(&m.user_data))
                }),
            UNAVAILABLE
        );
        let before = &vaults[base].0.value;
        before.ensure_can_remove_member(&update.request, Some(&update.sender))?;
        before.ensure_valid_removal_transfers(&update.request, &update.request.share_transfers)?;
        before.ensure_valid_removal_transfers(&update.request, &update.share_transfers)?;
        ensure!(
            before.clone().update_membership(update.new_status.clone()) == current.0.value,
            UNAVAILABLE
        );
        let origin = value
            .origin
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let slots = PersistentDeviceLog::from(self.p_obj.clone())
            .transport_slots(
                &value.owner,
                ArtifactId::from(DeviceLogDescriptor::from(value.owner.clone())),
            )
            .await?;
        let (request, decision) = raw_pair(&slots, &receipt, &origin.event_id)?;
        // The public ordinary result may have source=None. These are local full-content
        // checks, never a fabricated canonical signed-envelope digest or nonce proof.
        if let Some(context) = (PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        })
        .load(&value.owner)
        .await?
        .and_then(|r| r.0.value.producer_context)
        {
            ensure!(
                context.owner == value.owner
                    && context.target == other.device.device_id
                    && context.admission_vault == receipt.base_vault_artifact_id
                    && context
                        .request_id
                        .as_ref()
                        .is_none_or(|id| id == &later.request_id)
                    && context.request_stage.source.event_id == request.obj_id()
                    && context.request_stage.source.body_commitment
                        == producer_source_commitment(request)?
                    && context
                        .decision_stage
                        .as_ref()
                        .is_some_and(|d| d.source.event_id == decision.obj_id()
                            && producer_source_commitment(decision)
                                .is_ok_and(|h| h == d.source.body_commitment)),
                UNAVAILABLE
            );
        }
        // A clear context grants nothing; the same full canonical/raw proof above is mandatory.
        Ok(())
    }
}

fn applied_at(logs: &[VaultLogObject], result: &RemovalResultRecord) -> Result<usize> {
    ensure!(
        result.application_state == RemovalApplicationState::Applied,
        UNAVAILABLE
    );
    let mut first_prepared: Option<&RemovalApplicationReceipt> = None;
    let mut first_applied = None;
    for (index, log) in logs.iter().enumerate() {
        log.0.value.validate_removal_bindings(true)?;
        let Some(actual) = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == result.request_id)
        else {
            ensure!(first_prepared.is_none(), UNAVAILABLE);
            continue;
        };
        let mut expected = result.clone();
        expected.application_state = actual.application_state;
        ensure!(
            actual == &expected
                && !log
                    .0
                    .value
                    .liveboard_removal_pending
                    .iter()
                    .any(|p| p.request_id == result.request_id),
            UNAVAILABLE
        );
        match actual.application_state {
            RemovalApplicationState::Prepared => {
                ensure!(first_applied.is_none(), UNAVAILABLE);
                let receipt = log
                    .0
                    .value
                    .liveboard_removal_applications
                    .iter()
                    .find(|r| r.request_id == result.request_id)
                    .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
                let update = &receipt.update;
                let outcome = match update.decision {
                    RemoveMemberDecision::Accepted => DecisionOutcome::Accepted,
                    RemoveMemberDecision::Declined => DecisionOutcome::Declined,
                };
                ensure!(
                    outcome == result.outcome
                        && update.request.sender.user_data.device.device_id == result.requester_id
                        && update.sender.user_data.device.device_id == result.responder_id
                        && update.target.user_data.device.device_id == result.target_id
                        && update.target == update.request.target
                        && first_prepared.is_none_or(|previous| previous == receipt),
                    UNAVAILABLE
                );
                first_prepared.get_or_insert(receipt);
            }
            RemovalApplicationState::Applied => {
                ensure!(first_prepared.is_some(), UNAVAILABLE);
                first_applied.get_or_insert(index);
            }
        }
    }
    first_applied.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))
}

fn prepared(
    logs: &[VaultLogObject],
    id: &str,
) -> Result<(RemovalApplicationReceipt, usize, usize)> {
    let mut found: Option<(RemovalApplicationReceipt, usize)> = None;
    for (index, log) in logs.iter().enumerate() {
        for receipt in &log.0.value.liveboard_removal_applications {
            if receipt.request_id == id {
                if let Some((previous, _)) = &found {
                    ensure!(previous == receipt, UNAVAILABLE);
                } else {
                    found = Some((receipt.clone(), index));
                }
            }
        }
    }
    let (receipt, at) = found.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
    let mut first_pending = None;
    for (index, log) in logs.iter().enumerate() {
        for pending in &log.0.value.liveboard_removal_pending {
            if pending.request_id == id {
                ensure!(pending.request == receipt.update.request, UNAVAILABLE);
                first_pending.get_or_insert(index);
            }
        }
    }
    let pending = first_pending.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
    ensure!(pending < at, UNAVAILABLE);
    Ok((receipt, at, pending))
}

fn raw_pair<'a>(
    slots: &'a [DeviceTransportSlot],
    receipt: &RemovalApplicationReceipt,
    origin: &ArtifactId,
) -> Result<(&'a DeviceLogObject, &'a DeviceLogObject)> {
    let mut found = None;
    for adjacent in slots.windows(2) {
        let [
            DeviceTransportSlot::Business(r),
            DeviceTransportSlot::Business(u),
        ] = adjacent
        else {
            continue;
        };
        if r.obj_id().fqdn == origin.fqdn
            && r.obj_id().id.curr > origin.id.curr
            && matches!(&r.0.value, VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(v)) if *v == receipt.update.request)
            && matches!(&u.0.value, VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(v)) if *v == receipt.update)
        {
            ensure!(
                found.is_none() && u.obj_id() == next_transport_id(&r.obj_id())?,
                UNAVAILABLE
            );
            found = Some((&**r, &**u));
        }
    }
    found.ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))
}

#[cfg(test)]
#[path = "guarded_applied_singleton_tests.rs"]
mod tests;
