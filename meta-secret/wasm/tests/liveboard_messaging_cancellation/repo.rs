use super::{fixtures, transport::Protocol};
use anyhow::{Context, Result};
use async_trait::async_trait;
use meta_secret_core::node::{
    app::{meta_app::meta_client_service::MetaClientService, sync::sync_gateway::SyncGateway},
    db::{
        events::{generic_log_event::*, object_id::ArtifactId, vault::vault_log_event::*},
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_object::PersistentObject,
        repo::generic_db::*,
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};
use tokio::sync::Notify;

#[derive(Default)]
pub struct Barrier {
    mode: AtomicU8,
    entered: Notify,
    release: Notify,
    id: Mutex<Option<ArtifactId>>,
    pub hits: AtomicUsize,
}
impl Barrier {
    pub fn arm_save(&self) {
        self.mode.store(1, Ordering::SeqCst);
    }
    pub fn arm_read(&self) {
        self.mode.store(2, Ordering::SeqCst);
    }
    pub async fn wait(&self) {
        self.entered.notified().await;
    }
    pub fn release(&self) {
        self.release.notify_one();
    }
    pub fn id(&self) -> Result<ArtifactId> {
        self.id
            .lock()
            .unwrap()
            .clone()
            .context("actual repository boundary was not reached")
    }
    async fn pause(&self, mode: u8, id: ArtifactId) {
        if self
            .mode
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        *self.id.lock().unwrap() = Some(id);
        self.hits.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
    }
}

#[derive(Default)]
pub struct Repo {
    inner: InMemKvLogEventRepo,
    pub barrier: Barrier,
    creates: AtomicUsize,
    changed: Notify,
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let value = self.inner.find_one(id.clone()).await?;
        if matches!(value, Some(GenericKvLogEvent::Vault(_))) {
            self.barrier.pause(2, id).await;
        }
        Ok(value)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let value = value.to_generic();
        let create = matches!(&value, GenericKvLogEvent::DeviceLog(event)
            if matches!(event.0.value, VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(_))));
        let id = self.inner.save(value).await?;
        if create {
            self.creates.fetch_add(1, Ordering::SeqCst);
            self.changed.notify_one();
            self.barrier.pause(1, id.clone()).await;
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await;
    }
}
impl KvLogEventRepo for Repo {}

pub struct Harness {
    pub p: Arc<PersistentObject<Repo>>,
    pub service: Arc<MetaClientService<Repo, Protocol>>,
    pub web: meta_secret_wasm::app_manager::ApplicationManager<Repo, Protocol>,
    pub mobile: mobile_common::app_manager::ApplicationManager<Repo, Protocol>,
}
impl Harness {
    pub async fn new() -> Result<Self> {
        let seeded = fixtures::Harness::new(1).await?;
        let repo = Arc::new(Repo::default());
        for value in seeded.p.repo.get_db().await.into_values() {
            repo.save(value).await?;
        }
        let p = Arc::new(PersistentObject::new(repo));
        let gateway = Arc::new(SyncGateway::new(
            "cancellation-fixture".into(),
            p.clone(),
            seeded.protocol.clone(),
            seeded.master.clone(),
        ));
        let source = seeded.service();
        let service = Arc::new(MetaClientService {
            data_transfer: source.data_transfer,
            sync_gateway: gateway.clone(),
            state_provider: source.state_provider,
            p_obj: p.clone(),
            device_data: source.device_data,
            master_key: seeded.master.clone(),
        });
        let web = meta_secret_wasm::app_manager::ApplicationManager::new(
            seeded.protocol.clone(),
            gateway.clone(),
            service.clone(),
            seeded.master.clone(),
        );
        let mobile = mobile_common::app_manager::ApplicationManager::new(
            seeded.protocol.clone(),
            gateway,
            service.clone(),
            seeded.master,
        );
        Ok(Self {
            p,
            service,
            web,
            mobile,
        })
    }
    pub async fn assert_boundary(&self) -> Result<()> {
        let id = self.p.repo.barrier.id()?;
        assert!(
            self.p.repo.inner.find_one(id).await?.is_some(),
            "barrier must follow an actual saved/read record"
        );
        assert_eq!(self.p.repo.barrier.hits.load(Ordering::SeqCst), 1);
        Ok(())
    }
    pub async fn wait_creates(&self, count: usize) {
        while self.p.repo.creates.load(Ordering::SeqCst) < count {
            self.p.repo.changed.notified().await;
        }
    }
    pub async fn added(&self) -> BTreeMap<String, String> {
        self.p
            .repo
            .inner
            .get_db()
            .await
            .into_values()
            .filter_map(|event| match event {
                GenericKvLogEvent::DeviceLog(event) => match event.0.value {
                    VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add)) => {
                        Some((add.meta_pass_id.name.clone(), add.meta_pass_id.id_str()))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
}
