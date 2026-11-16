//! G3 real repository faults only; no successful result or handler is substituted.
use super::*;
use async_trait::async_trait;
use meta_secret_core::node::{
    api::{ServerTailRequest, ServerTailResponse},
    common::model::{
        meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
        secret::SsDistributionId,
        vault::vault::VaultMember,
    },
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::generic_log_event::GenericKvLogEvent,
        repo::{
            generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery},
            recovery_response_store::ServerRecoveryResponseStore,
        },
    },
};
use meta_secret_core::secret::MetaDistributor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Default)]
pub(super) struct QueryBarrier {
    target: Mutex<Option<ArtifactId>>,
    entered: Notify,
    release: Notify,
}
impl QueryBarrier {
    pub fn arm(&self, id: ArtifactId) {
        *self.target.lock().unwrap() = Some(id);
    }
    pub async fn wait(&self) {
        self.entered.notified().await;
    }
    pub fn release(&self) {
        self.release.notify_one();
    }
    async fn after_read(&self, id: &ArtifactId) {
        let pause = {
            let mut target = self.target.lock().unwrap();
            if target.as_ref() == Some(id) {
                target.take();
                true
            } else {
                false
            }
        };
        if pause {
            self.entered.notify_one();
            self.release.notified().await;
        }
    }
}

pub(super) struct FaultRepo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    mode: AtomicU8,
    pub hits: AtomicUsize,
    pub query_barrier: QueryBarrier,
}
impl<R: KvLogEventRepo> FaultRepo<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            mode: AtomicU8::new(0),
            hits: AtomicUsize::new(0),
            query_barrier: QueryBarrier::default(),
        }
    }
    pub fn before_target(&self) {
        self.mode.store(1, Ordering::SeqCst);
    }
    pub fn before_raw(&self) {
        self.mode.store(2, Ordering::SeqCst);
    }
    fn take(&self, mode: u8) -> bool {
        if self
            .mode
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for FaultRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let result = self.inner.find_one(id.clone()).await?;
        self.query_barrier.after_read(&id).await;
        Ok(result)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for FaultRepo<R> {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        if matches!(&event, GenericKvLogEvent::Vault(v) if !v.0.value.deletion_results.is_empty())
            && self.take(1)
        {
            anyhow::bail!("injected automatic target failure PRIVATE-BACKEND-ERROR");
        }
        if matches!(event, GenericKvLogEvent::DeviceLog(_)) && self.take(2) {
            anyhow::bail!("injected raw save failure PRIVATE-BACKEND-ERROR");
        }
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for FaultRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for FaultRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await;
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for FaultRepo<R> {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}

pub(super) struct Seed<R: KvLogEventRepo> {
    pub repo: Arc<R>,
    pub p: Arc<PersistentObject<R>>,
    pub actor: UserCreds,
    pub master: TransportSk,
    pub pass: MetaPasswordId,
    pub workflows: Vec<ArtifactId>,
}
impl<R: KvLogEventRepo> Seed<R> {
    pub async fn new(repo: Arc<R>, count: usize) -> Result<Self> {
        let f = FixtureRegistry::empty().state;
        let actor = f.user_creds.client;
        let peer = f.user_creds.client_b;
        let third = f.user_creds.vd;
        let p = Arc::new(PersistentObject::new(repo.clone()));
        let pass = MetaPasswordId::build_from_str("automatic-delete-real-shares");
        let mut vault = VaultObject::sign_up(actor.vault_name.clone(), member(&actor));
        if count >= 2 {
            vault.0.value = vault
                .0
                .value
                .update_membership(UserMembership::Member(member(&peer)));
        }
        if count >= 3 {
            vault.0.value = vault
                .0
                .value
                .update_membership(UserMembership::Member(member(&third)));
        }
        vault.0.value = vault.0.value.add_secret(pass.clone());
        let vm = VaultMember {
            member: member(&actor),
            vault: vault.clone().to_data(),
        };
        repo.save(vault.clone()).await?;
        repo.save(VaultLogObject::create(member(&actor))).await?;
        MetaDistributor {
            p_obj: p.clone(),
            user_creds: Arc::new(actor.clone()),
            vault_member: vm.clone(),
        }
        .distribute(
            vm.clone(),
            SecurePassInfo::from(PlainPassInfo {
                pass_id: pass.clone(),
                pass: "SYNTHETIC-PRIVATE-DELETE-SECRET".into(),
            }),
        )
        .await?;
        let mut workflows = Vec::new();
        for current in vm.vault.members() {
            let obj = p
                .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: pass.clone(),
                    receiver: current.user_data.device.device_id.clone(),
                }))
                .await?
                .context("actual Encrypted Key Share")?;
            workflows.push(obj.obj_id());
        }
        assert_eq!(workflows.len(), count);
        // Real distribution has already persisted encrypted work; no domain outcome is mocked.
        Ok(Self {
            repo,
            p,
            actor,
            master: f.device_creds.server_master_key,
            pass,
            workflows,
        })
    }
    pub async fn server(&self) -> Result<ServerApp<R>> {
        let server = ServerApp::new(self.repo.clone(), self.master.clone())?;
        server.init().await?;
        Ok(server)
    }
    pub async fn log(&self) -> Result<VaultLogObject> {
        PersistentVault::from(self.p.clone())
            .get_vault_log_artifact(self.actor.vault_name.clone())
            .await
    }
    pub async fn deletion(&self) -> Result<(DeviceLogObject, SignedAction, String)> {
        use meta_secret_core::node::db::events::vault::vault_log_event::DeleteSecretRequest;
        let vault = PersistentVault::from(self.p.clone())
            .get_vault(self.actor.vault_name.clone())
            .await?
            .to_data();
        let request = DeleteSecretRequest::new(member(&self.actor), self.pass.clone(), &vault);
        let id = request.request_id.clone();
        let source = next_event(
            &self.p,
            &self.actor,
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)),
        )
        .await?;
        let signed = sign_event(
            source.clone().to_generic(),
            self.actor.device_id().clone(),
            &self.actor.device_creds.key_manager()?.dsa,
        )?;
        Ok((source, signed, id))
    }
}
pub(super) async fn tail<R: KvLogEventRepo>(
    server: &ServerApp<R>,
    actor: &UserCreds,
) -> Result<ServerTailResponse> {
    server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                producer_reference: None,
                sender: actor.user(),
            },
        ))))
        .await?
        .to_server_tail()
}
