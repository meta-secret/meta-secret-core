//! Exact source proofs and existing public singleton Show/GetState checks.
use super::*;
use meta_secret_core::node::db::{
    descriptors::{object_descriptor::ToObjectDescriptor, vault_descriptor::VaultLogDescriptor},
    events::{
        generic_log_event::KeyExtractor,
        kv_log_event::KvKey,
        vault::{
            liveboard_metadata::RemovalApplicationReceipt,
            producer_metadata::{VerifiedProducerKind, VerifiedProducerSubject},
        },
    },
};
pub(super) async fn actual_source(
    h: &harness::Harness,
    source: &ProducerSourceRef,
) -> Result<(DeviceLogObject, SignedAction, usize)> {
    let body = h
        .local
        .inner
        .find_one(source.event_id.clone())
        .await?
        .context("actual producer source body")?
        .device_log()?;
    let raw = body.clone().to_generic();
    let server_repo = &h.case.h.state.server_repo;
    let remote = server_repo.find_one(source.event_id.clone()).await?;
    ensure!(
        producer_source_commitment(&body)? == source.body_commitment && remote == Some(raw.clone()),
        "actual source commitment/raw server ACK differs"
    );
    let sent = {
        let calls = h.wire.calls.lock().unwrap();
        calls
            .iter()
            .find_map(|call| {
                let SyncRequest::Write(w) = &call.request else {
                    return None;
                };
                let WriteSyncRequest::Event(action) = &**w else {
                    return None;
                };
                matches!(&action.payload, SignedActionPayload::Event(e) if *e == raw)
                    .then(|| (action.clone(), call.order))
            })
            .context("actual forwarded signed source")?
    };
    let a = &h.case.h.state.b;
    sent.0.verify(&a.user().device.keys.dsa_pk)?;
    ensure!(
        sent.0.signer == *a.device_id()
            && sent.0.stream == source.event_id.fqdn.clone().id_str()
            && sent.0.nonce == source.event_id.id.curr as u64,
        "actual signature/stream/nonce differs"
    );
    ensure!(h.wire.replies.lock().unwrap().iter().any(|r| r.order > sent.1
        && matches!(&r.request, SyncRequest::Write(w) if matches!(&**w, WriteSyncRequest::Event(s) if s == &sent.0))
        && r.response.clone().ensure_success().is_ok()), "actual successful source response missing");
    Ok((body, sent.0, sent.1))
}
/// Host-only canonical proof; private producer refs are NOT a client authorization input.
pub(super) async fn later_uuid(
    h: &harness::Harness,
    before: &VaultObject,
    current: &VaultObject,
    head: &VaultLogObject,
    r: &DeviceLogObject,
    u: &DeviceLogObject,
    signed_u: &SignedAction,
) -> Result<String> {
    let (
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)),
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
    ) = (&r.0.value, &u.0.value)
    else {
        anyhow::bail!("actual ordinary R/U types required for Prepared proof")
    };
    let a = &h.case.h.state.b;
    let descriptor = VaultLogDescriptor::from(a.vault_name.clone());
    let server = PersistentObject::new(h.case.h.state.server_repo.clone());
    let logs: Vec<VaultLogObject> = server
        .get_object_events_from_beginning(descriptor.clone())
        .await?;
    let mut expected = ArtifactId::from(descriptor.clone());
    let mut selected: Option<RemovalApplicationReceipt> = None;
    for log in &logs {
        ensure!(
            log.key() == KvKey::artifact(descriptor.clone().to_obj_desc(), expected.clone()),
            "actual same-Vault history descriptor/sequence mismatch"
        );
        log.0.value.validate_removal_bindings(true)?;
        expected = expected.next();
        for receipt in &log.0.value.liveboard_removal_applications {
            if receipt.update != *update {
                continue;
            }
            ensure!(
                receipt.update.request == *request,
                "actual Prepared request does not match raw R"
            );
            ensure!(
                log.0
                    .value
                    .liveboard_removal_results
                    .iter()
                    .any(|result| result.request_id == receipt.request_id
                        && result.application_state == RemovalApplicationState::Prepared
                        && result.outcome == DecisionOutcome::Accepted),
                "actual matching Prepared result absent"
            );
            if let Some(prior) = &selected {
                ensure!(prior == receipt, "ambiguous distinct Prepared receipt/UUID");
            } else {
                selected = Some(receipt.clone());
            }
        }
    }
    ensure!(
        logs.last() == Some(head),
        "actual complete history does not reach raw head"
    );
    let selected = selected.context("actual unique historical Prepared U receipt absent")?;
    ensure!(
        meta_secret_core::node::api::is_liveboard_request_id(&selected.request_id)
            && selected.base_vault_artifact_id == before.obj_id()
            && selected.target_vault_artifact_id == current.obj_id()
            && selected.target_vault_artifact_id == before.obj_id().next(),
        "actual Prepared base/target/UUID lineage mismatch"
    );
    ensure!(
        head.0
            .value
            .liveboard_removal_results
            .iter()
            .filter(|result| result.request_id == selected.request_id)
            .count()
            == 1,
        "actual unique current result UUID absent"
    );
    // Ordinary local2→1 result.source=None is valid; separately verify actual PRIVATE server correlation.
    let record = head
        .0
        .value
        .producer_source(&u.obj_id())?
        .context("actual private U correlation absent")?;
    ensure!(
        record.kind == VerifiedProducerKind::LocalRemoveAccept
            && record.source_event_id == u.obj_id()
            && record.body_commitment == producer_source_commitment(u)?
            && record.verified_envelope_digest == publication::hash(signed_u)?
            && record.actor_id == *a.device_id()
            && record.vault_name == a.vault_name
            && record.request_id == selected.request_id
            && record.subject
                == VerifiedProducerSubject::Device(h.case.h.state.a.device_id().clone()),
        "actual private verified U/source/actor/Vault/subject/UUID mismatch"
    );
    Ok(selected.request_id)
}

