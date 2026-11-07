use super::*;
use async_trait::async_trait;

/// Actual SQLite; every attempted material write is captured before forwarding.
pub(super) struct Repo {
    pub inner: Arc<SqlIteRepo>,
    pub writes: Mutex<Vec<GenericKvLogEvent>>,
    plan: Mutex<Option<(Identity, DeviceId)>>,
    pub hits: AtomicUsize,
    pub durable_three: AtomicBool,
}
impl Repo {
    pub fn open(url: String) -> Self {
        Self {
            inner: Arc::new(SqlIteRepo { conn_url: url }),
            writes: Mutex::new(vec![]),
            plan: Mutex::new(None),
            hits: 0.into(),
            durable_three: false.into(),
        }
    }
    pub fn arm(&self, identity: Identity, joined: DeviceId) {
        *self.plan.lock().unwrap() = Some((identity, joined));
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        self.writes.lock().unwrap().push(event.clone());
        let plan = self.plan.lock().unwrap().clone();
        let selected = match (&plan, &event) {
            (Some((_, joined)), GenericKvLogEvent::Vault(vault)) => {
                vault.0.value.members().len() == 3 && vault.0.value.is_member(joined)
            }
            _ => false,
        };
        let prior = if selected {
            let GenericKvLogEvent::Vault(vault) = &event else {
                unreachable!()
            };
            Some(
                PersistentVault::from(Arc::new(PersistentObject::new(self.inner.clone())))
                    .get_vault(vault.0.value.vault_name.clone())
                    .await?,
            )
        } else {
            None
        };
        let id = if replace {
            self.inner.replace(event.clone()).await?
        } else {
            self.inner.save(event.clone()).await?
        };
        if let (Some((identity, joined)), Some(prior)) = (plan, prior) {
            let GenericKvLogEvent::Vault(vault) = &event else {
                unreachable!()
            };
            let actual = PersistentVault::from(Arc::new(PersistentObject::new(self.inner.clone())))
                .get_vault(vault.0.value.vault_name.clone())
                .await?;
            ensure!(
                prior.0.value.members().len() == 2
                    && !prior.0.value.is_member(&joined)
                    && self.inner.find_one(id.clone()).await? == Some(event)
                    && actual.0.value.members().len() == 3
                    && actual.0.value.is_member(&joined),
                "Vault-save fault must follow actual durable two-to-three import"
            );
            self.durable_three.store(true, Ordering::SeqCst);
            self.hits.fetch_add(1, Ordering::SeqCst);
            self.plan.lock().unwrap().take();
            return Err(identity.error()); // Real write committed; only its reply is lost.
        }
        Ok(id)
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {}
