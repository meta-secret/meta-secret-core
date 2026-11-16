//! Real normal Split persistence, with an independently completed Recovery companion.
mod repo;
mod terminal;

use super::*;
use meta_secret_core::node::{
    common::model::secret::{SecretDistributionType, SsClaim, SsDistributionStatus, SsLogData},
    db::{
        descriptors::shared_secret_descriptor::{
            SsDeviceLogDescriptor, SsLogDescriptor, SsWorkflowDescriptor,
        },
        events::{
            shared_secret_event::SsLogObject, vault::liveboard_metadata::RecoveryDecisionRecord,
        },
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};

type Case = fixture::Case<repo::ClaimRepo<InMemKvLogEventRepo>>;
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    DeviceLostReply,
    LogReadback,
    DeviceBefore,
    LogBefore,
}
#[derive(Clone)]
struct Terminal {
    claim: SsClaim,
    result: RecoveryDecisionRecord,
}
impl Terminal {
    fn preserved(&self, data: &SsLogData) -> bool {
        data.claims.get(&self.claim.id) == Some(&self.claim)
            && data.liveboard_recovery_results.get(&self.claim.id) == Some(&self.result)
    }
}

#[tokio::test]
async fn actual_split_device_log_durable_lost_reply_resolves_exact_publisher_readback() -> Result<()>
{
    run(Fault::DeviceLostReply).await
}
#[tokio::test]
async fn actual_split_log_production_readback_fault_reuses_nonempty_keys_and_preserves_terminal_recovery()
-> Result<()> {
    run(Fault::LogReadback).await
}

async fn records(
    case: &Case,
    expected: &GuardedPreparedPass,
) -> Result<(SsDeviceLogObject, SsLogObject)> {
    let repo = &case.h.p.repo.inner.inner;
    let device = repo
        .device
        .lock()
        .unwrap()
        .clone()
        .context("actual nonempty durable SsDeviceLog captured")?;
    let log = repo
        .log
        .lock()
        .unwrap()
        .clone()
        .context("actual nonempty durable SsLog captured")?;
    ensure!(
        repo.inner.find_one(device.obj_id()).await? == Some(device.clone().to_generic()),
        "captured SsDeviceLog ID is not an actual durable record"
    );
    let actual_log = repo
        .inner
        .find_one(log.obj_id())
        .await?
        .context("actual durable captured SsLog key")?
        .ss_log()?;
    ensure!(
        actual_log.key() == log.key(),
        "captured actual SsLog key/type changed"
    );
    let key = KvKey::from(SsLogDescriptor::from(case.h.state.b.vault_name.clone()));
    let claim = actual_log
        .0
        .value
        .claims
        .get(&expected.claim.id)
        .context("actual new Split at captured SsLog key")?;
    let mut stable = claim.clone();
    stable.status = expected.claim.status.clone();
    stable.client_status = expected.claim.client_status.clone();
    ensure!(
        device.0.value == expected.claim
            && device.obj_id().fqdn
                == ArtifactId::from(SsDeviceLogDescriptor::from(
                    case.h.state.b.device_id().clone()
                ))
                .fqdn
            && log.obj_id().fqdn == key.obj_id.fqdn
            && log.key().obj_desc == key.obj_desc
            && stable == expected.claim
            && claim.distribution_type == SecretDistributionType::Split,
        "durable intended claim keys/type/Vault/generation differ"
    );
    Ok((device, log))
}