pub(super) async fn terminal_before_clear(
    h: &harness::Harness,
    terminal: &LocalGuardedPreparationObject,
    r: &DeviceLogObject,
    u: &DeviceLogObject,
    returned: usize,
) -> Result<()> {
    let intended = terminal.clone().to_generic();
    let owner = h.case.h.state.b.user_id();
    {
        let calls = h.repo.calls.lock().unwrap();
        let writes = h.repo.writes.lock().unwrap();
        let reads = h.repo.reads.lock().unwrap();
        let installed = writes
            .iter()
            .find(|w| {
                w.event == intended
                    && w.success
                    && w.exact
                    && w.done < returned
                    && calls.iter().any(|c| {
                        c.order < w.forward
                            && matches!(&c.op, repo::Op::Write(true, e) if **e == intended)
                    })
            })
            .context("LATE actual existing-only exact terminal replacement absent")?;
        let cleared = writes
            .iter()
            .find(|w| {
                w.forward > installed.done
                    && w.done < returned
                    && w.success
                    && w.exact
                    && matches!(&w.event, GenericKvLogEvent::LocalRemovalEnvelope(e)
                if e.0.value.owner == owner && e.0.value.producer_context.is_none()
                    && e.0.value.entries.iter().any(|entry| entry.event_id == u.obj_id()))
            })
            .context("LATE actual producer context clear absent")?;
        ensure!(
            reads.iter().any(|read| read.order > installed.done
                && read.order < cleared.forward
                && read.event.as_ref() == Some(&intended)),
            "LATE whole terminal PRODUCTION readback must precede clear"
        );
        let previous = writes.iter().rev().find(|w| w.done < cleared.forward && w.success && w.exact
            && matches!(&w.event, GenericKvLogEvent::LocalRemovalEnvelope(e)
                if e.0.value.owner == owner && e.0.value.producer_context.as_ref().is_some_and(|c|
                    c.request_stage.source.event_id == r.obj_id()
                        && c.decision_stage.as_ref().is_some_and(|d| d.source.event_id == u.obj_id()))))
            .context("LATE cleared context was not the actual R/U context")?;
        let mut expected_clear = meta_secret_core::node::db::events::local_removal_envelope::LocalRemovalEnvelopeObject::try_from(previous.event.clone())?;
        expected_clear.0.value.producer_context = None;
        expected_clear.0.key.obj_id = expected_clear.obj_id().next();
        ensure!(
            cleared.event == expected_clear.to_generic(),
            "LATE clear changed registry fields beyond exact context/next artifact"
        );
        ensure!(
            reads.iter().any(|read| read.order > cleared.done
                && read.order < returned
                && read.event.as_ref() == Some(&cleared.event)),
            "LATE actual clear PRODUCTION readback absent"
        );
    }
    let registry =
        meta_secret_core::node::db::objects::local_removal_envelope::PersistentRemovalEnvelope {
            p_obj: h.p.clone(),
        }
        .load(&owner)
        .await?
        .context("LATE actual registry absent")?;
    ensure!(
        registry.0.value.producer_context.is_none() && h.case.h.witness().await? == *terminal,
        "LATE cleared context or exact monotonic terminal absent"
    );
    Ok(())
}

