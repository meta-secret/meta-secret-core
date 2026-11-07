use super::*;
use meta_secret_core::node::common::model::secret::SecretDistributionType;

fn claim_identity(c: &meta_secret_core::node::common::model::secret::SsClaim) -> Result<String> {
    digest(&(
        &c.id,
        &c.dist_claim_id,
        &c.vault_name,
        &c.sender,
        &c.distribution_type,
        &c.receivers,
    ))
}
pub(super) async fn prerequisites<R: KvLogEventRepo>(h: &harness::Harness<R>) -> Result<Retired> {
    let origin = h
        .case
        .h
        .witness()
        .await?
        .0
        .value
        .origin
        .context("real original Decline origin")?;
    let initial_payload_hash = digest(
        h.case
            .h
            .witness()
            .await?
            .0
            .value
            .payload
            .as_ref()
            .context("actual old FULL before retirement")?,
    )?;
    let initial = h.case.h.p.repo.snapshot().await?;
    let mut existing = HashMap::new();
    for e in initial.values() {
        let claims = match e {
            GenericKvLogEvent::SsDeviceLog(c) => vec![&c.0.value],
            GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect(),
            _ => vec![],
        };
        for c in claims {
            existing.insert(c.id.wasm_id_str(), claim_identity(c)?);
        }
    }
    let first = h.sync().await;
    eprintln!(
        "OPPOSITE prerequisite first_FULL_public_sync_ok={}",
        first.is_ok()
    );
    first
        .context("DEPENDENCY: real first public sync did not return success; rollover UNREACHED")?;
    let retired = technical(h, &origin).await?;
    let repeat = h.sync().await;
    eprintln!(
        "OPPOSITE prerequisite repeated_FULL_public_sync_ok={}",
        repeat.is_ok()
    );
    repeat.context("DEPENDENCY: ordinary repeat failed before explicit rollover")?;
    let state = h.service().get_app_state().await;
    eprintln!(
        "OPPOSITE prerequisite actual_public_GetState_returned={}",
        state.is_ok()
    );
    let state = state.context("DEPENDENCY: actual GetState failed before explicit rollover")?;
    ensure!(
        matches!(state,ApplicationState::Vault(VaultFullInfo::Member(info)) if info.member.vault.members().len()==3),
        "actual GetState did not return three-member state"
    );
    ensure!(
        h.case.h.witness().await? == retired.witness,
        "ordinary sync/GetState automatically altered settled witness"
    );
    let db = h.case.h.p.repo.snapshot().await?;
    ensure!(checks::ordinary_clean(&db,&h.case.h.state.passes[0])
        && db.values().all(|e| !matches!(e,GenericKvLogEvent::DeviceLog(b) if b.obj_id().fqdn==origin.event_id.fqdn && b.obj_id()!=origin.event_id)),"ordinary sync/GetState made another own decision or restored deleted data");
    let mut scrubbed = false;
    let mut unchanged_old_full_writes = 0;
    for w in h.repo.writes.lock().unwrap().iter() {
        if let GenericKvLogEvent::LocalGuardedPreparation(x) = &w.event {
            ensure!(
                x.0.value.origin.as_ref() == Some(&origin)
                    && x.0.value.previous_attempts.is_empty(),
                "automatic new origin/archive before NEW user decision"
            );
            if let Some(payload) = &x.0.value.payload {
                ensure!(
                    !scrubbed && digest(payload)? == initial_payload_hash,
                    "automatic new preparation/FULL resurrection before NEW user decision"
                );
                unchanged_old_full_writes += 1;
            } else {
                scrubbed = true;
            }
        }
        let claims = match &w.event {
            GenericKvLogEvent::SsDeviceLog(c) => vec![&c.0.value],
            GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect(),
            _ => vec![],
        };
        for c in claims {
            if c.sender == *h.case.h.state.b.device_id()
                && c.distribution_type == SecretDistributionType::Split
            {
                ensure!(
                    existing.get(&c.id.wasm_id_str()) == Some(&claim_identity(c)?),
                    "ordinary sync/GetState generated own removal Split"
                );
            }
        }
    }
    ensure!(scrubbed, "actual private purge write UNREACHED");
    eprintln!(
        "OPPOSITE no_auto unchanged_original_FULL_auth_writes={unchanged_old_full_writes} actual_scrub_seen={scrubbed} no_new_origin_payload_archive=true"
    );
    ensure!(
        h.case.observations.sent.lock().unwrap().is_empty(),
        "stale Decline Business source was sent/applied"
    );
    eprintln!(
        "OPPOSITE prerequisite real_Noop_receipt_separate_confirmedTail=true repeated_ordinary_sync=true actual_public_GetState=true no_auto_decision=true"
    );
    Ok(retired)
}
async fn technical<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    origin: &GuardedCommandOrigin,
) -> Result<Retired> {
    let witness = h.case.h.witness().await?;
    ensure!(
        witness.0.value.payload.is_none()
            && witness.0.value.origin.as_ref() == Some(origin)
            && !origin.accepted
            && witness.0.value.claims.is_empty()
            && witness.0.value.packets.is_empty()
            && witness.0.value.previous_attempts.is_empty(),
        "actual retired Decline compact shape differs"
    );
    let t = witness
        .0
        .value
        .transport_noop
        .as_ref()
        .context("actual Noop auth/receipt/tail")?;
    let receipt = t.receipt.as_ref().context("actual durable Noop receipt")?;
    let next = origin.event_id.clone().next();
    t.authorization
        .verify(&h.case.h.state.b.user().device.keys.dsa_pk)?;
    ensure!(
        t.authorization.event_id == origin.event_id
            && t.authorization.actor_id == origin.actor_id
            && t.authorization.client_entry_commitment == origin.body_commitment
            && receipt.record_effect == TransportNoopEffect::NotApplied
            && receipt.authorization_hash == t.authorization.authorization_hash()?
            && t.confirmed_next_tail == Some(next.clone()),
        "actual settlement identity or confirmedTail differs"
    );
    let marker = h
        .case
        .h
        .state
        .server_repo
        .find_one(origin.event_id.clone())
        .await?
        .context("actual server marker N")?;
    let GenericKvLogEvent::DeviceTransportNoop(noop) = &marker else {
        anyhow::bail!("stale N is not actual Noop")
    };
    noop.validate(
        &h.case.h.state.b.user_id(),
        &h.case.h.state.b.user().device.keys.dsa_pk,
    )?;
    ensure!(
        noop.0.value.authorization == t.authorization && noop.receipt() == *receipt,
        "physical server Noop receipt differs"
    );
    let send = {
        let calls = h.wire.noops.lock().unwrap();
        ensure!(
            calls.len() == 1
                && calls[0].durable
                && calls[0].outer_read
                && calls[0].scrubbed
                && calls[0].authorization == t.authorization
                && calls[0].receipt.as_ref() == Some(receipt),
            "actual Noop send lacked private/ordinary purge/auth readback"
        );
        calls[0].order
    };
    ensure!(h.wire.tails.lock().unwrap().iter().any(|tail|tail.order>send && tail.after_receipt
        && tail.response.device_log_tail==Some(next.clone()) && h.repo.reads.lock().unwrap().iter().any(|r|
        r.order>tail.order && matches!(&r.event,GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.transport_noop.as_ref()==Some(t)))),"separate actual postreceipt Tail/OUTER durable confirmation read UNREACHED");
    let body = h
        .physical
        .find_one(origin.event_id.clone())
        .await?
        .context("actual redacted N retained")?;
    ensure!(
        matches!(&body,GenericKvLogEvent::DeviceLog(b) if !checks::action_transfer(&b.0.value,&h.case.h.state.passes[0])),
        "actual old Business N absent/unpurged"
    );
    let (vault, log) = canonical(&h.case).await?;
    ensure!(
        vault.0.value.members().len() == 3
            && vault.0.value.secrets.contains(&h.case.h.state.passes[1])
            && vault
                .0
                .value
                .deleted_secrets
                .contains(&h.case.h.state.passes[0])
            && log
                .0
                .value
                .liveboard_removal_pending
                .iter()
                .filter(|r| r.request_id == h.case.h.state.uuid)
                .count()
                == 1
            && !log
                .0
                .value
                .liveboard_removal_results
                .iter()
                .any(|r| r.request_id == h.case.h.state.uuid),
        "Noop decided the logical UUID or changed topology"
    );
    Ok(Retired {
        witness,
        index: h.case.h.index().await?,
        body_hash: digest(&body)?,
        marker_hash: digest(&marker)?,
    })
}
fn compact(
    old: &LocalGuardedPreparationObject,
) -> Result<meta_secret_core::node::db::events::local_guarded_preparation::GuardedCompactAttempt> {
    let x = &old.0.value;
    Ok(
        meta_secret_core::node::db::events::local_guarded_preparation::GuardedCompactAttempt {
            origin: x.origin.clone().context("old origin")?,
            transport_noop: x.transport_noop.clone(),
            phase: x.phase,
            base_vault: x.base_vault.clone(),
            requester_id: x.requester_id.clone(),
            target_id: x.target_id.clone(),
            survivors: x.survivors.clone(),
            claims: x.claims.clone(),
            packets: x.packets.clone(),
        },
    )
}
pub(super) async fn queue<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    retired: Retired,
) -> Result<Queued> {
    let start = h.repo.writes.lock().unwrap().len();
    let explicit = h.gateway.decide_remove_member(h.case.h.state.input()).await;
    eprintln!(
        "OPPOSITE explicit_Accept_after_all_real_prerequisites queued={}",
        explicit.is_ok()
    );
    let receipt=explicit.context("ROLLOVER: NEW explicit opposite Accept must queue N+1 after actual settled Noop and returned GetState")?;
    let witness = h.case.h.witness().await?;
    crypto::accepted(h, &witness).await?;
    let old = retired
        .witness
        .0
        .value
        .origin
        .as_ref()
        .context("old compact origin")?;
    let next = old.event_id.clone().next();
    let current = witness
        .0
        .value
        .origin
        .as_ref()
        .context("actual N+1 origin")?;
    ensure!(
        receipt.event_id == next
            && current.event_id == next
            && current.accepted
            && current.request_id == h.case.h.state.uuid
            && current.business_signature.is_none()
            && witness.0.value.previous_attempts == vec![compact(&retired.witness)?]
            && witness.0.value.transport_noop.is_none(),
        "explicit opposite did not preserve exact N archive/allocate unique next identity"
    );
    let index = h.case.h.index().await?;
    ensure!(
        index.0.value.request_ids == retired.index.0.value.request_ids
            && index
                .0
                .value
                .request_ids
                .iter()
                .filter(|id| **id == h.case.h.state.uuid)
                .count()
                == 1
            && index.0.value.revision == retired.index.0.value.revision + 1,
        "rollover did not probe EXISTING index exact revision+1"
    );
    let body = h
        .physical
        .find_one(next.clone())
        .await?
        .context("actual N+1 body")?
        .device_log()?;
    ensure!(
        producer_source_commitment(&body)? == current.body_commitment
            && digest(
                &h.physical
                    .find_one(old.event_id.clone())
                    .await?
                    .context("old N body")?
            )? == retired.body_hash
            && h.physical.find_one(next.clone().next()).await?.is_none()
            && h.physical
                .find_one(h.case.h.state.witness_id().next())
                .await?
                .is_none(),
        "origin/body/oldredaction changed or extra body/history allocated"
    );
    let payload = {
        let writes = h.repo.writes.lock().unwrap();
        let writes = &writes[start..];
        let full=writes.iter().find(|w|matches!(&w.event,GenericKvLogEvent::LocalGuardedPreparation(x) if x.0.value.payload.is_some())).context("actual atomic FULL install")?;
        ensure!(
            full.replace
                && full.event.obj_id() == h.case.h.state.witness_id()
                && matches!(&full.event,GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.phase==GuardedPreparationPhase::Staged && w.0.value.origin.is_none() && w.0.value.previous_attempts==vec![compact(&retired.witness)?]),
            "first FULL install is not atomic Staged+one compact N"
        );
        let probe = writes
            .iter()
            .find(
                |w| matches!(&w.event,GenericKvLogEvent::LocalGuardedPreparationIndex(x) if x==&index),
            )
            .context("changed existing index replacement")?;
        let reads = h.repo.reads.lock().unwrap();
        ensure!(
            probe.replace
                && probe.order < full.order
                && reads.iter().any(|r| r.order > probe.order
                    && r.order < full.order
                    && r.event == index.clone().to_generic())
                && writes
                    .iter()
                    .filter(|w| matches!(
                        &w.event,
                        GenericKvLogEvent::LocalGuardedPreparationIndex(_)
                    ))
                    .count()
                    == 1,
            "index probe was not existing-only replace/exact readback before FULL"
        );
        let first_origin = writes
            .iter()
            .find(|w| {
                matches!(
                    &w.event, GenericKvLogEvent::LocalGuardedPreparation(x)
                    if x.obj_id() == h.case.h.state.witness_id() && x.0.value.origin.is_some()
                )
            })
            .context("FIRST actual new origin attach")?;
        ensure!(
            full.order < first_origin.order
                && reads.iter().any(|r| r.order > full.order
                    && r.order < first_origin.order
                    && r.event == full.event),
            "WHOLE intended FULL was not read back through OUTER production repo before FIRST origin attach"
        );
        let body_write = writes
            .iter()
            .find(|w| w.event.obj_id() == next)
            .context("actual N+1 append")?;
        ensure!(writes.iter().any(|w|w.order<body_write.order && matches!(&w.event,GenericKvLogEvent::LocalGuardedPreparation(x) if x.0.value.origin.as_ref()==Some(current))
            && reads.iter().any(|r|r.order>w.order && r.order<body_write.order && r.event==w.event)),"actual origin not read back before N+1 body");
        let payload = witness.0.value.payload.as_ref().unwrap();
        ensure!(writes.iter().all(|w|!matches!(&w.event,GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))|GenericKvLogEvent::SsDeviceLog(_))
            && !matches!(&w.event,GenericKvLogEvent::SsLog(log) if payload.passes.iter().any(|p|log.0.value.claims.contains_key(&p.claim.id)))),"explicit preparation published before canonical approval");
        payload
    };
    let packet_hashes = payload
        .passes
        .iter()
        .flat_map(|p| p.distributions.iter())
        .map(|p| Ok((p.obj_id(), digest(p)?)))
        .collect::<Result<HashMap<_, _>>>()?;
    let registry = h
        .physical
        .find_one(ArtifactId::from(LocalRemovalEnvelopeDescriptor(
            h.case.h.state.b.user_id(),
        )))
        .await?
        .context("actual envelope registry")?;
    eprintln!(
        "OPPOSITE actual_N_plus_1=true p1_only_crypto=true old_archive_exact=true existing_index_probe_exact=true origin_before_body=true no_preaccept_publication=true"
    );
    Ok(Queued {
        retired,
        witness_hash: digest(&witness)?,
        payload_hash: digest(payload)?,
        index_hash: digest(&index)?,
        registry_hash: digest(&registry)?,
        body_hash: digest(&body)?,
        packet_hashes,
        event_id: next,
    })
}
pub(super) async fn retry_and_admit<R: KvLogEventRepo>(
    h: &harness::Harness<R>,
    queued: &Queued,
) -> Result<()> {
    let before = h.repo.writes.lock().unwrap().len();
    let retry = h
        .gateway
        .decide_remove_member(h.case.h.state.input())
        .await?;
    let w = h.case.h.witness().await?;
    let payload =
        w.0.value
            .payload
            .as_ref()
            .context("persisted current FULL")?;
    ensure!(
        retry.event_id == queued.event_id
            && digest(&w)? == queued.witness_hash
            && digest(payload)? == queued.payload_hash
            && digest(&h.case.h.index().await?)? == queued.index_hash
            && digest(
                &h.physical
                    .find_one(queued.event_id.clone())
                    .await?
                    .context("durable N+1")?
                    .device_log()?
            )? == queued.body_hash
            && h.repo.writes.lock().unwrap()[before..]
                .iter()
                .all(|x| !matches!(&x.event, GenericKvLogEvent::LocalGuardedPreparationIndex(_))),
        "same-Accept retry rerandomized/forked/probed index again"
    );
    for p in payload.passes.iter().flat_map(|p| p.distributions.iter()) {
        ensure!(
            queued.packet_hashes.get(&p.obj_id()) == Some(&digest(p)?),
            "persisted frozen packet commitment differs"
        );
    }
    crypto::accepted(h, &w).await?;
    let frozen = h.expect_current().await?;
    let sync = h.sync().await;
    let m = super::super::super::proof::measure(&h.case).await?;
    m.print("OPPOSITE actual N+1 admission", &sync);
    ensure!(
        m.sent == 1 && m.signature && m.server_applied && m.local_applied && m.raw_ack,
        "real ServerApp N+1 guarded admission/Applied source UNREACHED"
    );
    sync?;
    super::super::super::proof::published(&h.case, &frozen).await?;
    crypto::published(h, &frozen).await?;
    let old = queued.retired.witness.0.value.origin.as_ref().unwrap();
    ensure!(
        digest(
            &h.case
                .h
                .state
                .server_repo
                .find_one(old.event_id.clone())
                .await?
                .context("old N marker")?
        )? == queued.retired.marker_hash
            && h.case.h.witness().await?.0.value.previous_attempts
                == vec![compact(&queued.retired.witness)?],
        "N NotApplied/archive changed during actual N+1 Business admission"
    );
    let (vault, _) = canonical(&h.case).await?;
    ensure!(
        vault.0.value.members().len() == 2
            && vault.0.value.secrets.contains(&h.case.h.state.passes[1])
            && vault
                .0
                .value
                .deleted_secrets
                .contains(&h.case.h.state.passes[0]),
        "actual admission lost p1/p0 tombstone"
    );
    eprintln!(
        "OPPOSITE real_ServerApp_N_plus_1_Applied=true separate_frozen_publication_readbacks=true actual_A_and_B_K1_recovery=true old_N_NotApplied_immutable=true"
    );
    Ok(())
}