async fn run(fault: Fault) -> Result<()> {
    let repo = Arc::new(repo::ClaimRepo::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let case = fixture::Case::new(repo.clone()).await?;
    let terminal = if fault == Fault::LogReadback {
        let snapshot = terminal::create(&case).await?;
        *repo.terminal.lock().unwrap() = Some(snapshot.clone());
        Some(snapshot)
    } else {
        None
    };
    let (receipt, frozen) = case.prepare(true).await?;
    super::super::stage::private_crypto(&case.h.state, &case.h.witness().await?)?;
    let first = frozen
        .passes
        .first()
        .context("actual first intended prepared Pass")?;
    let body = repo
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual original source")?;
    let before = repo.inner.get_db().await;
    repo.arm(
        case.observations.expected()?,
        fault,
        case.observations.clone(),
    );
    let outcome = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print(&format!("claim {fault:?} first attempt"), &outcome);
    let device_reads = repo.device_reads.load(Ordering::SeqCst);
    let log_reads = repo.log_reads.load(Ordering::SeqCst);
    let hits = repo.fault_hits.load(Ordering::SeqCst);
    let partial = case.h.witness().await?;
    eprintln!(
        "C2 claim {fault:?} actual_fault_hits={hits} device_write_hits={} log_write_hits={} device_matching_reads={device_reads} log_matching_reads={log_reads} observer_log_first_forwarded={} publisher_log_second_faulted={} compact_allocated_pairs={} first_sync_ok={}",
        repo.device_writes.load(Ordering::SeqCst),
        repo.log_writes.load(Ordering::SeqCst),
        repo.observer_log_forwarded.load(Ordering::SeqCst),
        repo.publisher_read_hits.load(Ordering::SeqCst),
        partial
            .0
            .value
            .claims
            .iter()
            .filter(|c| c.ss_device_event_id.is_some() && c.ss_log_event_id.is_some())
            .count(),
        outcome.is_ok()
    );
    ensure!(
        hits == 1,
        "actual intended post-LOCALApplied claim fault UNREACHED/repeated"
    );
    proof::common(&measured)?;
    let (device, log) = records(&case, first).await?;
    let signature = partial
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("actual original durable signature")?;
    match fault {
        Fault::DeviceLostReply => {
            ensure!(
                outcome.is_ok()
                    && device_reads > 0
                    && repo.first_device_read_exact.load(Ordering::SeqCst)
                    && repo.device_reads_before_log.load(Ordering::SeqCst) > 0,
                "actual publisher did not resolve exact durable SsDeviceLog LostReply"
            );
            eprintln!(
                "C2 claim LostReply FIRSTactualDeviceFind exact=true BEFOREactualSsLogSave=true observerFindSkippedBySaveErr=true"
            );
            proof::published(&case, &frozen).await?;
        }
        Fault::LogReadback => {
            ensure!(
                outcome.is_err()
                    && repo.observer_log_forwarded.load(Ordering::SeqCst) == 1
                    && repo.publisher_read_hits.load(Ordering::SeqCst) == 1
                    && log_reads == 2,
                "fault did not reach SECOND matching Find in actual claim publisher"
            );
            ensure!(
                partial.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
                    && partial.0.value.payload.as_ref() == Some(&frozen)
                    && partial
                        .0
                        .value
                        .claims
                        .iter()
                        .find(|c| c.pass_id == first.pass_id)
                        .is_some_and(|c| c.ss_device_event_id.is_none()
                            && c.ss_log_event_id.is_none()
                            && !c.handed_off),
                "uncertain claim publication completed/scrubbed or invented compact allocated IDs"
            );
            super::super::stage::private_crypto(&case.h.state, &partial)?;
            eprintln!(
                "C2 claim LogReadback actual durable BOTH-key proof=true compact_keys_still_none=true full_stage=true"
            );
        }
        Fault::DeviceBefore | Fault::LogBefore => {
            anyhow::bail!("BEFORE-save cases require their independent real lifecycle runner");
        }
    }
    repo.armed.store(false, Ordering::SeqCst);
    let retry = case.gateway().sync(case.h.state.b.user()).await;
    let final_measure = proof::measure(&case).await?;
    final_measure.print(&format!("claim {fault:?} ordinary retry"), &retry);
    proof::common(&final_measure)?;
    retry?;
    proof::published(&case, &frozen).await?;
    let completed = case.h.witness().await?;
    let ownership = completed
        .0
        .value
        .claims
        .iter()
        .find(|c| c.pass_id == first.pass_id)
        .context("actual intended ownership after retry")?;
    ensure!(
        ownership.ss_device_event_id.as_ref() == Some(&device.obj_id())
            && ownership.ss_log_event_id.as_ref() == Some(&log.obj_id())
            && ownership.claim == first.claim,
        "ordinary retry reallocated actual NONEMPTY claim keys/generation"
    );
    records(&case, first).await?;
    let after = repo.inner.get_db().await;
    let matching_device = after
        .values()
        .filter(
            |x| matches!(x, GenericKvLogEvent::SsDeviceLog(d) if d.0.value.id == first.claim.id),
        )
        .count();
    let matching_log: Vec<_> = after
        .values()
        .filter_map(|x| match x {
            GenericKvLogEvent::SsLog(l) if l.0.value.claims.contains_key(&first.claim.id) => {
                Some(l)
            }
            _ => None,
        })
        .collect();
    ensure!(
        matching_device == 1
            && matching_log.iter().any(|x| x.obj_id() == log.obj_id())
            && repo.device_writes.load(Ordering::SeqCst) == 1
            && repo.log_writes.load(Ordering::SeqCst) == 1,
        "retry duplicated actual intended claim allocation/write"
    );
    let action = case.observations.action().context("actual signed source")?;
    let origin = completed
        .0
        .value
        .origin
        .as_ref()
        .context("actual retained origin")?;
    ensure!(
        origin.business_signature.as_ref() == Some(&signature)
            && signature.signed_action_hash == proof::hash(&action)?
            && origin.request_id == receipt.request_id
            && origin.event_id == receipt.event_id
            && repo.inner.find_one(receipt.event_id.clone()).await? == Some(body),
        "ordinary claim retry changed original source/UUID/signature"
    );
    let own_bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, x)| {
                id.fqdn == receipt.event_id.fqdn && matches!(x, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, x)| (id.clone(), x.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        own_bodies(&after) == own_bodies(&before),
        "claim retry queued another business body"
    );
    if let Some(snapshot) = terminal {
        let writes_count = {
            let writes = repo.terminal_writes.lock().unwrap();
            ensure!(
                !writes.is_empty() && writes.iter().all(|(_, preserved)| *preserved),
                "a normal new SsLog write reset the actual terminal Recovery claim/result"
            );
            writes.len()
        };
        terminal::assert_final(&case, &snapshot).await?;
        eprintln!(
            "C2 claim terminal Recovery whole_claim+first_responder+sourceproof preserved in ALL{writes_count} SsLog writes and FINAL local/server"
        );
    }
    eprintln!(
        "C2 claim {fault:?} retry_ok=true actual_nonempty_keys_reused=true one_device_allocation=true frozen_generation_cipher=true"
    );
    Ok(())
}

#[tokio::test]
async fn actual_split_device_log_before_save_rejection_retries_same_frozen_source() -> Result<()> {
    run_before(Fault::DeviceBefore).await
}

#[tokio::test]
async fn actual_split_log_before_save_rejection_reuses_durable_device_key() -> Result<()> {
    run_before(Fault::LogBefore).await
}

async fn run_before(fault: Fault) -> Result<()> {
    ensure!(matches!(fault, Fault::DeviceBefore | Fault::LogBefore));
    let repo = Arc::new(repo::ClaimRepo::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let case = fixture::Case::new(repo.clone()).await?;
    let terminal = terminal::create(&case).await?;
    *repo.terminal.lock().unwrap() = Some(terminal.clone());
    let (receipt, frozen) = case.prepare(true).await?;
    let first = frozen.passes.first().context("actual first frozen Pass")?;
    let before = repo.inner.get_db().await;
    let body = before
        .get(&receipt.event_id)
        .cloned()
        .context("actual queued source")?;
    let prepared = case.h.witness().await?;
    super::super::stage::private_crypto(&case.h.state, &prepared)?;
    repo.arm(
        case.observations.expected()?,
        fault,
        case.observations.clone(),
    );

    let outcome = case.gateway().sync(case.h.state.b.user()).await;
    // Freeze the production-only ledger before any test-side witness/crypto reads.
    let first_attempts = repo.attempts.lock().unwrap().clone();
    let first_reads = repo.reads.lock().unwrap().clone();
    let rejected = repo
        .rejected
        .lock()
        .unwrap()
        .clone()
        .context("actual BEFORE-save hook")?;
    let measured = proof::measure(&case).await?;
    measured.print(&format!("claim {fault:?} BEFORE-save"), &outcome);
    proof::common(&measured)?;
    ensure!(
        outcome.is_err() && repo.fault_hits.load(Ordering::SeqCst) == 1,
        "BEFORE-save fault was not reached exactly once or falsely completed"
    );
    ensure!(
        !format!("{outcome:#?}").contains("PRIVATE_SENTINEL")
            && !serde_json::to_string(&receipt)?.contains("PRIVATE_SENTINEL"),
        "private backend sentinel escaped the production/public result"
    );
    let failed = first_attempts
        .iter()
        .find(|a| a.returned == rejected.order)
        .context("ALL-attempt ledger contains the actual rejected write")?;
    ensure!(
        failed.order < failed.returned
            && !failed.saved
            && !failed.replace
            && failed.event == rejected.event
            && rejected.prior.is_none()
            && repo
                .inner
                .find_one(rejected.event.obj_id())
                .await?
                .is_none(),
        "BEFORE-save Reject changed the actual selected physical record"
    );
    ensure!(
        first_reads.iter().any(|r| r.order > rejected.order
            && r.success
            && r.returned > r.order
            && r.id == rejected.event.obj_id()
            && r.actual.is_none()),
        "actual publisher did not observe the missing rejected record"
    );
    let partial = case.h.witness().await?;
    let claim = partial
        .0
        .value
        .claims
        .iter()
        .find(|c| c.pass_id == first.pass_id)
        .context("actual selected compact claim")?;
    ensure!(
        partial.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
            && partial.0.value.payload.as_ref() == Some(&frozen)
            && claim.claim == first.claim
            && claim.ss_device_event_id.is_none()
            && claim.ss_log_event_id.is_none()
            && !claim.handed_off,
        "unverified claim publication was falsely completed/scrubbed/allocated"
    );
    super::super::stage::private_crypto(&case.h.state, &partial)?;
    let durable_device = repo.device.lock().unwrap().clone();
    match fault {
        Fault::DeviceBefore => ensure!(
            durable_device.is_none()
                && repo.device_writes.load(Ordering::SeqCst) == 0
                && repo.log_writes.load(Ordering::SeqCst) == 0,
            "rejected SsDeviceLog was falsely called durable"
        ),
        Fault::LogBefore => {
            let device = durable_device
                .as_ref()
                .context("actual prior durable SsDeviceLog")?;
            ensure!(
                device.0.value == first.claim
                    && repo.inner.find_one(device.obj_id()).await?
                        == Some(device.clone().to_generic())
                    && repo.device_writes.load(Ordering::SeqCst) == 1
                    && repo.log_writes.load(Ordering::SeqCst) == 0,
                "SsLog rejection lost/reallocated the actual prior device key"
            );
        }
        _ => unreachable!("independent BEFORE-save runner"),
    }
    let signature = partial
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("original actual durable business signature")?;
    repo.armed.store(false, Ordering::SeqCst);
    let retry = case.gateway().sync(case.h.state.b.user()).await;
    let retry_attempts = repo.attempts.lock().unwrap().clone();
    let retry_reads = repo.reads.lock().unwrap().clone();
    let measured = proof::measure(&case).await?;
    measured.print(&format!("claim {fault:?} ordinary retry"), &retry);
    proof::common(&measured)?;
    retry?;
    proof::published(&case, &frozen).await?;
    let (device, log) = records(&case, first).await?;
    if let Some(old) = durable_device {
        ensure!(
            device == old,
            "ordinary retry reallocated a real nonempty device key"
        );
    }
    let selected = if fault == Fault::DeviceBefore {
        device.clone().to_generic()
    } else {
        log.clone().to_generic()
    };
    ensure!(
        selected == *rejected.event,
        "ordinary retry changed the rejected intended object/key/generation"
    );
    let completion = retry_attempts
        .iter()
        .find(|a| {
            matches!(a.event.as_ref(), GenericKvLogEvent::LocalGuardedPreparation(w)
            if w.obj_id() == case.h.state.witness_id()
                && w.0.value.claims.iter().any(|c| c.pass_id == first.pass_id
                    && c.ss_device_event_id.as_ref() == Some(&device.obj_id())
                    && c.ss_log_event_id.as_ref() == Some(&log.obj_id()) && c.handed_off))
        })
        .context("actual compact completion write")?;
    ensure!(
        completion.saved,
        "compact completion was not actually persisted"
    );
    for intended in [device.clone().to_generic(), log.clone().to_generic()] {
        let saved = retry_attempts
            .iter()
            .find(|a| a.saved && *a.event == intended)
            .context("actual exact claim write before completion")?;
        let reads: Vec<_> = retry_reads
            .iter()
            .filter(|r| {
                r.success
                    && r.order > saved.returned
                    && r.returned < completion.order
                    && r.id == intended.obj_id()
                    && r.actual.as_deref() == Some(&intended)
            })
            .collect();
        // The first exact Find is the existing success observer; the following
        // exact Find is the real claim publisher's independent readback.
        ensure!(
            reads.len() >= 2,
            "publisher whole-object readback did not precede compact completion"
        );
    }
    ensure!(
        repo.device_writes.load(Ordering::SeqCst) == 1
            && repo.log_writes.load(Ordering::SeqCst) == 1
            && repo.fault_hits.load(Ordering::SeqCst) == 1,
        "ordinary retry duplicated actual claim allocation or repeated the cleared fault"
    );
    for attempt in &retry_attempts {
        let claims: Vec<_> = match attempt.event.as_ref() {
            GenericKvLogEvent::SsDeviceLog(d) => vec![&d.0.value],
            GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect(),
            _ => vec![],
        };
        for actual in claims {
            if actual.sender == *case.h.state.b.device_id()
                && actual.vault_name == case.h.state.b.vault_name
                && actual.dist_claim_id.pass_id == first.pass_id
                && actual.distribution_type == SecretDistributionType::Split
            {
                let mut stable = actual.clone();
                stable.status = first.claim.status.clone();
                stable.client_status = first.claim.client_status.clone();
                ensure!(
                    stable == first.claim,
                    "ALL-attempt ledger found another selected Pass generation"
                );
            }
        }
    }
    let completed = case.h.witness().await?;
    ensure!(
        completed.0.value.previous_attempts == prepared.0.value.previous_attempts
            && completed.0.value.previous_attempts == partial.0.value.previous_attempts,
        "first claim completion changed the original compact history"
    );
    let origin = completed
        .0
        .value
        .origin
        .as_ref()
        .context("retained original origin")?;
    let action = case
        .observations
        .action()
        .context("original signed action")?;
    action.verify(&case.h.state.b.device().keys.dsa_pk)?;
    ensure!(
        origin.business_signature.as_ref() == Some(&signature)
            && signature.signed_action_hash == proof::hash(&action)?
            && origin.request_id == receipt.request_id
            && origin.event_id == receipt.event_id
            && action.nonce == receipt.event_id.id.curr as u64,
        "ordinary retry changed actual source/UUID/nonce/signature"
    );
    let repeat = case.gateway().sync(case.h.state.b.user()).await;
    let repeated_attempts = repo.attempts.lock().unwrap().clone();
    exact_claim_attempts(
        repeated_attempts.iter().map(|a| a.event.as_ref()),
        &before,
        &frozen,
        &completed,
        &case.h.state.b.user_id(),
    )?;
    let measured = proof::measure(&case).await?;
    proof::common(&measured)?;
    repeat?;
    proof::published(&case, &frozen).await?;
    ensure!(
        case.h.witness().await? == completed
            && repo.device_writes.load(Ordering::SeqCst) == 1
            && repo.log_writes.load(Ordering::SeqCst) == 1,
        "ordinary repeat changed compact/history or duplicated intended claim keys"
    );
    let after = repo.inner.get_db().await;
    let bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, event)| {
                id.fqdn == receipt.event_id.fqdn && matches!(event, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, event)| (id.clone(), event.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        bodies(&after) == bodies(&before) && after.get(&receipt.event_id) == Some(&body),
        "ordinary retry/repeat appended or changed the actual business source"
    );
    let writes = repo.terminal_writes.lock().unwrap().clone();
    ensure!(
        !writes.is_empty() && writes.iter().all(|(_, preserved)| *preserved),
        "new claim write lost the real terminal Recovery companion"
    );
    terminal::assert_final(&case, &terminal).await?;
    eprintln!(
        "C2 claim {fault:?} real_before_hit=1 physical_no_effect=true ordinary_retry_repeat_ok=true exact_source_keys_cipher=true publisher_readbacks_before_completion=true terminal_preserved=true"
    );
    Ok(())
}

/// Validate the complete after-repeat ALL-attempt ledger, not only the IDs
/// recognized by the fault counters. Mutable delivery/client status is allowed;
/// immutable binding and every local device-stream allocation remain exact.
pub(super) fn exact_claim_attempts<'a>(
    events: impl IntoIterator<Item = &'a GenericKvLogEvent>,
    before: &super::super::super::fixture::Snapshot,
    frozen: &GuardedPreparationPayload,
    completed: &LocalGuardedPreparationObject,
    owner: &UserId,
) -> Result<()> {
    let scoped = |claim: &SsClaim| {
        claim.sender == owner.device_id
            && claim.vault_name == owner.vault_name
            && frozen
                .passes
                .iter()
                .any(|p| p.pass_id == claim.dist_claim_id.pass_id)
    };
    let mut allowed: Vec<(SsClaim, Option<ArtifactId>)> = vec![];
    for pass in &frozen.passes {
        let ownership = completed
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == pass.pass_id && c.claim == pass.claim)
            .context("completed exact frozen claim binding")?;
        allowed.push((
            pass.claim.clone(),
            Some(
                ownership
                    .ss_device_event_id
                    .clone()
                    .context("actual frozen claim device-stream allocation")?,
            ),
        ));
    }
    // Only actual pre-operation records/prior compact metadata authorize an
    // older claim; final/new writes cannot enlarge this identity allowlist.
    for event in before.values() {
        match event {
            GenericKvLogEvent::SsDeviceLog(d) if scoped(&d.0.value) => {
                allowed.push((d.0.value.clone(), Some(d.obj_id())))
            }
            GenericKvLogEvent::SsLog(log) => {
                for claim in log.0.value.claims.values().filter(|c| scoped(c)) {
                    allowed.push((claim.clone(), None));
                }
            }
            GenericKvLogEvent::LocalGuardedPreparation(w) if w.0.value.owner == *owner => {
                for claim in
                    w.0.value
                        .previous_attempts
                        .iter()
                        .flat_map(|p| &p.claims)
                        .filter(|c| scoped(&c.claim))
                {
                    allowed.push((claim.claim.clone(), claim.ss_device_event_id.clone()));
                }
            }
            _ => {}
        }
    }
    let same = |actual: &SsClaim, intended: &SsClaim| {
        let mut stable = actual.clone();
        stable.status = intended.status.clone();
        stable.client_status = intended.client_status.clone();
        stable == *intended
    };
    let check = |actual: &SsClaim,
                 device_key: Option<&ArtifactId>,
                 map_collision: bool|
     -> Result<bool> {
        // Colliding IDs/physical keys cannot escape by changing owner/Vault/Pass.
        let relevant = scoped(actual)
            || map_collision
            || allowed.iter().any(|(c, key)| {
                c.id == actual.id
                    || c.dist_claim_id == actual.dist_claim_id
                    || device_key.is_some_and(|id| key.as_ref() == Some(id))
            });
        if relevant {
            ensure!(
                allowed.iter().any(|(c, _)| same(actual, c)),
                "ALL attempts contain an unapproved immutable claim binding/generation"
            );
            if let Some(id) = device_key {
                ensure!(
                    allowed
                        .iter()
                        .any(|(c, key)| same(actual, c) && key.as_ref() == Some(id)),
                    "ALL attempts duplicate/reallocate a scoped device claim at another physical ID"
                );
            }
        }
        Ok(relevant)
    };
    for event in events {
        match event {
            GenericKvLogEvent::SsDeviceLog(d) => {
                check(&d.0.value, Some(&d.obj_id()), false)?;
            }
            GenericKvLogEvent::SsLog(log) => {
                for (key, claim) in &log.0.value.claims {
                    let collision = allowed.iter().any(|(known, _)| known.id == *key);
                    if check(claim, None, collision)? {
                        ensure!(
                            key == &claim.id,
                            "SsLog map key differs from the whole scoped claim identity"
                        );
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