pub(super) async fn ready_singleton(
    h: &harness::Harness,
    old: &LocalGuardedPreparationObject,
    terminal: &LocalGuardedPreparationObject,
    r: &DeviceLogObject,
    u: &DeviceLogObject,
    repeat_marker: (usize, usize, usize),
) -> Result<()> {
    let before = h.local.inner.get_db().await;
    let service = Arc::new(h.service());
    let manager = meta_secret_wasm::app_manager::ApplicationManager::new(
        h.wire.clone(),
        h.gateway.clone(),
        service.clone(),
        h.case.h.state.master.clone(),
    );
    let shown = manager
        .show_local_secret(h.case.h.state.passes[0].clone())
        .await?;
    ensure!(
        shown.text == "C1_PRIVATE_FIXTURE_FIRST",
        "actual singleton retained Show mismatch"
    );
    let state = service.get_app_state().await?;
    ensure!(
        matches!(state, ApplicationState::Vault(VaultFullInfo::Member(m)) if m.member.vault.members().len() == 1),
        "actual singleton GetState failed"
    );
    let witness = h.case.h.witness().await?;
    ensure!(
        witness == *terminal,
        "repeat resurrected FULL or changed original compact"
    );
    for pass in &h.case.h.state.passes {
        for peer in [&h.case.h.state.a, &h.case.h.state.c] {
            ensure!(
                h.local
                    .inner
                    .find_one(packet_id(pass, peer))
                    .await?
                    .is_none(),
                "old peer ciphertext resurrected after singleton repeat"
            );
        }
    }
    let prefix = ArtifactId::from(DeviceLogDescriptor::from(h.case.h.state.b.user_id())).fqdn;
    for event in h.local.inner.get_db().await.into_values() {
        if let GenericKvLogEvent::DeviceLog(body) = &event
            && body.obj_id().fqdn == prefix
            && body.obj_id().id.curr > r.obj_id().id.curr
        {
            ensure!(
                *body == *u && before.get(&event.obj_id()) == Some(&event),
                "repeat created/replaced a decision"
            );
        }
    }
    // ALL attempted writes/uploads since BEFORE the first repeat, not a final DB-only oracle.
    {
        let calls = h.repo.calls.lock().unwrap();
        for call in &calls[repeat_marker.0..] {
            if let repo::Op::Write(_, event) = &call.op {
                ensure!(
                    not_old_output(old, event)?,
                    "repeat attempted old owned output/private FULL write"
                );
            }
        }
    }
    {
        let writes = h.repo.writes.lock().unwrap();
        for write in &writes[repeat_marker.1..] {
            ensure!(
                not_old_output(old, &write.event)?,
                "repeat forwarded old owned output/private FULL write"
            );
        }
    }
    {
        let calls = h.wire.calls.lock().unwrap();
        for call in &calls[repeat_marker.2..] {
            if let Some(event) = proof::signed_event(&call.request) {
                ensure!(
                    not_old_output(old, &event)?,
                    "repeat uploaded old owned output"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn not_old_output(
    old: &LocalGuardedPreparationObject,
    event: &GenericKvLogEvent,
) -> Result<bool> {
    match event {
        GenericKvLogEvent::LocalGuardedPreparation(w) => {
            Ok(w.obj_id() != old.obj_id() || w.0.value.payload.is_none())
        }
        GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) => {
            let Some(owned) = old
                .0
                .value
                .packets
                .iter()
                .find(|p| p.event_id == packet.obj_id())
            else {
                return Ok(true);
            };
            if digest(packet)? != owned.body_commitment {
                return Ok(true); // Same fixed key alone is not the old generation.
            }
            let data = packet.clone().to_distribution_data()?;
            let channel = &data.secret_message.cipher_text().channel;
            let sender = old
                .0
                .value
                .survivors
                .iter()
                .find(|s| s.device.device_id == old.0.value.owner.device_id)
                .context("exact old packet owner")?;
            ensure!(
                data.vault_name == old.0.value.owner.vault_name
                    && old
                        .0
                        .value
                        .claims
                        .iter()
                        .any(|c| c.claim.dist_claim_id == data.claim_id
                            && c.pass_id == data.claim_id.pass_id)
                    && channel.receiver().to_device_id() == owned.receiver_id
                    && channel.sender() == &sender.device.keys.transport_pk,
                "old commitment/packet Vault Pass receiver channel binding differs"
            );
            Ok(false)
        }
        GenericKvLogEvent::SsDeviceLog(c) => {
            let actual = &c.0.value;
            Ok(!old.0.value.claims.iter().any(|c| {
                let intended = &c.claim;
                (
                    &actual.id,
                    &actual.dist_claim_id,
                    &actual.vault_name,
                    &actual.sender,
                    &actual.distribution_type,
                    &actual.receivers,
                ) == (
                    &intended.id,
                    &intended.dist_claim_id,
                    &intended.vault_name,
                    &intended.sender,
                    &intended.distribution_type,
                    &intended.receivers,
                )
            }))
        }
        // Cumulative canonical SsLog history alone is not an append/publication replay.
        _ => Ok(true),
    }
}
