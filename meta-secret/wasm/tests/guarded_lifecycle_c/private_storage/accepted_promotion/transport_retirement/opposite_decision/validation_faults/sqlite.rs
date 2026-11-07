use super::super::super::super::repo::Observed as PromotionObserved;
use super::*;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::crypto::encoding::base64::Base64Text;
struct Saved {
    state: State,
    keys: Vec<ArtifactId>,
    queued: Queued,
    mutant_hash: String,
    server_hash: String,
}
async fn corrupt_and_close(physical: Arc<SqlIteRepo>) -> Result<Saved> {
    let (e, queued) = fixture::queued(physical).await?;
    let server_hash = fixture::server_hash(&e.h).await?;
    let mut mutant = e.h.case.h.witness().await?;
    let retained = mutant.0.value.previous_attempts[0]
        .transport_noop
        .as_mut()
        .context("actual retained N auth")?;
    retained.authorization.signature = Base64Text::from(vec![0u8; 64]);
    ensure!(
        Vec::<u8>::try_from(&retained.authorization.signature)?.len() == 64
            && retained
                .authorization
                .verify(&e.h.case.h.state.b.user().device.keys.dsa_pk)
                .is_err(),
        "bad local signature must be valid-size and DSA-invalid"
    );
    retained
        .receipt
        .as_mut()
        .context("actual local derived receipt")?
        .authorization_hash = retained.authorization.authorization_hash()?;
    let mutant_hash = digest(&mutant)?;
    ensure!(
        digest(
            mutant
                .0
                .value
                .payload
                .as_ref()
                .context("actual current FULL")?
        )? == queued.payload_hash,
        "negative changed current frozen crypto"
    );
    e.h.physical.replace(mutant).await?; // Deliberately malformed LOCAL proof only.
    let keys = e.h.case.h.p.repo.snapshot().await?.into_keys().collect();
    let fixture::Env {
        h,
        gate,
        p,
        gateway,
    } = e;
    drop(gateway);
    drop(p);
    drop(gate);
    let OldHarness {
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
    drop(observations); // ALL old FULL/request/observers.
    let H { p, state, wire } = h;
    drop(wire);
    drop(p);
    ensure!(
        queued.retired.witness.0.value.payload.is_none(),
        "saved oracle retained old FULL"
    );
    Ok(Saved {
        state,
        keys,
        queued,
        mutant_hash,
        server_hash,
    })
}
async fn reopened_reject(physical: Arc<SqlIteRepo>, saved: Saved) -> Result<()> {
    let owner = saved.state.b.user_id();
    let mutant: LocalGuardedPreparationObject = physical
        .find_one(saved.state.witness_id())
        .await?
        .context("actual reopened mutant FULL")?
        .try_into()?;
    let index: LocalGuardedPreparationIndexObject = physical
        .find_one(saved.state.index_id())
        .await?
        .context("actual reopened index")?
        .try_into()?;
    ensure!(
        digest(&mutant)? == saved.mutant_hash
            && digest(
                mutant
                    .0
                    .value
                    .payload
                    .as_ref()
                    .context("file current FULL")?
            )? == saved.queued.payload_hash
            && digest(&index)? == saved.queued.index_hash
            && mutant.0.value.previous_attempts.len() == 1,
        "actual reopened malformed record differs"
    );
    drop(mutant); // Rebuild callers from actual file, never a retained FULL object.
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
    ensure!(stored == h.state.b, "persisted actual B identity differs");
    let h = OldHarness::from_case(ActualCase::from_h(h, observations), physical)?;
    let e = fixture::Env::wrap(h)?;
    let mutant = e.h.case.h.witness().await?;
    let retained = mutant.0.value.previous_attempts[0]
        .transport_noop
        .as_ref()
        .unwrap();
    let old = mutant.0.value.previous_attempts[0].origin.event_id.clone();
    let actual =
        e.h.case
            .h
            .state
            .server_repo
            .find_one(old)
            .await?
            .context("unchanged real server N marker")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = actual else {
        anyhow::bail!("actual N server marker missing")
    };
    let actual_user = stored.user();
    let key = &actual_user.device.keys.dsa_pk;
    ensure!(
        Vec::<u8>::try_from(&retained.authorization.signature)?.len() == 64
            && retained.authorization.verify(key).is_err()
            && marker.0.value.authorization.verify(key).is_ok()
            && retained.receipt.as_ref().unwrap().authorization_hash
                == retained.authorization.authorization_hash()?
            && fixture::server_hash(&e.h).await? == saved.server_hash,
        "actual-key DSA isolation or unchanged server proof failed"
    );
    eprintln!(
        "ROLLOVER HISTORY SQLite actual_bad_signature_valid_size=true local_hash_consistent=true verify_actual_persisted_B_key_rejects=true actual_server_signature_still_valid=true SAME_file_no_FULL_RAM_recovery=true"
    );
    checks::reject(
        &e,
        &mutant,
        &saved.server_hash,
        "SQLiteBadRetainedSignature",
    )
    .await
}
#[tokio::test]
async fn sqlite_real_closed_reopened_full_retry_rejects_bad_retained_signature_using_actual_user_key()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("bad-retained-signature.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let physical = Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    });
    let weak = Arc::downgrade(&physical);
    let saved = corrupt_and_close(physical).await?;
    ensure!(
        weak.upgrade().is_none(),
        "actual SQLite still has a local owner at reopen"
    );
    eprintln!(
        "ROLLOVER HISTORY SQLite actual_Weak_None=true ALL_local_owners_dropped=true actual_SAME_file_reopen=true"
    );
    reopened_reject(Arc::new(SqlIteRepo { conn_url: url }), saved).await
}
