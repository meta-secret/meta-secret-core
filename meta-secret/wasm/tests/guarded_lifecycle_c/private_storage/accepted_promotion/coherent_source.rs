//! Labelled local private-metadata corruption; real immutable business source stays TWO-Pass.
use super::*;
use meta_secret_core::node::{
    common::model::liveboard_producer::producer_source_commitment,
    db::{
        events::{
            local_removal_envelope::RemovalEnvelopeMode,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{RemoveMemberEvent, VaultActionEvent, VaultActionUpdateEvent},
            },
        },
        objects::local_removal_envelope::PersistentRemovalEnvelope,
    },
};
use std::collections::HashSet;

type Case = fixture::Case<InMemKvLogEventRepo>;
#[derive(Clone, Copy, Debug)]
enum Entry {
    BeforeFirstSend,
    AppliedTailSkipsSigner,
}
#[derive(Clone, Copy)]
struct Counts {
    source: usize,
    upload: usize,
    noop: usize,
    material: usize,
    claims: usize,
}
fn counts(case: &Case) -> Counts {
    Counts {
        source: case.observations.sent.lock().unwrap().len(),
        upload: case.observations.uploads.lock().unwrap().len(),
        noop: case.observations.noops.load(Ordering::SeqCst),
        material: case.observations.live.lock().unwrap().len(),
        claims: case.observations.claim_writes.lock().unwrap().len(),
    }
}

#[tokio::test]
async fn actual_two_pass_source_rejects_coherent_private_subset_before_send_and_after_applied_tail()
-> Result<()> {
    // Both real paths execute and print their actual predicates before either failure escapes.
    let first = run(Entry::BeforeFirstSend).await;
    let second = run(Entry::AppliedTailSkipsSigner).await;
    for result in [first, second] {
        result?;
    }
    Ok(())
}

async fn full_source(case: &Case, receipt: &RemovalDecisionReceipt) -> Result<DeviceLogObject> {
    let witness = case.h.witness().await?;
    super::super::stage::private_crypto(&case.h.state, &witness)?;
    let body = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("genuine durable TWO-Pass body")?
        .device_log()?;
    let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = &body.0.value
    else {
        anyhow::bail!("actual source is not RemoveMember update")
    };
    let payload = witness
        .0
        .value
        .payload
        .as_ref()
        .context("FULL actual producer stage")?;
    let requested: HashSet<_> = update
        .request
        .share_transfers
        .iter()
        .filter(|t| t.receiver == *case.h.state.b.device_id())
        .map(|t| t.pass_id.clone())
        .collect();
    ensure!(
        payload.update == *update
            && requested.len() == 2
            && requested == case.h.state.passes.iter().cloned().collect()
            && requested == payload.passes.iter().map(|p| p.pass_id.clone()).collect(),
        "fixture source/request/private stage must initially contain the SAME TWO actual Passes"
    );
    let origin = witness
        .0
        .value
        .origin
        .as_ref()
        .context("actual queued origin")?;
    ensure!(
        origin.event_id == body.obj_id()
            && origin.request_id == receipt.request_id
            && origin.request_id == case.h.state.uuid
            && origin.accepted
            && origin.body_commitment == producer_source_commitment(&body)?,
        "actual source identity/hash differs"
    );
    let registry = PersistentRemovalEnvelope {
        p_obj: case.h.p.clone(),
    }
    .load(&case.h.state.b.user_id())
    .await?
    .context("genuine Guarded registry")?;
    ensure!(
        registry.0.value.first_managed_removal_nonce > 0
            && registry
                .0
                .value
                .entries
                .iter()
                .any(|e| e.event_id == body.obj_id()
                    && e.mode == RemovalEnvelopeMode::Guarded
                    && e.request_id.as_deref() == Some(&case.h.state.uuid)),
        "real full body must retain exact Guarded mode/UUID ownership"
    );
    Ok(body)
}

async fn partial_applied(case: &Case, frozen: &GuardedPreparationPayload) -> Result<()> {
    case.observations.fail_second.store(true, Ordering::SeqCst);
    let sync = case.gateway().sync(case.h.state.b.user()).await;
    let m = proof::measure(case).await?;
    m.print("binding V2 actual packet2/setup", &sync);
    ensure!(m.faults == 1, "V2 real second-new-packet fault UNREACHED");
    proof::common(&m)?;
    ensure!(
        sync.is_err(),
        "V2 actual packet2 interruption did not stop sync"
    );
    let witness = case.h.witness().await?;
    ensure!(
        witness.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
            && witness.0.value.payload.as_ref() == Some(frozen)
            && witness
                .0
                .value
                .origin
                .as_ref()
                .is_some_and(|o| o.business_signature.is_some()),
        "V2 needs real Applied and FULL partial promoting stage/original signature"
    );
    let e = case.observations.expected()?;
    let writes: Vec<_> = case
        .observations
        .live
        .lock()
        .unwrap()
        .iter()
        .filter(|w| proof::intended(&e, &w.packet))
        .cloned()
        .collect();
    ensure!(
        writes.len() == 2
            && writes[0].durable
            && !writes[1].durable
            && writes
                .iter()
                .all(|w| w.local_applied && w.promoting_readback),
        "V2 genuine first/second intended packet boundaries differ"
    );
    ensure!(
        frozen
            .passes
            .iter()
            .any(|p| p.distributions.contains(&writes[0].packet))
            && case
                .h
                .p
                .repo
                .inner
                .inner
                .find_one(writes[0].packet.obj_id())
                .await?
                == Some(writes[0].packet.clone().to_generic()),
        "V2 first frozen packet not actually durable"
    );
    eprintln!(
        "C2 binding V2 setup real_fault_hits=1 full_passes={} first_packet_durable=true allocated_claim_keys={}",
        frozen.passes.len(),
        witness
            .0
            .value
            .claims
            .iter()
            .filter(|c| c.ss_device_event_id.is_some())
            .count()
    );
    case.observations.fail_second.store(false, Ordering::SeqCst);
    Ok(())
}

