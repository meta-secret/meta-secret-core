//! Canonical-view faults preserve the actual produced identity; no crypto outcome is mocked.
use super::{fault_repo, fixtures::Harness, transport::Protocol};
use anyhow::{Result, bail};
use async_trait::async_trait;
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::{
    api::*,
    app::{
        meta_app::meta_client_service::MetaClientService,
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::{
        meta_pass::MetaPasswordId,
        secret::{ClaimId, SecretDistributionType, SsClaim, SsDistributionStatus},
        vault::vault::VaultName,
    },
    db::{
        events::{generic_log_event::*, object_id::ArtifactId},
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
        },
        repo::generic_db::*,
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
pub enum Echo {
    Exact,
    Missing,
    WrongSender,
    WrongVault,
    WrongKind,
    WrongPass,
    WrongId,
    ReadFailure,
    Accepted,
    Declined,
    Done,
}
pub struct Repo {
    pub inner: Arc<fault_repo::Repo>,
    pub sync_owner: Mutex<Option<Arc<async_lock::Mutex<()>>>>,
    pub read_fault: AtomicBool,
    pub hits: AtomicUsize,
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let outside_sync = self
            .sync_owner
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|owner| owner.try_lock().is_some());
        if id.fqdn.obj_type == "SsLog"
            && outside_sync
            && self.read_fault.swap(false, Ordering::SeqCst)
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("PRIVATE-CANONICAL-READ-ERROR");
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.inner.replace(event).await
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

pub struct EchoProtocol {
    pub p: Arc<PersistentObject<Repo>>,
    pub base: Arc<Protocol>,
    pub mode: Echo,
    pub seen: Mutex<Option<SsClaim>>,
    pub echoed: Mutex<Option<SsClaim>>,
    pub requests: AtomicUsize,
}
impl SyncProtocol for EchoProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if let SyncRequest::Write(write) = &request {
            let WriteSyncRequest::Event(action) = write.as_ref() else {
                anyhow::bail!("UnsupportedNoop in legacy test protocol");
            };
            if let GenericKvLogEvent::SsDeviceLog(event) = action.event()?
                && event.0.value.distribution_type == SecretDistributionType::Recover
            {
                *self.seen.lock().unwrap() = Some(event.0.value.clone());
            }
        }
        if let SyncRequest::Read(read) = &request
            && let ReadSyncRequest::SsRequest(input) = read.as_ref()
        {
            self.requests.fetch_add(1, Ordering::SeqCst);
            let seen = self.seen.lock().unwrap().clone();
            if let Some(mut claim) = seen
                && !matches!(self.mode, Echo::Missing)
            {
                match self.mode {
                    Echo::WrongSender => {
                        claim.sender = claim.receivers[0].clone();
                    }
                    Echo::WrongVault => {
                        claim.vault_name = VaultName("wrong-confirmation-vault".into());
                    }
                    Echo::WrongKind => {
                        claim.distribution_type = SecretDistributionType::Split;
                    }
                    Echo::WrongPass => {
                        claim.dist_claim_id.pass_id =
                            MetaPasswordId::build_from_str("wrong-confirmation-pass");
                    }
                    Echo::WrongId => {
                        claim.id = ClaimId(Id48bit::generate());
                        claim.dist_claim_id.id = claim.id.clone();
                    }
                    Echo::Accepted => {
                        for status in claim.status.statuses.values_mut() {
                            *status = SsDistributionStatus::Declined;
                        }
                        claim
                            .status
                            .statuses
                            .insert(claim.receivers[0].clone(), SsDistributionStatus::Sent);
                    }
                    Echo::Declined => {
                        for status in claim.status.statuses.values_mut() {
                            *status = SsDistributionStatus::Declined;
                        }
                    }
                    Echo::Done => {
                        claim
                            .status
                            .statuses
                            .insert(claim.sender.clone(), SsDistributionStatus::Delivered);
                    }
                    _ => {}
                }
                let ss = PersistentSharedSecret::from(self.p.clone());
                let mut view = ss.get_ss_log_obj(input.sender.vault_name.clone()).await?;
                view.claims.insert(claim.id.clone(), claim.clone());
                let obj = ss
                    .create_new_ss_log_object(view, input.sender.vault_name.clone())
                    .await?;
                *self.echoed.lock().unwrap() = Some(claim);
                if matches!(self.mode, Echo::ReadFailure) {
                    self.p.repo.read_fault.store(true, Ordering::SeqCst);
                }
                return Ok(DataSyncResponse::Data(DataEventsResponse(vec![
                    obj.to_generic(),
                ])));
            }
        }
        self.base.send(request).await
    }
}
pub fn service(h: &Harness, mode: Echo) -> MetaClientService<Repo, EchoProtocol> {
    let repo = Arc::new(Repo {
        inner: h.p.repo.clone(),
        sync_owner: Mutex::new(None),
        read_fault: AtomicBool::new(false),
        hits: AtomicUsize::new(0),
    });
    let p = Arc::new(PersistentObject::new(repo));
    *p.repo.sync_owner.lock().unwrap() = Some(p.device_sync_guard.clone());
    let protocol = Arc::new(EchoProtocol {
        p: p.clone(),
        base: h.protocol.clone(),
        mode,
        seen: Mutex::new(None),
        echoed: Mutex::new(None),
        requests: AtomicUsize::new(0),
    });
    let original = h.service();
    MetaClientService {
        data_transfer: original.data_transfer,
        sync_gateway: Arc::new(SyncGateway::new(
            "checked-confirmation-fixture".into(),
            p.clone(),
            protocol,
            h.master.clone(),
        )),
        state_provider: original.state_provider,
        p_obj: p,
        device_data: h.approver.device(),
        master_key: h.master.clone(),
    }
}
