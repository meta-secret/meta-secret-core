use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::repo::generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

#[derive(Clone, Copy)]
enum Fault {
    None = 0,
    RejectVaultLog = 1,
    DropVaultLog = 2,
    ConflictingVaultLog = 3,
}

struct FaultRepo {
    inner: Arc<InMemKvLogEventRepo>,
    fault: AtomicU8,
    hits: AtomicUsize,
}

impl FaultRepo {
    fn new(inner: Arc<InMemKvLogEventRepo>) -> Self {
        Self {
            inner,
            fault: AtomicU8::new(Fault::None as u8),
            hits: AtomicUsize::new(0),
        }
    }
    fn arm(&self, fault: Fault) {
        self.fault.store(fault as u8, Ordering::SeqCst);
    }
}

#[async_trait(?Send)]
impl SaveCommand for FaultRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let mut event = value.to_generic();
        if matches!(event, GenericKvLogEvent::VaultLog(_)) && self.fault.load(Ordering::SeqCst) != 0
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            match self.fault.load(Ordering::SeqCst) {
                1 => anyhow::bail!("injected VaultLog write failure"),
                2 => return Ok(event.obj_id()),
                3 => {
                    if let GenericKvLogEvent::VaultLog(log) = &mut event {
                        log.0.value.requests.clear();
                    }
                    self.inner.save(event.clone()).await?;
                    return Ok(event.obj_id());
                }
                _ => {}
            }
        }
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}

#[async_trait(?Send)]
impl FindOneQuery for FaultRepo {
    async fn find_one(&self, key: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.inner.find_one(key).await
    }
    async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(key).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for FaultRepo {
    async fn delete(&self, key: ArtifactId) {
        self.inner.delete(key).await
    }
    async fn try_delete(&self, key: ArtifactId) -> Result<()> {
        self.inner.try_delete(key).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for FaultRepo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for FaultRepo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.inner.server_recovery_response_store()
    }
}

async fn faulted_request(fault: Fault) -> Result<()> {
    let h = Harness::new().await?;
    let repo = Arc::new(FaultRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    repo.arm(fault);
    let event = next_event(
        &h.p_obj,
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
    )
    .await?;
    let action = sign_event(
        event.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let result = send(&server, action).await;
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "fault must hit the real canonical pending save"
    );
    assert!(
        result.is_err(),
        "RED: save returning an ID is not proof the exact pending+binding record was durably stored"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_pending_save_ack_without_record_must_fail_readback() -> Result<()> {
    faulted_request(Fault::DropVaultLog).await
}

#[tokio::test]
async fn liveboard_pending_save_at_occupied_key_with_different_content_must_fail_readback()
-> Result<()> {
    faulted_request(Fault::ConflictingVaultLog).await
}

#[tokio::test]
async fn liveboard_partial_removal_write_never_publishes_mixed_membership_and_pending_state()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let repo = Arc::new(FaultRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    repo.arm(Fault::RejectVaultLog);
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(true))),
    )
    .await?;
    let action = sign_event(
        event.to_generic(),
        h.c.device_id().clone(),
        &h.c.device_creds.key_manager()?.dsa,
    )?;
    assert!(
        send(&server, action).await.is_err(),
        "injected storage failure cannot report successful removal"
    );
    assert!(repo.hits.load(Ordering::SeqCst) > 0);

    // Reopen the real server on the same persistent data. If repair is still
    // blocked, refresh may fail. It must never serve settled membership while
    // the same canonical request is still pending or lacks its committed result.
    let reopened = ServerApp::new(repo.clone(), h.master_key.clone())?;
    let read = refresh(&reopened, &h.a).await;
    let p_vault = PersistentVault::from(h.p_obj.clone());
    if read.is_ok() {
        let vault = p_vault.get_vault(h.a.vault_name.clone()).await?;
        let log = h.log().await?;
        let pending = log["requests"].as_array().is_some_and(|requests| {
            requests
                .iter()
                .any(|request| request.get("removeMember").is_some())
        });
        assert!(
            !(vault.0.value.members().len() == 2 && pending),
            "RED: refresh exposed removed membership together with its still-pending removal request"
        );
    }
    repo.arm(Fault::None);
    refresh(&reopened, &h.a).await?;
    // Failed BEFORE preparation leaves the request pending; failed AFTER
    // preparation must repair the same frozen transition before publishing.
    let vault = p_vault.get_vault(h.a.vault_name.clone()).await?;
    let log = h.log().await?;
    if vault.0.value.members().len() == 2 {
        let results = log
            .get("liveboardRemovalResults")
            .and_then(Value::as_array)
            .context("completed transition needs its exact retained result")?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["applicationState"], "applied");
        assert_eq!(
            results[0]["responderId"],
            serde_json::to_value(h.c.device_id())?
        );
    }
    Ok(())
}
