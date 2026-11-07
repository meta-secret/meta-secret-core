use super::*;
use async_trait::async_trait;
use std::{collections::HashSet, sync::Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Point {
    Index,
    Witness,
    Origin,
    Queued,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fault {
    Before,
    Drop,
    Readback,
    LostReply,
}
#[derive(Clone, Copy)]
pub struct Plan {
    pub point: Point,
    pub fault: Fault,
}
#[derive(Clone)]
pub enum Trace {
    Read(ArtifactId),
    Write(bool, Box<GenericKvLogEvent>),
}
type BeforeBodySnapshot = (
    GenericKvLogEvent,
    Option<GenericKvLogEvent>,
    Option<GenericKvLogEvent>,
);
pub struct Repo<R: KvLogEventRepo> {
    pub inner: Arc<R>,
    plan: Mutex<Option<Plan>>,
    read_fault: Mutex<Option<ArtifactId>>,
    keys: Mutex<HashSet<ArtifactId>>,
    pub trace: Mutex<Vec<Trace>>,
    pub before_body: Mutex<Vec<BeforeBodySnapshot>>,
    pub hits: std::sync::atomic::AtomicUsize,
    pub read_hits: std::sync::atomic::AtomicUsize,
    owner: meta_secret_core::node::common::model::user::common::UserId,
    uuid: String,
}
impl<R: KvLogEventRepo> Repo<R> {
    pub fn new(
        inner: Arc<R>,
        owner: meta_secret_core::node::common::model::user::common::UserId,
        uuid: String,
    ) -> Self {
        Self {
            inner,
            owner,
            uuid,
            plan: Mutex::new(None),
            read_fault: Mutex::new(None),
            keys: Mutex::new(HashSet::new()),
            trace: Mutex::new(vec![]),
            before_body: Mutex::new(vec![]),
            hits: 0.into(),
            read_hits: 0.into(),
        }
    }
    pub fn arm(&self, point: Point, fault: Fault) {
        *self.plan.lock().unwrap() = Some(Plan { point, fault });
        self.hits.store(0, Ordering::SeqCst);
        self.read_hits.store(0, Ordering::SeqCst);
    }
    pub fn remember(&self, keys: impl IntoIterator<Item = ArtifactId>) {
        self.keys.lock().unwrap().extend(keys);
    }
    pub fn clear(&self) {
        *self.plan.lock().unwrap() = None;
        *self.read_fault.lock().unwrap() = None;
    }
    pub async fn snapshot(&self) -> Result<super::super::fixture::Snapshot> {
        let keys: Vec<_> = self.keys.lock().unwrap().iter().cloned().collect();
        let mut out = std::collections::HashMap::new();
        for id in keys {
            if let Some(v) = self.inner.find_one(id.clone()).await? {
                out.insert(id, v);
            }
        }
        Ok(out)
    }
    pub async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        let id = event.obj_id();
        self.keys.lock().unwrap().insert(id.clone());
        self.trace
            .lock()
            .unwrap()
            .push(Trace::Write(replace, Box::new(event.clone())));
        let own_stream = ArtifactId::from(
            meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor::from(
                self.owner.clone(),
            ),
        )
        .fqdn;
        if matches!(&event, GenericKvLogEvent::DeviceLog(body) if body.obj_id().fqdn==own_stream) {
            let index = self
                .inner
                .find_one(ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
                    owner: self.owner.clone(),
                }))
                .await?;
            let witness = self
                .inner
                .find_one(ArtifactId::from(LocalGuardedPreparationDescriptor {
                    owner: self.owner.clone(),
                    request_id: self.uuid.clone(),
                }))
                .await?;
            self.before_body
                .lock()
                .unwrap()
                .push((event.clone(), index, witness));
        }
        let point = match &event {
            GenericKvLogEvent::LocalGuardedPreparationIndex(x)
                if replace && x.0.value.revision > 0 =>
            {
                Some(Point::Index)
            }
            GenericKvLogEvent::LocalGuardedPreparation(_) if !replace => Some(Point::Witness),
            GenericKvLogEvent::LocalGuardedPreparation(x)
                if x.0.value.phase == GuardedPreparationPhase::Queued =>
            {
                Some(Point::Queued)
            }
            GenericKvLogEvent::LocalGuardedPreparation(x) if x.0.value.origin.is_some() => {
                Some(Point::Origin)
            }
            _ => None,
        };
        let plan = *self.plan.lock().unwrap();
        let selected = plan.filter(|p| Some(p.point) == point);
        if let Some(p) = selected {
            self.hits.fetch_add(1, Ordering::SeqCst);
            if p.fault == Fault::Before {
                anyhow::bail!("C1b injected private-store unsupported/before PRIVATE_SENTINEL");
            }
            if p.fault == Fault::Drop {
                return Ok(id);
            }
        }
        let saved = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if let Some(p) = selected {
            if p.fault == Fault::LostReply {
                anyhow::bail!("C1b injected private-store lost reply PRIVATE_SENTINEL");
            }
            if p.fault == Fault::Readback {
                *self.read_fault.lock().unwrap() = Some(id);
            }
        }
        Ok(saved)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Repo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.trace.lock().unwrap().push(Trace::Read(id.clone()));
        if self.read_fault.lock().unwrap().as_ref() == Some(&id) {
            self.read_hits.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("C1b injected private readback PRIVATE_SENTINEL");
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|x| x.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Repo<R> {
    async fn save<T: ToGenericEvent>(&self, x: T) -> Result<ArtifactId> {
        self.write(false, x.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, x: T) -> Result<ArtifactId> {
        self.write(true, x.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Repo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for Repo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Repo<R> {}

/// A real trait-default unsupported atomic replace, not a supportsReplace flag.
pub struct Unsupported<R: KvLogEventRepo>(pub Arc<R>);
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for Unsupported<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.0.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.0.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for Unsupported<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.0.save(event).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for Unsupported<R> {
    async fn delete(&self, id: ArtifactId) {
        self.0.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.0.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for Unsupported<R> {
    async fn db_clean_up(&self) {
        self.0.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for Unsupported<R> {}
