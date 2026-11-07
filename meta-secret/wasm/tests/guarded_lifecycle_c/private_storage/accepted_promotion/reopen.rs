use super::*;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

struct Interrupted {
    state: State,
    keys: Vec<ArtifactId>,
    observations: Arc<Observations>,
    frozen: GuardedPreparationPayload,
    body: GenericKvLogEvent,
    signature: GuardedBusinessSignature,
    allocated_claims: Vec<GuardedClaimOwnership>,
}
async fn interrupt(inner: Arc<SqlIteRepo>) -> Result<Interrupted> {
    let case = fixture::Case::new(inner).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    super::super::stage::private_crypto(&case.h.state, &case.h.witness().await?)?;
    let body = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(receipt.event_id)
        .await?
        .context("actual original B body")?;
    case.observations.fail_second.store(true, Ordering::SeqCst);
    let gateway = case.gateway();
    let sync = gateway.sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print("P4 SQLite second-live-write interruption", &sync);
    // No late fault claim is made when current production has no promoter.
    ensure!(
        measured.faults == 1,
        "C2 P4 second actual post-LOCAL-Applied live-write fault UNREACHED"
    );
    proof::common(&measured)?;
    ensure!(sync.is_err(), "C2 partial publication falsely completed");
    let witness = case.h.witness().await?;
    ensure!(
        witness.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
            && witness.0.value.payload.as_ref() == Some(&frozen),
        "C2 partial publication prematurely scrubbed/full-completed"
    );
    let writes: Vec<_> = case
        .observations
        .live
        .lock()
        .unwrap()
        .iter()
        .filter(|w| proof::intended(&case.observations.expected().unwrap(), &w.packet))
        .cloned()
        .collect();
    ensure!(
        writes.len() >= 2
            && writes[0].durable
            && !writes[1].durable
            && writes
                .iter()
                .all(|w| w.local_applied && w.promoting_readback),
        "C2 actual second-write boundary differs"
    );
    let first = &writes[0].packet;
    ensure!(
        frozen
            .passes
            .iter()
            .any(|p| p.distributions.contains(first))
            && case.h.p.repo.inner.inner.find_one(first.obj_id()).await?
                == Some(first.clone().to_generic()),
        "C2 first persisted output changed its frozen bytes"
    );
    let signature = witness
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("actual original durable signature")?;
    let keys = case.h.p.repo.snapshot().await?.into_keys().collect();
    let allocated_claims = witness.0.value.claims.clone();
    drop(gateway);
    let fixture::Case {
        h,
        wire,
        observations,
    } = case;
    drop(wire);
    let H { p, state, wire } = h;
    drop(wire);
    drop(p); // Every local repo/observer/SQLite Arc is now released.
    Ok(Interrupted {
        state,
        keys,
        observations,
        frozen,
        body,
        signature,
        allocated_claims,
    })
}

#[tokio::test]
async fn p4_second_real_live_publication_failure_sqlite_reopen_reuses_original_source_and_frozen_packets()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("c2-live-promotion.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let inner = Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    });
    let weak = Arc::downgrade(&inner);
    let saved = interrupt(inner).await?;
    ensure!(
        weak.upgrade().is_none(),
        "C2 local SQLite backend never actually closed"
    );
    eprintln!("C2 P4 actual ALL-local-Arc close verified; reopening SAME SQLite file");
    saved
        .observations
        .fail_second
        .store(false, Ordering::SeqCst);
    let observed = Arc::new(repo::Observed::new(
        Arc::new(SqlIteRepo { conn_url: url }),
        saved.observations.clone(),
    ));
    let existing = Arc::new(ExistingRepo::new(
        observed,
        saved.state.b.user_id(),
        saved.state.uuid.clone(),
    ));
    existing.remember(saved.keys);
    let h = H::open(existing, saved.state)?;
    ensure!(
        h.p.repo.inner.inner.find_one(saved.body.obj_id()).await? == Some(saved.body.clone()),
        "C2 reopened source differs from actual original body"
    );
    let reopened = h.witness().await?;
    ensure!(
        reopened.0.value.payload.as_ref() == Some(&saved.frozen)
            && reopened
                .0
                .value
                .origin
                .as_ref()
                .and_then(|o| o.business_signature.as_ref())
                == Some(&saved.signature),
        "C2 reopen changed frozen cipher or original signature"
    );
    let case = fixture::Case::from_h(h, saved.observations);
    let sync = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print("P4 actual reopened ordinary sync", &sync);
    proof::common(&measured)?;
    sync?;
    proof::published(&case, &saved.frozen).await?;
    let completed = case.h.witness().await?;
    for old in &saved.allocated_claims {
        let current = completed
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == old.pass_id)
            .context("same intended claim")?;
        ensure!(
            current.claim == old.claim
                && old
                    .ss_device_event_id
                    .as_ref()
                    .is_none_or(|id| current.ss_device_event_id.as_ref() == Some(id))
                && old
                    .ss_log_event_id
                    .as_ref()
                    .is_none_or(|id| current.ss_log_event_id.as_ref() == Some(id)),
            "C2 retry reallocated an already-proven claim key/generation"
        );
    }
    ensure!(
        completed
            .0
            .value
            .origin
            .as_ref()
            .and_then(|o| o.business_signature.as_ref())
            == Some(&saved.signature)
            && case
                .h
                .p
                .repo
                .inner
                .inner
                .find_one(saved.body.obj_id())
                .await?
                == Some(saved.body),
        "C2 ordinary reopen generated a different source/signature"
    );
    Ok(())
}
