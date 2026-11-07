use super::{
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    recovery_intake_fixture::{IntakeHarness, assert_clean},
    *,
};
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};

async fn interrupt<R: KvLogEventRepo>(
    inner: Arc<R>,
) -> Result<(
    Harness,
    meta_secret_core::node::db::events::shared_secret_event::SsDeviceLogObject,
    SignedAction,
    ArtifactId,
)> {
    let repo = Arc::new(IntakeFaultRepo::new(inner));
    let h = IntakeHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(Phase::Raw, Fault::Before);
    assert!(send(&server, h.signed.clone()).await.is_err());
    let log = h.log().await?;
    assert!(
        log.0.value.pending_recovery_request_intake.is_some(),
        "RED: real backend lacks canonical Prepared before raw failure"
    );
    Ok((h.base, h.source, h.signed, h.prepared.obj_id())) // Drop all original DB handles here.
}
async fn reopen<R: KvLogEventRepo>(
    repo: Arc<R>,
    state: (
        Harness,
        meta_secret_core::node::db::events::shared_secret_event::SsDeviceLogObject,
        SignedAction,
        ArtifactId,
    ),
) -> Result<()> {
    use meta_secret_core::node::{
        api::ServerTailRequest, db::objects::persistent_shared_secret::PersistentSharedSecret,
    };
    let (base, source, signed, target) = state;
    let server = ServerApp::new(repo.clone(), base.master_key)?;
    let response = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                producer_reference: None,
                sender: base.a.user(),
            },
        ))))
        .await?;
    assert_clean(&response)?;
    let log = PersistentSharedSecret::from(Arc::new(PersistentObject::new(repo.clone())))
        .find_ss_log_tail_event(base.a.vault_name.clone())
        .await?
        .context("reopened canonical log")?;
    assert_eq!(log.obj_id(), target);
    assert!(log.0.value.pending_recovery_request_intake.is_none());
    assert!(repo.find_one(source.obj_id()).await? == Some(source.to_generic()));
    assert!(send(&server, signed).await.is_err());
    Ok(())
}
#[tokio::test]
async fn recovery_intake_sqlite_real_close_reopen_finishes_exact_prepared_source() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("intake.sqlite").display());
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let state = interrupt(Arc::new(SqlIteRepo {
        conn_url: url.clone(),
    }))
    .await?;
    reopen(Arc::new(SqlIteRepo { conn_url: url }), state).await
}
#[tokio::test]
async fn recovery_intake_redb_real_close_reopen_finishes_exact_prepared_source() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("intake.redb");
    let state = interrupt(Arc::new(ReDbRepo::new(&path)?)).await?;
    reopen(Arc::new(ReDbRepo::open(&path)?), state).await
}
