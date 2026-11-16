use super::super::super::repo::Observed as PromotionObserved;
use super::*;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

/// Only compact N and commitment oracles survive. Current FULL must come from SAME file.
struct Saved {
    state: State,
    keys: Vec<ArtifactId>,
    queued: Queued,
}
async fn queue_and_close(physical: Arc<SqlIteRepo>) -> Result<Saved> {
    let h = harness::Harness::fresh(physical).await?;
    let retired = proof::prerequisites(&h).await?;
    let queued = proof::queue(&h, retired).await?;
    let keys = h.case.h.p.repo.snapshot().await?.into_keys().collect();
    ensure!(
        queued.retired.witness.0.value.payload.is_none(),
        "file recovery oracle retained old FULL"
    );
    let harness::Harness {
        case,
        physical,
        repo,
        wire,
        p,
        gateway,
    } = h;
    drop(gateway);
    drop(wire);
    drop(p);
    drop(repo);
    drop(physical);
    let ActualCase {
        h,
        wire,
        observations,
    } = case;
    drop(wire);
    drop(observations); // Drops old Expected/request ciphertext/original vectors.
    let H { p, state, wire } = h;
    drop(wire);
    drop(p);
    Ok(Saved {
        state,
        keys,
        queued,
    })
}
async fn reopen_and_finish(physical: Arc<SqlIteRepo>, saved: Saved) -> Result<()> {
    let owner = saved.state.b.user_id();
    let current: LocalGuardedPreparationObject = physical
        .find_one(saved.state.witness_id())
        .await?
        .context("actual reopened current FULL")?
        .try_into()?;
    let index: LocalGuardedPreparationIndexObject = physical
        .find_one(saved.state.index_id())
        .await?
        .context("actual reopened existing index")?
        .try_into()?;
    let registry = physical
        .find_one(ArtifactId::from(LocalRemovalEnvelopeDescriptor(
            owner.clone(),
        )))
        .await?
        .context("actual reopened envelope registry")?;
    let old_id = saved
        .queued
        .retired
        .witness
        .0
        .value
        .origin
        .as_ref()
        .context("old compact source")?
        .event_id
        .clone();
    let old_body = physical
        .find_one(old_id)
        .await?
        .context("actual reopened redacted old N")?;
    let new_body = physical
        .find_one(saved.queued.event_id.clone())
        .await?
        .context("actual reopened current N+1")?
        .device_log()?;
    ensure!(
        digest(&current)? == saved.queued.witness_hash
            && digest(
                current
                    .0
                    .value
                    .payload
                    .as_ref()
                    .context("current FULL read from file")?
            )? == saved.queued.payload_hash
            && digest(&index)? == saved.queued.index_hash
            && digest(&registry)? == saved.queued.registry_hash
            && digest(&old_body)? == saved.queued.retired.body_hash
            && digest(&new_body)? == saved.queued.body_hash
            && current.0.value.previous_attempts.len() == 1
            && index
                .0
                .value
                .request_ids
                .iter()
                .filter(|id| **id == saved.state.uuid)
                .count()
                == 1
            && physical
                .find_one(saved.queued.event_id.clone().next())
                .await?
                .is_none()
            && physical
                .find_one(saved.state.witness_id().next())
                .await?
                .is_none(),
        "actual reopened file identity/frozen bytes/archive/index/both body slots differ"
    );
    drop(current); // No in-memory FULL is used to reconstruct repository state.
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
    .context("actual persisted B credentials")?;
    ensure!(stored == h.state.b, "reopened persisted B identity differs");
    let case = ActualCase::from_h(h, observations);
    let h = harness::Harness::from_case(case, physical)?;
    eprintln!(
        "OPPOSITE SQLite actual_same_file_compactN_currentFULL_index_registry_both_slots=true persisted_credentials=true no_FULL_RAM_recovery=true"
    );
    proof::retry_and_admit(&h, &saved.queued).await
}
#[tokio::test]
async fn sqlite_explicit_opposite_current_full_true_close_reopen_same_accept_retry_and_admission()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("explicit-opposite.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let physical = Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    });
    let weak = Arc::downgrade(&physical);
    let saved = queue_and_close(physical).await?;
    ensure!(
        weak.upgrade().is_none(),
        "SQLite ALL-local ownership close was not real"
    );
    eprintln!(
        "OPPOSITE SQLite actual_Weak_None=true ALL_local_Arcs_dropped=true SAME_file_reopened=true"
    );
    reopen_and_finish(Arc::new(SqlIteRepo { conn_url: url }), saved).await
}
