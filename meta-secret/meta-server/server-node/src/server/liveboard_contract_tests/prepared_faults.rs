use super::*;
use async_trait::async_trait;
use meta_secret_core::node::common::model::{meta_pass::MetaPasswordId, vault::vault::VaultMember};
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Phase {
    Base,
    Prepared,
    Target,
    Status,
    Claims,
    Cleanup,
    Applied,
    RawSource,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum FaultKind {
    Save,
    Read,
}
pub(super) struct PhaseRepo {
    inner: Arc<InMemKvLogEventRepo>,
    fault: Mutex<Option<(Phase, FaultKind)>>,
    pub(super) hits: AtomicUsize,
    prepared_saved: AtomicBool,
    cleanup_saved: AtomicBool,
}
impl PhaseRepo {
    pub(super) fn new(inner: Arc<InMemKvLogEventRepo>) -> Self {
        Self {
            inner,
            fault: Mutex::new(None),
            hits: AtomicUsize::new(0),
            prepared_saved: AtomicBool::new(false),
            cleanup_saved: AtomicBool::new(false),
        }
    }
    pub(super) fn arm(&self, phase: Phase, kind: FaultKind) {
        *self.fault.lock().unwrap() = Some((phase, kind));
    }
    pub(super) fn clear(&self) {
        *self.fault.lock().unwrap() = None;
    }
    fn fail(&self, event: &GenericKvLogEvent, kind: FaultKind) -> Result<()> {
        let fault = *self.fault.lock().unwrap();
        if let Some((phase, wanted)) = fault {
            let matches = match phase {
                Phase::Base => {
                    matches!(event, GenericKvLogEvent::Vault(vault) if vault.0.value.members().len() == 3 && self.prepared_saved.load(Ordering::SeqCst))
                }
                Phase::Prepared => {
                    matches!(event, GenericKvLogEvent::VaultLog(log) if !log.0.value.liveboard_removal_applications.is_empty())
                }
                Phase::Applied => {
                    matches!(event, GenericKvLogEvent::VaultLog(log) if log.0.value.liveboard_removal_applications.is_empty() && !log.0.value.liveboard_removal_results.is_empty())
                }
                Phase::Target => {
                    matches!(event, GenericKvLogEvent::Vault(vault) if vault.0.value.members().len() == 2)
                }
                Phase::Status => {
                    matches!(event, GenericKvLogEvent::VaultStatus(status) if !status.is_member())
                }
                Phase::Claims => {
                    matches!(event, GenericKvLogEvent::SsLog(log) if log.0.value.claims.values().any(|claim| claim.status.statuses.values().any(|status| *status == meta_secret_core::node::common::model::secret::SsDistributionStatus::Declined)))
                }
                Phase::RawSource => matches!(event, GenericKvLogEvent::DeviceLog(_)),
                Phase::Cleanup => {
                    kind == FaultKind::Read && self.cleanup_saved.load(Ordering::SeqCst)
                }
            };
            if wanted == kind && matches {
                self.hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("injected prepared removal phase failure");
            }
        }
        Ok(())
    }
}
#[async_trait(?Send)]
impl SaveCommand for PhaseRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        self.fail(&event, FaultKind::Save)?;
        let prepared = matches!(&event, GenericKvLogEvent::VaultLog(log) if !log.0.value.liveboard_removal_applications.is_empty());
        let id = self.inner.save(event).await?;
        if prepared {
            self.prepared_saved.store(true, Ordering::SeqCst);
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        if *self.fault.lock().unwrap() == Some((Phase::Cleanup, FaultKind::Save)) {
            self.hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("injected cleanup replace failure");
        }
        let id = self.inner.replace(value).await?;
        self.cleanup_saved.store(true, Ordering::SeqCst);
        Ok(id)
    }
}
#[async_trait(?Send)]
impl FindOneQuery for PhaseRepo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let value = self.inner.find_one(id).await?;
        if let Some(event) = &value {
            self.fail(event, FaultKind::Read)?;
        }
        Ok(value)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|event| event.obj_id()))
    }
}
#[async_trait(?Send)]
impl DeleteCommand for PhaseRepo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for PhaseRepo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for PhaseRepo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.inner.server_recovery_response_store()
    }
}

