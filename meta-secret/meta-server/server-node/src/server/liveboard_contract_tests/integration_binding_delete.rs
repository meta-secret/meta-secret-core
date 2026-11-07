//! Same admitted automatic Delete must finish before raw acknowledgement.
use super::integration_binding_fixture::{FaultRepo, Seed, tail};
use super::*;
use meta_db_redb::ReDbRepo;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::node::{
    common::model::meta_pass::MetaPasswordId,
    db::{events::vault::vault_log_event::AddMetaPassEvent, repo::generic_db::FindOneQuery},
};
use std::sync::atomic::Ordering;

struct Interrupted {
    actor: UserCreds,
    master: TransportSk,
    pass: MetaPasswordId,
    workflows: Vec<ArtifactId>,
    source: DeviceLogObject,
    signed: SignedAction,
    request_id: String,
}

async fn interrupted<R: KvLogEventRepo>(inner: Arc<R>, count: usize) -> Result<Interrupted> {
    let repo = Arc::new(FaultRepo::new(inner));
    let h = Seed::new(repo.clone(), count).await?;
    let server = h.server().await?;
    let (source, signed, request_id) = h.deletion().await?;
    repo.before_target();
    assert!(send(&server, signed.clone()).await.is_err());
    assert_eq!(
        repo.hits.load(Ordering::SeqCst),
        1,
        "real target save was not reached"
    );
    assert!(repo.find_one(source.obj_id()).await?.is_none());
    let log = h.log().await?;
    assert!(log.0.value.requests.iter().any(|event| matches!(event,
        VaultActionRequestEvent::DeleteSecret(p) if p.request_id == request_id)));
    for id in &h.workflows {
        assert!(
            repo.find_one(id.clone()).await?.is_some(),
            "failure was before ciphertext purge"
        );
    }
    Ok(Interrupted {
        actor: h.actor,
        master: h.master,
        pass: h.pass,
        workflows: h.workflows,
        source,
        signed,
        request_id,
    })
    // All original real backend/server/adapter handles close on return.
}

async fn assert_completed<R: KvLogEventRepo>(
    repo: &Arc<R>,
    actor: &UserCreds,
    request_id: &str,
    pass: &MetaPasswordId,
) -> Result<()> {
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let vault = PersistentVault::from(p.clone())
        .get_vault(actor.vault_name.clone())
        .await?
        .to_data();
    let result = vault
        .deletion_results
        .get(request_id)
        .context("same exact automatic result")?;
    assert!(result.accepted);
    assert_eq!(result.request.pass_id, *pass);
    assert_eq!(
        vault.deletion_results.len(),
        1,
        "same source must never allocate P2"
    );
    assert!(vault.deleted_secrets.iter().any(|item| item.id == pass.id));
    assert!(!vault.secrets.iter().any(|item| item.id == pass.id));
    let log = PersistentVault::from(p)
        .get_vault_log_artifact(actor.vault_name.clone())
        .await?;
    assert!(!log.0.value.requests.iter().any(|event| matches!(event,
        VaultActionRequestEvent::DeleteSecret(p) if p.request_id == request_id)));
    assert_eq!(log.0.value.deletion_results.len(), 1);
    let wire = serde_json::to_value(log.0.value)?;
    assert!(
        wire.get("automaticDeleteApplication")
            .is_none_or(Value::is_null),
        "completed phase must be cleared"
    );
    Ok(())
}

async fn assert_next_progress<R: KvLogEventRepo>(
    server: &ServerApp<R>,
    p: &Arc<PersistentObject<R>>,
    actor: &UserCreds,
) -> Result<()> {
    let pass = MetaPasswordId::build_from_str("legitimate-command-after-delete");
    let next = next_event(
        p,
        actor,
        VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(AddMetaPassEvent {
            sender: member(actor),
            meta_pass_id: pass.clone(),
        })),
    )
    .await?;
    let signed = sign_event(
        next.clone().to_generic(),
        actor.device_id().clone(),
        &actor.device_creds.key_manager()?.dsa,
    )?;
    send(server, signed)
        .await
        .context("next legitimate own-stream command must progress")?;
    assert!(p.repo.find_one(next.obj_id()).await? == Some(next.to_generic()));
    assert!(
        PersistentVault::from(p.clone())
            .get_vault(actor.vault_name.clone())
            .await?
            .0
            .value
            .secrets
            .contains(&pass)
    );
    Ok(())
}

