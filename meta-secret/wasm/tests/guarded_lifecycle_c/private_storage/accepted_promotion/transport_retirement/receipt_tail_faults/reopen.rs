use super::super::super::{fixture::Case as ActualCase, repo::Observed as PromotionObserved};
use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

/// Contains no local Arc, old Observations/Expected, FULL payload or transfer body.
struct Saved {
    state: State,
    keys: Vec<ArtifactId>,
    reference: LocalGuardedPreparationObject,
    persisted: LocalGuardedPreparationObject,
    index_hash: String,
    registry_hash: String,
    original_body_hash: String,
    redacted_body_hash: String,
    canonical_hash: String,
    claim_identities: HashMap<String, String>,
}
async fn interrupt<R: KvLogEventRepo>(physical: Arc<R>, mode: Mode) -> Result<Saved> {
    let h = harness::Harness::new(physical, mode).await?;
    let first = h.sync().await;
    proof::interrupted(&h, mode, &first).await?;
    ensure!(
        h.wire.hits.load(Ordering::SeqCst) == 1,
        "actual file transport cut UNREACHED"
    );
    let persisted = h.repo.witness().await?;
    let technical = persisted
        .0
        .value
        .transport_noop
        .as_ref()
        .context("actual compact file proof")?;
    ensure!(
        persisted.0.value.payload.is_none()
            && technical.confirmed_next_tail.is_none()
            && (mode != Mode::TailLoss || technical.receipt.is_some()),
        "file interruption is not selected auth/receipt durability point"
    );
    let origin = persisted
        .0
        .value
        .origin
        .as_ref()
        .context("actual compact file origin")?;
    let redacted_body_hash = digest(
        &h.physical
            .find_one(origin.event_id.clone())
            .await?
            .context("actual redacted file N")?,
    )?;
    let keys = h.case.h.p.repo.snapshot().await?.into_keys().collect();
    let harness::Harness {
        case,
        physical,
        repo,
        wire,
        gateway,
        reference,
        body_hash,
        index_hash,
        registry_hash,
        canonical_hash,
        claim_identities,
    } = h;
    drop(gateway);
    drop(wire);
    drop(repo);
    drop(physical);
    let ActualCase {
        h,
        wire,
        observations,
    } = case;
    drop(wire);
    drop(observations); // Includes old Expected FULL and original packet vectors.
    let H { p, state, wire } = h;
    drop(wire);
    drop(p); // ALL local wrappers/backend owners are now released.
    ensure!(
        reference.0.value.payload.is_none(),
        "reopen oracle retained expected FULL"
    );
    Ok(Saved {
        state,
        keys,
        reference,
        persisted,
        index_hash,
        registry_hash,
        original_body_hash: body_hash,
        redacted_body_hash,
        canonical_hash,
        claim_identities,
    })
}
async fn resume<R: KvLogEventRepo>(physical: Arc<R>, saved: Saved, mode: Mode) -> Result<()> {
    let owner = saved.state.b.user_id();
    let witness_id = saved.state.witness_id();
    let index_id = saved.state.index_id();
    let witness_equal =
        physical.find_one(witness_id.clone()).await? == Some(saved.persisted.clone().to_generic());
    let actual_index: LocalGuardedPreparationIndexObject = physical
        .find_one(index_id)
        .await?
        .context("reopened actual index")?
        .try_into()?;
    let index_equal = digest(&actual_index)? == saved.index_hash;
    let registry_equal = digest(
        &physical
            .find_one(ArtifactId::from(LocalRemovalEnvelopeDescriptor(
                owner.clone(),
            )))
            .await?
            .context("reopened actual registry")?,
    )? == saved.registry_hash;
    let no_history = physical.find_one(witness_id.next()).await?.is_none();
    eprintln!(
        "RECEIPT_TAIL FILE {mode:?} LOAD physical_witness_exact={witness_equal} SAME_typed_index_hash_exact={index_equal} physical_registry_hash_exact={registry_equal} no_private_history={no_history}"
    );
    ensure!(
        witness_equal && index_equal && registry_equal && no_history,
        "actual reopened compact file records differ or contain history"
    );
    let origin = saved
        .persisted
        .0
        .value
        .origin
        .as_ref()
        .context("reopened compact source")?;
    ensure!(
        digest(
            &physical
                .find_one(origin.event_id.clone())
                .await?
                .context("reopened actual redacted N")?
        )? == saved.redacted_body_hash,
        "reopened actual redacted source differs"
    );
    // Fresh observers: no old RAM Expected/FULL survives into the reopened caller.
    let observations = Arc::new(Observations::default());
    let observed = Arc::new(PromotionObserved::new(
        physical.clone(),
        observations.clone(),
    ));
    let existing = Arc::new(ExistingRepo::new(observed, owner, saved.state.uuid.clone()));
    existing.remember(saved.keys);
    let h = H::open(existing, saved.state)?;
    let stored = PersistentCredentials {
        p_obj: h.p.clone(),
        master_key: h.state.master.clone(),
    }
    .get_user_creds()
    .await?
    .context("reopened persisted credentials")?;
    ensure!(
        stored.user() == h.state.b.user(),
        "reopened actual credentials changed B identity"
    );
    let case = GenericCase::from_h(h, observations);
    let mut reference = saved.reference;
    reference.0.value.transport_noop = saved.persisted.0.value.transport_noop;
    let h = harness::Harness::from_case(
        case,
        physical,
        mode,
        harness::SeedProof {
            reference,
            body_hash: saved.original_body_hash,
            index_hash: saved.index_hash,
            registry_hash: saved.registry_hash,
            canonical_hash: saved.canonical_hash,
            claim_identities: saved.claim_identities,
        },
    )?;
    h.clear();
    let cuts = h.wire.cuts.lock().unwrap().len();
    let result = h.sync().await;
    proof::settled(&h, &result, cuts).await?;
    let actual = h.repo.witness().await?;
    let noops = h.wire.noops.lock().unwrap().len();
    let cuts = h.wire.cuts.lock().unwrap().len();
    let repeat = h.sync().await;
    proof::settled(&h, &repeat, cuts).await?;
    ensure!(
        h.repo.witness().await? == actual && h.wire.noops.lock().unwrap().len() == noops,
        "file confirmed retry regenerated proof or resent Noop"
    );
    eprintln!(
        "RECEIPT_TAIL FILE {mode:?} reopened_physical_compact_index_registry_redactedN=true persisted_B_identity=true SAME_auth_receipt=true real_separateTail_outer_readback_before_FIRST_SS=true real_retry_Noop_attempts={noops} immutable_repeat=true"
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_actual_noop_commit_reply_loss_all_local_arcs_close_and_same_file_reopens()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("receipt-tail.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let physical = Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    });
    let weak = Arc::downgrade(&physical);
    let saved = interrupt(physical, Mode::ServerReplyLoss).await?;
    ensure!(
        weak.upgrade().is_none(),
        "actual SQLite local backend remained live through reopen"
    );
    eprintln!(
        "RECEIPT_TAIL SQLite actual_backend_Weak_None=true ALL_local_owners_dropped=true SAME_file_reopen=true no_retained_FULL_expected=true"
    );
    resume(
        Arc::new(SqlIteRepo { conn_url: url }),
        saved,
        Mode::ServerReplyLoss,
    )
    .await
}

#[tokio::test]
async fn redb_actual_durable_receipt_tail_loss_all_local_arcs_close_and_same_file_reopens()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("receipt-tail.redb");
    let physical = Arc::new(ReDbRepo::new(&path)?);
    let weak = Arc::downgrade(&physical);
    let saved = interrupt(physical, Mode::TailLoss).await?;
    ensure!(
        weak.upgrade().is_none(),
        "actual ReDB Database handle remained live through reopen"
    );
    let reopened = Arc::new(ReDbRepo::open(&path)?);
    eprintln!(
        "RECEIPT_TAIL ReDB actual_backend_Weak_None=true actual_same_Database_file_open=true ALL_local_owners_dropped=true no_retained_FULL_expected=true"
    );
    resume(reopened, saved, Mode::TailLoss).await
}
