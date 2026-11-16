use super::recovery_receipt_faults::{Fault, ReceiptFaultRepo, Step};
use super::recovery_receipt_fixture::ReceiptHarness;
use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::descriptors::object_descriptor::ObjectFqdn;
use meta_secret_core::node::db::repo::generic_db::DbCleanUpCommand;
use meta_secret_core::node::db::repo::recovery_response_store::{
    PrivateRecoveryResponse, RecoveryResponsePhase,
};

async fn reserve<R: KvLogEventRepo>(
    repo: Arc<R>,
) -> Result<(
    Harness,
    meta_secret_core::node::common::model::secret::SsClaim,
    PrivateRecoveryResponse,
)> {
    let h = ReceiptHarness::new(repo).await?;
    let row = h.prepared(h.packet(&h.base.b, true)?, true).await?;
    let store = h
        .repo
        .server_recovery_response_store()
        .context("RED: real backend must expose its own durable private store")?;
    store.ensure_ready().await?;
    store.ensure_ready().await?;
    store.reserve(&row.vault_name, &row).await?;
    assert!(
        store.load(&row.vault_name).await? == Some(row.clone()),
        "real backend must read the exact reserved packet"
    );
    Ok((h.base, h.claim, row))
}

async fn after_reopen<R: KvLogEventRepo>(repo: Arc<R>, row: PrivateRecoveryResponse) -> Result<()> {
    let store = repo.server_recovery_response_store()?;
    store.ensure_ready().await?;
    assert!(
        store.load(&row.vault_name).await? == Some(row.clone()),
        "private row must survive actual database reopen"
    );
    store.reserve(&row.vault_name, &row).await?;
    let mut conflict = row.clone();
    conflict.source_proof.verified_envelope_hash.push('f');
    assert!(
        store.reserve(&row.vault_name, &conflict).await.is_err(),
        "occupied private row cannot be replaced by a different source"
    );
    for field in 0..6 {
        let mut changed = row.clone();
        match field {
            0 => changed.claim_id = meta_secret_core::node::common::model::secret::ClaimId(meta_secret_core::crypto::utils::Id48bit::generate()),
            1 => changed.base_ss_log_artifact_id = changed.base_ss_log_artifact_id.next(),
            2 => changed.target_ss_log_artifact_id = changed.target_ss_log_artifact_id.next(),
            3 => changed.source_proof.workflow_artifact_id = changed.source_proof.workflow_artifact_id.next(),
            4 => changed.source_proof.outcome = meta_secret_core::node::db::events::vault::liveboard_metadata::DecisionOutcome::Declined,
            _ => changed.packet.as_mut().context("Prepared original packet")?.nonce += 1,
        }
        assert!(
            store.reserve(&row.vault_name, &changed).await.is_err(),
            "reserve must compare the complete prepared source/packet/base/target, not just a hash"
        );
        assert!(
            store.load(&row.vault_name).await? == Some(row.clone()),
            "failed competing reserve cannot replace any part of the first response"
        );
    }
    assert!(
        store
            .mark_committed(&row.vault_name, &conflict.source_proof)
            .await
            .is_err()
    );
    assert!(
        store
            .try_remove_committed(&row.vault_name, &row.source_proof)
            .await
            .is_err(),
        "Prepared packet cannot be deleted as committed"
    );
    assert!(store.load(&row.vault_name).await? == Some(row.clone()));
    for object_type in [
        "server_recovery_response_private",
        "server-recovery-response-private-v1",
        "RecoveryResponse",
    ] {
        let id = ArtifactId::from(ObjectFqdn {
            obj_type: object_type.into(),
            obj_instance: row.vault_name.0.clone(),
        });
        assert!(
            repo.find_one(id).await?.is_none(),
            "private rows are not GenericKvLogEvent/ArtifactId-addressable"
        );
    }
    store
        .mark_committed(&row.vault_name, &row.source_proof)
        .await?;
    let committed = store
        .load(&row.vault_name)
        .await?
        .context("committed private row must remain until checked deletion")?;
    assert!(
        committed.phase == RecoveryResponsePhase::Committed && committed.packet.is_none(),
        "commit must atomically remove retained ciphertext"
    );
    assert!(
        store.reserve(&row.vault_name, &row).await.is_err(),
        "Prepared reserve cannot resurrect a committed packet"
    );
    assert!(
        store
            .try_remove_committed(&row.vault_name, &conflict.source_proof)
            .await
            .is_err()
    );
    store
        .mark_committed(&row.vault_name, &row.source_proof)
        .await?;
    store
        .try_remove_committed(&row.vault_name, &row.source_proof)
        .await?;
    assert!(store.load(&row.vault_name).await?.is_none());
    store.reserve(&row.vault_name, &row).await?;
    repo.db_clean_up().await;
    assert!(
        store.load(&row.vault_name).await?.is_none(),
        "whole owned database reset must clear the private table as well as public events"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_sqlite_http_backend_private_row_survives_reopen_and_cas() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("server.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let (_, _, row) = reserve(Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    }))
    .await?;
    after_reopen(Arc::new(SqlIteRepo { conn_url: url }), row).await
}

#[tokio::test]
async fn recovery_receipt_redb_private_row_survives_file_close_reopen_and_cas() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("server.redb");
    let (_, _, row) = reserve(Arc::new(ReDbRepo::new(&path)?)).await?;
    after_reopen(Arc::new(ReDbRepo::open(&path)?), row).await
}

#[tokio::test]
async fn recovery_receipt_inmem_private_store_is_repo_owned_and_not_in_exported_event_map()
-> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let before = repo.get_db().await;
    let (_, _, row) = reserve(repo.clone()).await?;
    let events = repo.get_db().await;
    assert!(
        events.values().all(|event| !serde_json::to_string(event)
            .unwrap()
            .contains("verifiedEnvelopeHash")),
        "private reservation cannot enter public exported event map"
    );
    assert!(
        events.len() > before.len(),
        "canonical real fixture events remain separate"
    );
    after_reopen(repo.clone(), row.clone()).await?;
    let store = repo.server_recovery_response_store()?;
    store.reserve(&row.vault_name, &row).await?;
    repo.db_clean_up().await;
    assert!(
        store.load(&row.vault_name).await?.is_none(),
        "whole owned DB reset clears both stores"
    );
    Ok(())
}

