use super::{repository::Repo, support::*, *};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use std::sync::atomic::Ordering;

async fn uncertainty(mode: u8) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(Repo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    let id = next(&h).await?;
    let auth = signed_value(&h.actor, id.clone())?;
    let canonical = h.log().await?;
    repo.arm(mode);
    for _ in 0..2 {
        assert!(submit(&server, &auth).await.is_err());
    }
    assert!(repo.hits.load(Ordering::SeqCst) >= 2);
    assert_eq!(h.log().await?, canonical);
    let persisted = inner.find_one(id.clone()).await?;
    assert_eq!(persisted.is_some(), mode == 4);
    repo.arm(0);
    let reopened = ServerApp::new(repo.clone(), h.master.clone())?;
    reopened.init().await?;
    let receipt = submit(&reopened, &auth).await?;
    assert_eq!(submit(&reopened, &auth).await?, receipt);
    if let Some(before) = persisted {
        assert_eq!(inner.find_one(id.clone()).await?, Some(before));
    }
    assert_eq!(normal(&h, &reopened).await?.obj_id(), id.next());
    Ok(())
}
#[tokio::test]
async fn noop_b_insert_before_repeated_failure_then_actual_retry() -> Result<()> {
    uncertainty(1).await
}
#[tokio::test]
async fn noop_b_insert_silent_drop_cannot_issue_receipt() -> Result<()> {
    uncertainty(2).await
}
#[tokio::test]
async fn noop_b_persisted_readback_failure_reopen_same_marker() -> Result<()> {
    uncertainty(4).await
}

struct Saved {
    actor: UserCreds,
    master: TransportSk,
    auth: Value,
    marker: GenericKvLogEvent,
    receipt: Value,
}
async fn persist<R: KvLogEventRepo>(inner: Arc<R>) -> Result<Saved> {
    let repo = Arc::new(Repo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    let id = next(&h).await?;
    let auth = signed_value(&h.actor, id.clone())?;
    repo.arm(3);
    // Real persistence succeeds, the wrapper loses the save reply, exact readback resolves it.
    let receipt = submit(&server, &auth).await?;
    assert_eq!(repo.hits.load(Ordering::SeqCst), 1);
    let marker = inner
        .find_one(id)
        .await?
        .context("actual persisted typed marker")?;
    assert!(matches!(marker, GenericKvLogEvent::DeviceTransportNoop(_)));
    Ok(Saved {
        actor: h.actor,
        master: h.master,
        auth,
        marker,
        receipt,
    })
}
async fn reopened<R: KvLogEventRepo>(repo: Arc<R>, saved: Saved) -> Result<()> {
    let server = ServerApp::new(repo.clone(), saved.master.clone())?;
    server.init().await?;
    assert_eq!(
        repo.find_one(saved.marker.obj_id()).await?,
        Some(saved.marker.clone())
    );
    assert_eq!(submit(&server, &saved.auth).await?, saved.receipt);
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let body = next_event(&p, &saved.actor, VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(
        meta_secret_core::node::db::events::vault::vault_log_event::AddMetaPassEvent { sender:member(&saved.actor), meta_pass_id:meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str("B reopened real N+1") },
    ))).await?;
    assert_eq!(body.obj_id(), saved.marker.obj_id().next());
    send(
        &server,
        sign_event(
            body.to_generic(),
            saved.actor.device_id().clone(),
            &saved.actor.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    assert_eq!(submit(&server, &saved.auth).await?, saved.receipt);
    let mut conflict = saved.auth.clone();
    conflict["clientEntryCommitment"] = json!("ff".repeat(32));
    assert!(
        submit(&server, &sign_value(&saved.actor, conflict)?)
            .await
            .is_err()
    );
    assert_eq!(
        repo.find_one(saved.marker.obj_id()).await?,
        Some(saved.marker)
    );
    Ok(())
}
#[tokio::test]
async fn noop_b_sqlite_lost_reply_real_reopen_codec_stable_after_n_plus_one() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("noop-b.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    // persist returns no Repo/Server/PersistentObject handles; reopen uses a new backend instance.
    let saved = persist(Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    }))
    .await?;
    reopened(Arc::new(SqlIteRepo { conn_url: url }), saved).await
}
#[tokio::test]
async fn noop_b_redb_lost_reply_real_close_reopen_codec_stable_after_n_plus_one() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("noop-b.redb");
    let saved = persist(Arc::new(ReDbRepo::new(&path)?)).await?;
    reopened(Arc::new(ReDbRepo::open(&path)?), saved).await
}