async fn reduce_private_request(
    case: &Case,
    actual: &RemoveMemberEvent,
) -> Result<LocalGuardedPreparationObject> {
    let before = case.h.witness().await?;
    let mut reduced = before.clone();
    let payload = reduced
        .0
        .value
        .payload
        .as_mut()
        .context("FULL payload before subset corruption")?;
    ensure!(
        payload.passes.len() == 2 && payload.update == *actual,
        "must begin with unchanged FULL producer update"
    );
    let removed = payload.passes[1].clone();
    let packet_ids: HashSet<_> = removed
        .distributions
        .iter()
        .map(ObjIdExtractor::obj_id)
        .collect();
    let count_before = payload.update.request.share_transfers.len();
    payload
        .update
        .request
        .share_transfers
        .retain(|t| !(t.receiver == *case.h.state.b.device_id() && t.pass_id == removed.pass_id));
    ensure!(
        payload.update.request.share_transfers.len() + 1 == count_before,
        "must remove ONE actual later B-addressed transfer"
    );
    payload.passes.retain(|p| p.pass_id != removed.pass_id);
    reduced
        .0
        .value
        .claims
        .retain(|c| c.pass_id != removed.pass_id);
    reduced
        .0
        .value
        .packets
        .retain(|p| !packet_ids.contains(&p.event_id));
    let x = &reduced.0.value;
    let payload = x.payload.as_ref().unwrap();
    let requested: HashSet<_> = payload
        .update
        .request
        .share_transfers
        .iter()
        .filter(|t| t.receiver == *case.h.state.b.device_id())
        .map(|t| t.pass_id.clone())
        .collect();
    ensure!(
        requested.len() == 1
            && payload.passes.len() == 1
            && x.claims.len() == 1
            && x.packets.len() == 2
            && requested.contains(&payload.passes[0].pass_id)
            && x.claims[0].claim == payload.passes[0].claim
            && x.packets.iter().all(|p| payload.passes[0]
                .distributions
                .iter()
                .any(|d| d.obj_id() == p.event_id))
            && payload.update != *actual
            && x.origin == before.0.value.origin
            && x.phase == before.0.value.phase,
        "local corruption must remain a coherent ONE-Pass subset with unchanged origin/phase"
    );
    // ONLY the local private fixed record changes. Immutable source and server remain untouched.
    case.h.p.repo.inner.inner.replace(reduced.clone()).await?;
    ensure!(
        case.h.witness().await? == reduced,
        "actual private corruption readback failed"
    );
    Ok(reduced)
}

async fn run(entry: Entry) -> Result<()> {
    let case = Case::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let body = full_source(&case, &receipt).await?;
    if matches!(entry, Entry::AppliedTailSkipsSigner) {
        partial_applied(&case, &frozen).await?;
        let response = case
            .wire
            .send(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
                ServerTailRequest {
                    sender: case.h.state.b.user(),
                    producer_reference: None,
                },
            ))))
            .await?
            .to_server_tail()?;
        ensure!(
            response
                .device_log_tail
                .is_some_and(|tail| tail.fqdn == body.obj_id().fqdn
                    && tail.id.curr >= body.obj_id().next().id.curr),
            "V2 actual advanced ServerTail UNREACHED"
        );
        eprintln!("C2 binding V2 actual_server_tail_past_body=true signer_skip_route=true");
    }
    let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) = &body.0.value
    else {
        anyhow::bail!("actual immutable update")
    };
    let server_before_corruption = case.h.state.server_repo.get_db().await;
    let reduced = reduce_private_request(&case, update).await?;
    ensure!(
        case.h.p.repo.inner.inner.find_one(body.obj_id()).await? == Some(body.clone().to_generic())
            && case.h.state.server_repo.get_db().await == server_before_corruption,
        "fixture corruption changed immutable body or actual canonical/server state"
    );
    let before = case.h.p.repo.snapshot().await?;
    let c = counts(&case);
    let result = case.gateway().sync(case.h.state.b.user()).await;
    let after = case.h.p.repo.snapshot().await?;
    let end = counts(&case);
    let unavailable = result
        .as_ref()
        .err()
        .is_some_and(|e| e.to_string() == "GuardedPreparationUnavailable");
    let body_same =
        case.h.p.repo.inner.inner.find_one(body.obj_id()).await? == Some(body.to_generic());
    let private_same = case.h.witness().await? == reduced;
    let db_same = after == before;
    let server_same = case.h.state.server_repo.get_db().await == server_before_corruption;
    let no_send = end.source == c.source && end.upload == c.upload && end.noop == c.noop;
    let no_publish = end.material == c.material && end.claims == c.claims;
    eprintln!(
        "C2 coherent-binding {entry:?} full_source_passes=2 private_passes=1 static_unavailable={unavailable} body_same={body_same} private_same={private_same} db_same={db_same} server_same={server_same} source_delta={} upload_delta={} noop_delta={} material_delta={} claim_write_delta={}",
        end.source - c.source,
        end.upload - c.upload,
        end.noop - c.noop,
        end.material - c.material,
        end.claims - c.claims
    );
    ensure!(
        unavailable,
        "ordinary sync accepted coherent private subset of its actual TWO-Pass source"
    );
    ensure!(
        body_same && private_same && db_same && server_same && no_send && no_publish,
        "rejected coherent subset changed source/signature/private phase/material or server state"
    );
    Ok(())
}