async fn reopen_pending<R: KvLogEventRepo>(
    repo: Arc<R>,
) -> Result<(
    Harness,
    meta_secret_core::node::common::model::secret::SsClaim,
    SignedAction,
)> {
    let fault = Arc::new(ReceiptFaultRepo::new(repo));
    let h = ReceiptHarness::new(fault.clone()).await?;
    let packet = h.packet(&h.base.b, true)?;
    let server = h.server().await?;
    fault.arm(Step::Raw, Fault::Before);
    assert!(send(&server, packet.clone()).await.is_err());
    let row = fault
        .inner
        .server_recovery_response_store()?
        .load(&h.claim.vault_name)
        .await?
        .context("RED: verified reservation must precede the failed raw save")?;
    assert!(row.packet.as_ref() == Some(&packet));
    Ok((h.base, h.claim, packet))
}

async fn repaired<R: KvLogEventRepo>(
    repo: Arc<R>,
    base: Harness,
    claim: meta_secret_core::node::common::model::secret::SsClaim,
    packet: SignedAction,
) -> Result<()> {
    let h = ReceiptHarness { base, repo, claim };
    let server = h.server().await?;
    refresh(&server, &h.base.a).await?; // No original sender retransmission yet.
    h.assert_winner(true).await?;
    let _ = send(&server, h.packet(&h.base.c, false)?).await;
    h.assert_winner(true).await?;
    send(&server, packet).await?;
    assert!(
        h.repo
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_sqlite_restart_repairs_before_sender_retransmission() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("server.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let (base, claim, packet) = reopen_pending(Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    }))
    .await?;
    repaired(Arc::new(SqlIteRepo { conn_url: url }), base, claim, packet).await
}

#[tokio::test]
async fn recovery_receipt_redb_restart_repairs_before_sender_retransmission() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("server.redb");
    let (base, claim, packet) = reopen_pending(Arc::new(ReDbRepo::new(&path)?)).await?;
    repaired(Arc::new(ReDbRepo::open(&path)?), base, claim, packet).await
}

#[tokio::test]
async fn recovery_receipt_private_phase_schema_and_vault_mismatch_fail_closed() -> Result<()> {
    let h = ReceiptHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let store = h.repo.server_recovery_response_store()?;
    store.ensure_ready().await?;
    let row = h.prepared(h.packet(&h.base.b, true)?, true).await?;
    let mut invalid = row.clone();
    invalid.version = 99;
    assert!(store.reserve(&row.vault_name, &invalid).await.is_err());
    let mut invalid = row.clone();
    invalid.phase = RecoveryResponsePhase::Committed;
    assert!(store.reserve(&row.vault_name, &invalid).await.is_err());
    assert!(
        store
            .reserve(&VaultName::from("wrong-vault"), &row)
            .await
            .is_err()
    );
    assert!(
        store.load(&row.vault_name).await?.is_none(),
        "invalid reservations have no durable effect"
    );
    Ok(())
}