async fn resumed<R: KvLogEventRepo>(repo: Arc<R>, state: Interrupted) -> Result<()> {
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let server = ServerApp::new(repo.clone(), state.master)?;
    server.init().await?;
    send(&server, state.signed.clone())
        .await
        .context("RED: reopen must finish SAME automatic P/R before acknowledging source")?;
    assert_completed(&repo, &state.actor, &state.request_id, &state.pass).await?;
    for id in state.workflows {
        assert!(
            repo.find_one(id).await?.is_none(),
            "actual Encrypted Key Share must be purged"
        );
    }
    assert!(repo.find_one(state.source.obj_id()).await? == Some(state.source.clone().to_generic()));
    assert_eq!(
        tail(&server, &state.actor).await?.device_log_tail,
        Some(state.source.obj_id().next())
    );
    assert!(
        send(&server, state.signed).await.is_err(),
        "already stored original Event remains replay-protected"
    );
    assert_next_progress(&server, &p, &state.actor).await
}

async fn sqlite(count: usize) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("automatic-delete.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let state = interrupted(
        Arc::new(SqlIteRepo {
            conn_url: url.clone(),
        }),
        count,
    )
    .await?;
    resumed(Arc::new(SqlIteRepo { conn_url: url }), state).await
}
async fn redb(count: usize) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("automatic-delete.redb");
    let state = interrupted(Arc::new(ReDbRepo::new(&path)?), count).await?;
    resumed(Arc::new(ReDbRepo::open(&path)?), state).await
}
#[tokio::test]
async fn integration_binding_delete_sqlite_one_device_same_p_after_target_fault_reopen()
-> Result<()> {
    sqlite(1).await
}
#[tokio::test]
async fn integration_binding_delete_sqlite_two_devices_same_p_after_target_fault_reopen()
-> Result<()> {
    sqlite(2).await
}
#[tokio::test]
async fn integration_binding_delete_redb_one_device_same_p_after_target_fault_reopen() -> Result<()>
{
    redb(1).await
}
#[tokio::test]
async fn integration_binding_delete_redb_two_devices_same_p_after_target_fault_reopen() -> Result<()>
{
    redb(2).await
}

async fn prepared_once(count: usize) -> Result<()> {
    let repo = Arc::new(FaultRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), count).await?;
    let server = h.server().await?;
    let (source, signed, request_id) = h.deletion().await?;
    let base = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    repo.before_target();
    assert!(send(&server, signed).await.is_err());
    assert_eq!(repo.hits.load(Ordering::SeqCst), 1);
    let wire = serde_json::to_value(h.log().await?.0.value)?;
    let phase = wire
        .get("automaticDeleteApplication")
        .context("RED: admitted automatic P must atomically contain its prepared phase")?;
    assert_eq!(phase["requestId"], request_id);
    assert_eq!(
        phase["sourceEventId"],
        serde_json::to_value(source.obj_id())?
    );
    assert_eq!(
        phase["baseVaultArtifactId"],
        serde_json::to_value(base.obj_id())?
    );
    assert_eq!(
        phase["targetVaultArtifactId"],
        serde_json::to_value(base.obj_id().next())?
    );
    assert_eq!(phase["phase"], "prepared");
    assert!(repo.find_one(source.obj_id()).await?.is_none());
    Ok(())
}
#[tokio::test]
async fn integration_binding_delete_one_device_admission_has_same_p_prepared_phase() -> Result<()> {
    prepared_once(1).await
}
#[tokio::test]
async fn integration_binding_delete_two_devices_admission_has_same_p_prepared_phase() -> Result<()>
{
    prepared_once(2).await
}

async fn automatic_control(count: usize) -> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = Seed::new(repo.clone(), count).await?;
    let server = h.server().await?;
    let (source, signed, id) = h.deletion().await?;
    send(&server, signed).await?;
    assert_completed(&repo, &h.actor, &id, &h.pass).await?;
    for id in h.workflows {
        assert!(repo.find_one(id).await?.is_none());
    }
    assert!(repo.find_one(source.obj_id()).await? == Some(source.to_generic()));
    Ok(())
}
#[tokio::test]
async fn integration_binding_delete_one_device_existing_automatic_policy_control() -> Result<()> {
    automatic_control(1).await
}
#[tokio::test]
async fn integration_binding_delete_two_devices_existing_automatic_policy_control() -> Result<()> {
    automatic_control(2).await
}

#[tokio::test]
async fn integration_binding_delete_three_devices_stays_remote_pending_control() -> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = Seed::new(repo.clone(), 3).await?;
    let server = h.server().await?;
    let (source, signed, request_id) = h.deletion().await?;
    send(&server, signed).await?;
    let log = h.log().await?;
    assert!(log.0.value.requests.iter().any(|event| matches!(event,
        VaultActionRequestEvent::DeleteSecret(p) if p.request_id == request_id)));
    let wire = serde_json::to_value(log.0.value)?;
    assert!(
        wire.get("automaticDeleteApplication")
            .is_none_or(Value::is_null)
    );
    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?
        .to_data();
    assert!(!vault.deletion_results.contains_key(&request_id));
    assert!(vault.deleted_secrets.is_empty());
    assert!(repo.find_one(source.obj_id()).await? == Some(source.to_generic()));
    for id in h.workflows {
        assert!(repo.find_one(id).await?.is_some());
    }
    Ok(())
}
