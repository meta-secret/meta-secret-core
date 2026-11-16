use super::{
    repo::{Fault, Point, Trace},
    support::*,
    *,
};
use meta_secret_core::{
    node::{
        common::model::{
            liveboard::LiveboardOperationPhase, liveboard_producer::producer_source_commitment,
            secret::SsDistributionId,
        },
        db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
    },
    recover_from_shares,
    secret::shared_secret::UserShareDto,
};

pub fn private_crypto(state: &State, object: &LocalGuardedPreparationObject) -> Result<()> {
    let x = &object.0.value;
    ensure!(
        object.obj_id() == state.witness_id()
            && x.owner == state.b.user_id()
            && x.request_id == state.uuid,
        "fixed witness owner/key/UUID mismatch"
    );
    let payload = x
        .payload
        .as_ref()
        .context("complete private payload required")?;
    ensure!(
        payload.passes.len() == 2,
        "ALL actual Passes must be frozen before queue"
    );
    let mut ids = std::collections::HashSet::new();
    for pass in &payload.passes {
        ensure!(
            state.passes.contains(&pass.pass_id) && ids.insert(pass.pass_id.clone()),
            "private Pass set differs"
        );
        ensure!(pass.distributions.len() == 2, "two real survivors required");
        let mut receivers = std::collections::HashSet::new();
        for packet in &pass.distributions {
            let data = packet.clone().to_distribution_data()?;
            ensure!(
                data.claim_id == pass.claim.dist_claim_id && data.claim_id.pass_id == pass.pass_id,
                "private packet/claim generation differs"
            );
            let receiver = data
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id();
            let user = if receiver == *state.a.device_id() {
                &state.a
            } else {
                ensure!(
                    receiver == *state.b.device_id(),
                    "removed/foreign recipient got private material"
                );
                &state.b
            };
            ensure!(receivers.insert(receiver), "duplicate private survivor");
            let plain = data
                .secret_message
                .cipher_text()
                .decrypt(&user.device_creds.secret_box.transport.sk)?;
            let share = UserShareDto::try_from(&plain.msg)?;
            ensure!(
                !share.share_blocks.is_empty()
                    && share
                        .share_blocks
                        .iter()
                        .all(|b| b.config.threshold == 1 && b.config.number_of_shares == 2),
                "private output is not actual K1-of2"
            );
            let expected = if pass.pass_id == state.passes[0] {
                "C1_PRIVATE_FIXTURE_FIRST"
            } else {
                "C1_PRIVATE_FIXTURE_SECOND"
            };
            ensure!(
                recover_from_shares(vec![share])?.text == expected,
                "private output cannot recover exact fixture Secret"
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn s01_all_pass_private_frozen_and_verified_before_origin_actual_body() -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = h.p.repo.snapshot().await?;
    let receipt = h.decide().await?;
    let observations = h.p.repo.before_body.lock().unwrap().clone();
    eprintln!(
        "C1b S01 actual_body={} index_before={} witness_before={}",
        observations.len(),
        observations.first().is_some_and(|x| x.1.is_some()),
        observations.first().is_some_and(|x| x.2.is_some())
    );
    let (body, index, witness) = observations
        .first()
        .context("actual body save boundary required")?;
    let index: LocalGuardedPreparationIndexObject = index
        .clone()
        .context("changed private index MUST precede body")?
        .try_into()?;
    ensure!(
        index.0.value.revision > 0 && index.0.value.request_ids == vec![h.state.uuid.clone()],
        "actual enrollment not proven"
    );
    let witness: LocalGuardedPreparationObject = witness
        .clone()
        .context("complete witness MUST precede actual body")?
        .try_into()?;
    private_crypto(&h.state, &witness)?;
    for prepared in &witness.0.value.payload.as_ref().unwrap().passes {
        let original = before
            .get(&prepared.input.distribution_id)
            .context("frozen input ID is not an original real packet")?
            .clone()
            .shared_secret()?;
        let data = original.clone().to_distribution_data()?;
        let digest = meta_secret_core::crypto::utils::sha256_hex(&String::from_utf8(
            meta_secret_core::node::security::canonical_json(&original)?,
        )?);
        ensure!(
            prepared.input.claim_id == data.claim_id
                && prepared.input.distribution_commitment == digest,
            "frozen input generation/digest differs from actual original"
        );
        ensure!(
            prepared.claim.sender == *h.state.b.device_id()
                && prepared.claim.receivers == vec![h.state.a.device_id().clone()],
            "intended claim names wrong actual survivor"
        );
    }
    let origin = witness
        .0
        .value
        .origin
        .as_ref()
        .context("origin must be durably attached BEFORE body")?;
    let body = body.clone().device_log()?;
    ensure!(
        origin.event_id == body.obj_id()
            && origin.event_id == receipt.event_id
            && origin.request_id == h.state.uuid
            && origin.body_commitment == producer_source_commitment(&body)?,
        "origin/body actual identity differs"
    );
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "preaccept live material/claims changed"
    );
    let trace = h.p.repo.trace.lock().unwrap().clone();
    let changed=trace.iter().position(|t|matches!(t,Trace::Write(true,event) if matches!(event.as_ref(),GenericKvLogEvent::LocalGuardedPreparationIndex(x) if x.0.value.revision>0))).context("actual CHANGED index replacement required")?;
    let witness_at = trace
        .iter()
        .position(|t| {
            matches!(
                t,
                Trace::Write(false, event) if matches!(event.as_ref(), GenericKvLogEvent::LocalGuardedPreparation(_))
            )
        })
        .context("actual fixed witness insertion required")?;
    ensure!(
        changed < witness_at
            && trace[changed + 1..witness_at]
                .iter()
                .any(|t| matches!(t,Trace::Read(id) if *id==h.state.index_id())),
        "index changed replacement was not read back before payload"
    );
    let origin_at=trace.iter().position(|t|matches!(t,Trace::Write(true,event) if matches!(event.as_ref(),GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.origin.is_some()))).context("actual origin attachment")?;
    let body_at=trace.iter().position(|t|matches!(t,Trace::Write(_,event) if matches!(event.as_ref(),GenericKvLogEvent::DeviceLog(b) if b.obj_id()==receipt.event_id))).context("actual body write")?;
    ensure!(
        witness_at < origin_at
            && origin_at < body_at
            && trace[witness_at + 1..origin_at]
                .iter()
                .any(|t| matches!(t,Trace::Read(id) if *id==h.state.witness_id()))
            && trace[origin_at + 1..body_at]
                .iter()
                .any(|t| matches!(t,Trace::Read(id) if *id==h.state.witness_id())),
        "complete witness/origin was not read back before actual body"
    );
    let current = h.witness().await?;
    private_crypto(&h.state, &current)?;
    let frozen = current.0.value.payload.clone();
    let retry = h.decide().await?;
    ensure!(
        retry.event_id == receipt.event_id && h.witness().await?.0.value.payload == frozen,
        "explicit retry rerandomized/reallocated existing witness"
    );
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id().next())
            .await?
            .is_none(),
        "fixed private payload grew append history"
    );
    Ok(())
}
#[tokio::test]
async fn s02_invalid_later_pass_never_leaves_partial_payload_or_actual_body() -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    // Labelled invalid local input, NOT an actual topology transition.
    h.p.repo
        .inner
        .try_delete(ArtifactId::from(SsWorkflowDescriptor::Distribution(
            SsDistributionId {
                pass_id: h.state.passes[1].clone(),
                receiver: h.state.b.device_id().clone(),
            },
        )))
        .await?;
    let before = h.p.repo.snapshot().await?;
    let r = h.decide().await;
    ensure!(r.is_err(), "invalid later Pass reported Queued");
    h.no_body_or_send().await?;
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "earlier Pass published while later input invalid"
    );
    ensure!(
        h.p.repo
            .inner
            .find_one(h.state.witness_id())
            .await?
            .is_none(),
        "partial packet-bearing witness persisted"
    );
    Ok(())
}
#[tokio::test]
async fn s06_origin_replace_uncertainty_is_not_queued_or_noop_eligible() -> Result<()> {
    let mut results = vec![];
    for fault in [Fault::Before, Fault::Drop, Fault::Readback] {
        results.push(origin_fault(fault).await);
    }
    for r in results {
        r?;
    }
    Ok(())
}
async fn origin_fault(fault: Fault) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = h.p.repo.snapshot().await?;
    h.p.repo.arm(Point::Origin, fault);
    let r = h.decide().await;
    eprintln!(
        "C1b origin {fault:?} hits={}",
        h.p.repo.hits.load(Ordering::SeqCst)
    );
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "origin fault boundary UNREACHED"
    );
    if fault == Fault::Readback {
        ensure!(
            h.p.repo.read_hits.load(Ordering::SeqCst) > 0,
            "origin readback fault UNREACHED"
        );
    }
    static_error(&r)?;
    h.no_body_or_send().await?;
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "origin failure published ordinary packets/claims"
    );
    let w = h.witness().await?;
    private_crypto(&h.state, &w)?;
    let _sync = h.gateway().sync(h.state.b.user()).await;
    h.no_body_or_send().await?;
    Ok(())
}
#[tokio::test]
async fn s07_actual_body_then_phase_failure_retains_same_unknown_identity() -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let before = h.p.repo.snapshot().await?;
    h.p.repo.arm(Point::Queued, Fault::Before);
    let result = h.decide().await;
    eprintln!(
        "C1b queued-phase hits={}",
        h.p.repo.hits.load(Ordering::SeqCst)
    );
    ensure!(
        h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "postbody queued-phase boundary UNREACHED"
    );
    let receipt = result.context("actual body persistence must retain actual receipt")?;
    ensure!(
        receipt.phase == LiveboardOperationPhase::Unknown && receipt.request_id == h.state.uuid,
        "postbody uncertainty lost actual identity"
    );
    let body =
        h.p.repo
            .inner
            .find_one(receipt.event_id.clone())
            .await?
            .context("actual saved body required")?;
    let frozen = h.witness().await?.0.value.payload;
    for _ in 0..2 {
        let r = h.decide().await?;
        ensure!(
            r.event_id == receipt.event_id && r.phase == LiveboardOperationPhase::Unknown,
            "held retry allocated another body"
        );
    }
    h.p.repo.clear();
    let retry = h.decide().await?;
    ensure!(
        retry.event_id == receipt.event_id
            && h.p.repo.inner.find_one(receipt.event_id).await? == Some(body)
            && h.witness().await?.0.value.payload == frozen,
        "repair changed saved body/frozen bytes"
    );
    ensure!(
        ordinary(&h.p.repo.snapshot().await?) == ordinary(&before),
        "postbody private-phase retry published before acceptance"
    );
    Ok(())
}