async fn seed_pending_claim(h: &Harness) -> Result<()> {
    let p = PersistentVault::from(h.p_obj.clone());
    let mut vault = p.get_vault(h.a.vault_name.clone()).await?;
    let pass = MetaPasswordId::build_from_str("phase-recovery");
    vault.0.value = vault.0.value.add_secret(pass.clone());
    h.p_obj.repo.replace(vault.clone()).await?;
    let member = VaultMember {
        member: member(&h.a),
        vault: vault.0.value,
    };
    PersistentSharedSecret::from(h.p_obj.clone())
        .save_ss_log_event(member.create_recovery_claim(pass))
        .await?;
    Ok(())
}

async fn phase_case(phase: Phase, kind: FaultKind) -> Result<()> {
    let h = Harness::new().await?;
    seed_pending_claim(&h).await?;
    let mut request = h.removal();
    if phase == Phase::Cleanup {
        request.share_transfers.push(
            meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberShareTransfer {
                pass_id: MetaPasswordId::build_from_str("phase-deleted-history"),
                receiver: h.c.device_id().clone(),
                claim_id: None,
                encrypted_share: "synthetic-test-transfer".into(),
            },
        );
    }
    h.send_action(
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request.clone())),
    )
    .await?;
    if phase == Phase::Cleanup {
        // Model a durable tombstone whose sensitive-history cleanup was
        // interrupted. Inserting it after request persistence keeps the exact
        // original request pending until this removal's repair performs cleanup.
        let mut vault = PersistentVault::from(h.p_obj.clone())
            .get_vault(h.a.vault_name.clone())
            .await?;
        vault.0.value = vault
            .0
            .value
            .delete_secret(MetaPasswordId::build_from_str("phase-deleted-history"));
        h.p_obj.repo.replace(vault).await?;
    }
    let id = h.pending_id().await?;
    let mut update = h.decision(true);
    update.request = request;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
    )
    .await?;
    let action = guarded(&h.c, &event, &id, 1)?;
    let repo = Arc::new(PhaseRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    repo.arm(phase, kind);
    assert!(
        send(&server, action.clone()).await.is_err(),
        "storage fault cannot acknowledge a successful source command"
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "the requested real persistence phase was not reached"
    );
    repo.clear();
    let reopened = ServerApp::new(repo.clone(), h.master_key.clone())?;
    refresh(&reopened, &h.a).await?;
    // A failure before preparation keeps the exact request pending; a committed
    // preparation must repair. In both cases replaying THIS authenticated source
    // item must eventually advance its own tail without a new command/nonce.
    send(&reopened, action)
        .await
        .context("RED: canonical result committed but exact source retry was stranded")?;
    let vault = PersistentVault::from(h.p_obj.clone())
        .get_vault(h.a.vault_name.clone())
        .await?;
    assert_eq!(vault.0.value.members().len(), 2);
    let log = h.log().await?;
    let result = result_record(&log, &id)?;
    assert_eq!(result["applicationState"], "applied");
    assert_eq!(
        result["responderId"],
        serde_json::to_value(h.c.device_id())?
    );
    assert!(
        log.get("liveboardRemovalApplications")
            .is_none_or(|items| items.as_array().is_some_and(Vec::is_empty))
    );
    assert!(
        h.p_obj.repo.find_one(event.obj_id()).await?.is_some(),
        "durable source tail must retire the same signed queue item"
    );
    Ok(())
}

macro_rules! phase_test {
    ($name:ident, $phase:ident, $kind:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            phase_case(Phase::$phase, FaultKind::$kind).await
        }
    };
}
phase_test!(
    liveboard_prepared_save_fault_reopens_without_false_outcome,
    Prepared,
    Save
);
phase_test!(
    liveboard_base_read_fault_after_preparation_reopens_same_transition,
    Base,
    Read
);
phase_test!(
    liveboard_cleanup_replace_fault_reopens_same_transition,
    Cleanup,
    Save
);
phase_test!(
    liveboard_cleanup_readback_fault_reopens_same_transition,
    Cleanup,
    Read
);
phase_test!(
    liveboard_prepared_readback_fault_reopens_same_winner,
    Prepared,
    Read
);
phase_test!(
    liveboard_target_save_fault_repairs_same_transition,
    Target,
    Save
);
phase_test!(
    liveboard_target_readback_fault_repairs_same_transition,
    Target,
    Read
);
phase_test!(
    liveboard_status_save_fault_repairs_same_transition,
    Status,
    Save
);
phase_test!(
    liveboard_status_readback_fault_repairs_same_transition,
    Status,
    Read
);
phase_test!(
    liveboard_claims_save_fault_repairs_same_transition,
    Claims,
    Save
);
phase_test!(
    liveboard_claims_readback_fault_repairs_same_transition,
    Claims,
    Read
);
phase_test!(
    liveboard_final_save_fault_repairs_same_transition,
    Applied,
    Save
);
phase_test!(
    liveboard_final_readback_fault_repairs_same_transition,
    Applied,
    Read
);

#[tokio::test]
async fn liveboard_canonical_commit_then_raw_source_failure_retries_exact_envelope_only()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let id = h.pending_id().await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(true))),
    )
    .await?;
    let original = guarded(&h.c, &event, &id, 1)?;
    let repo = Arc::new(PhaseRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    repo.arm(Phase::RawSource, FaultKind::Save);
    assert!(send(&server, original.clone()).await.is_err());
    assert!(repo.hits.load(Ordering::SeqCst) > 0);
    repo.clear();
    let reopened = ServerApp::new(repo, h.master_key.clone())?;
    refresh(&reopened, &h.a).await?;
    let canonical_before = h.p_obj.repo.get_db().await;
    let mut opposite = event.clone();
    opposite.0.value =
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false)));
    assert!(
        send(&reopened, guarded(&h.c, &opposite, &id, 1)?)
            .await
            .is_err(),
        "terminal UUID does not authorize a different decision body"
    );
    assert!(
        h.p_obj.repo.get_db().await == canonical_before,
        "opposite reuse cannot mutate membership or canonical winner"
    );
    send(&reopened, original).await.context("RED: authenticated identical command cannot finish its own durable source tail after canonical commit")?;
    let log = h.log().await?;
    assert_eq!(result_record(&log, &id)?["outcome"], "accepted");
    assert!(h.p_obj.repo.find_one(event.obj_id()).await?.is_some());
    let settled = h.p_obj.repo.get_db().await;
    let immutable = |value: &&GenericKvLogEvent| {
        matches!(
            value,
            GenericKvLogEvent::Vault(_)
                | GenericKvLogEvent::SsLog(_)
                | GenericKvLogEvent::SsWorkflow(_)
        )
    };
    assert!(
        settled.values().filter(immutable).count()
            == canonical_before.values().filter(immutable).count()
            && canonical_before
                .iter()
                .filter(|(_, value)| immutable(value))
                .all(|(key, value)| settled.get(key) == Some(value)),
        "source acknowledgement must not create another membership/share/recovery generation"
    );
    let tail = reopened
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            meta_secret_core::node::api::ServerTailRequest {
                producer_reference: None,
                sender: h.c.user(),
            },
        ))))
        .await?
        .to_server_tail()?;
    assert!(
        tail.device_log_tail == Some(event.obj_id().next()),
        "source tail must advance past exactly the acknowledged item"
    );
    assert!(
        send(&reopened, guarded(&h.c, &opposite, &id, 1)?)
            .await
            .is_err()
    );
    assert!(h.p_obj.repo.get_db().await == settled);
    Ok(())
}
