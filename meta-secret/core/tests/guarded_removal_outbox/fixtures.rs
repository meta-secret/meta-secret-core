use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_secret_core::node::api::*;
use meta_secret_core::node::app::sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use meta_secret_core::node::common::model::user::{common::*, user_creds::UserCreds};
use meta_secret_core::node::common::model::vault::vault::{VaultMember, VaultStatus};
use meta_secret_core::node::db::actions::sign_up::remove::{RemoveAction, RemoveActionUpdate};
use meta_secret_core::node::db::descriptors::{
    local_guarded_preparation::{
        LocalGuardedPreparationDescriptor, LocalGuardedPreparationIndexDescriptor,
    },
    local_removal_envelope::LocalRemovalEnvelopeDescriptor,
    vault_descriptor::{DeviceLogDescriptor, VaultStatusDescriptor},
};
use meta_secret_core::node::db::events::{
    generic_log_event::*,
    kv_log_event::*,
    local_guarded_preparation::{
        LocalGuardedPreparationIndexObject, LocalGuardedPreparationObject,
    },
    local_removal_envelope::*,
    object_id::*,
    vault::{
        device_log_event::DeviceLogObject, liveboard_metadata::PendingRemovalBinding,
        vault_event::VaultObject, vault_log_event::*, vault_status::VaultStatusObject,
    },
};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::{
    persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
};
use meta_secret_core::node::db::repo::{
    generic_db::*, persistent_credentials::PersistentCredentials,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
};

pub const REQUEST_ID: &str = "bb9ff326-178c-46e4-97ed-275d28e9db6b";

#[derive(Clone, Copy)]
pub enum Fault {
    None = 0,
    MarkerSave = 1,
    MarkerDrop = 2,
    MarkerReadback = 3,
    BodyResponseLost = 4,
    Delete = 5,
}
pub struct Repo {
    pub inner: Arc<InMemKvLogEventRepo>,
    pub mode: AtomicU8,
    pub hits: AtomicUsize,
    pub yield_reads: AtomicBool,
    marker_saved: AtomicBool,
    pub order: Mutex<Vec<&'static str>>,
}
impl Repo {
    pub fn arm(&self, fault: Fault) {
        self.mode.store(fault as u8, Ordering::SeqCst);
    }
    pub async fn snapshot(&self) -> std::collections::HashMap<ArtifactId, GenericKvLogEvent> {
        self.inner.get_db().await
    }
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let found = self.inner.find_one(id).await?;
        if self.yield_reads.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        if matches!(found, Some(GenericKvLogEvent::LocalRemovalEnvelope(_))) {
            self.order.lock().unwrap().push("marker-readback");
        }
        if matches!(found, Some(GenericKvLogEvent::LocalRemovalEnvelope(_)))
            && self.marker_saved.load(Ordering::SeqCst)
            && self.mode.load(Ordering::SeqCst) == Fault::MarkerReadback as u8
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("injected marker readback failure");
        }
        Ok(found)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|event| event.obj_id()))
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        let is_marker = matches!(event, GenericKvLogEvent::LocalRemovalEnvelope(_));
        let is_body = matches!(event, GenericKvLogEvent::DeviceLog(_));
        self.order.lock().unwrap().push(if is_marker {
            "marker"
        } else if is_body {
            "body"
        } else {
            "other"
        });
        if is_marker {
            match self.mode.load(Ordering::SeqCst) {
                1 | 5 => {
                    self.hits.fetch_add(1, Ordering::SeqCst);
                    bail!("injected marker/orphan ownership save failure");
                }
                2 => {
                    self.hits.fetch_add(1, Ordering::SeqCst);
                    return Ok(event.obj_id());
                }
                _ => {}
            }
        }
        let id = self.inner.save(event).await?;
        if is_marker {
            self.marker_saved.store(true, Ordering::SeqCst);
        }
        if is_body && self.mode.load(Ordering::SeqCst) == Fault::BodyResponseLost as u8 {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("injected lost body save response");
        }
        Ok(id)
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        let event = value.to_generic();
        if matches!(event, GenericKvLogEvent::LocalRemovalEnvelope(_))
            && self.mode.load(Ordering::SeqCst) == Fault::Delete as u8
        {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("injected orphan registry update failure");
        }
        self.inner.replace(event).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        if self.mode.load(Ordering::SeqCst) == Fault::Delete as u8 {
            self.hits.fetch_add(1, Ordering::SeqCst);
            bail!("injected orphan deletion failure");
        }
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

pub struct Protocol {
    pub capability: AtomicU8,
    pub probes: AtomicUsize,
    pub fail_next_write: AtomicBool,
    pub writes: Mutex<Vec<SignedAction>>,
}
impl Default for Protocol {
    fn default() -> Self {
        Self {
            capability: AtomicU8::new(1),
            probes: AtomicUsize::new(0),
            fail_next_write: AtomicBool::new(false),
            writes: Mutex::new(vec![]),
        }
    }
}
impl Protocol {
    pub fn enable_current_guarded(&self) {
        self.capability.store(3, Ordering::SeqCst);
    }
}
impl SyncProtocol for Protocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        match request {
            SyncRequest::Read(read) => match *read {
                ReadSyncRequest::LiveboardCapabilities(_) => {
                    self.probes.fetch_add(1, Ordering::SeqCst);
                    match self.capability.load(Ordering::SeqCst) {
                        mode @ (1 | 3) => Ok(DataSyncResponse::LiveboardCapabilities(
                            LiveboardCapabilities {
                                device_transport_noop_version: u32::from(mode == 3),
                                producer_correlation_version: 0,
                                projection_version: 1,
                                guarded_removal_version: 1,
                            },
                        )),
                        2 => bail!("injected capability transport outage"),
                        _ => Ok(DataSyncResponse::Error {
                            msg: "unsupported liveboard contract".into(),
                        }),
                    }
                }
                ReadSyncRequest::ServerTail(_) => Ok(DataSyncResponse::ServerTailResponse(
                    Box::new(ServerTailResponse {
                        producer_observation: None,
                        device_log_tail: None,
                        ss_device_log_tail: None,
                    }),
                )),
                ReadSyncRequest::Vault(_) | ReadSyncRequest::SsRequest(_) => {
                    Ok(DataSyncResponse::Data(DataEventsResponse(vec![])))
                }
                _ => Ok(DataSyncResponse::Empty),
            },
            SyncRequest::Write(write) => {
                let WriteSyncRequest::Event(action) = *write else {
                    bail!("UnsupportedNoop in legacy test protocol");
                };
                self.writes.lock().unwrap().push(action);
                if self.fail_next_write.swap(false, Ordering::SeqCst) {
                    bail!("injected transport response lost");
                }
                Ok(DataSyncResponse::Empty)
            }
        }
    }
}

pub struct Harness {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub actor: UserCreds,
    pub request: RemoveMemberRequestEvent,
    pub member: VaultMember,
    pub master_key: TransportSk,
    pub protocol: Arc<Protocol>,
}
impl Harness {
    pub async fn new() -> Result<Self> {
        let state = FixtureRegistry::empty().state;
        let a = state.user_creds.client;
        let b = state.user_creds.client_b;
        let actor = state.user_creds.vd;
        let repo = Arc::new(Repo {
            inner: Arc::new(InMemKvLogEventRepo::default()),
            mode: AtomicU8::new(0),
            hits: AtomicUsize::new(0),
            yield_reads: AtomicBool::new(false),
            marker_saved: AtomicBool::new(false),
            order: Mutex::new(vec![]),
        });
        let p_obj = Arc::new(PersistentObject::new(repo));
        let master_key = state.device_creds.vd_master_key;
        PersistentCredentials {
            p_obj: p_obj.clone(),
            master_key: master_key.clone(),
        }
        .save_user_creds(actor.clone())
        .await?;
        let mut vault = VaultObject::sign_up(a.vault_name.clone(), UserDataMember::from(a.user()));
        vault.0.value = vault
            .0
            .value
            .update_membership(UserMembership::Member(UserDataMember::from(b.user())))
            .update_membership(UserMembership::Member(UserDataMember::from(actor.user())));
        let member = VaultMember {
            member: UserDataMember::from(actor.user()),
            vault: vault.0.value.clone(),
        };
        p_obj.repo.save(vault).await?;
        p_obj
            .repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(member.member.clone()),
                ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
            ))
            .await?;
        let request = RemoveMemberRequestEvent {
            sender: UserDataMember::from(a.user()),
            target: UserDataMember::from(b.user()),
            share_transfers: vec![],
        };
        let mut log = VaultLogObject::create(member.member.clone());
        log.0
            .value
            .requests
            .insert(VaultActionRequestEvent::RemoveMember(request.clone()));
        log.0
            .value
            .liveboard_removal_pending
            .push(PendingRemovalBinding {
                request_id: REQUEST_ID.into(),
                request: request.clone(),
            });
        p_obj.repo.save(log).await?;
        p_obj.repo.order.lock().unwrap().clear();
        Ok(Self {
            p_obj,
            actor,
            request,
            member,
            master_key,
            protocol: Arc::new(Protocol::default()),
        })
    }
    pub fn gateway(&self) -> SyncGateway<Repo, Protocol> {
        SyncGateway::new(
            "outbox-test".into(),
            self.p_obj.clone(),
            self.protocol.clone(),
            self.master_key.clone(),
        )
    }
    pub fn reopened(&self) -> SyncGateway<Repo, Protocol> {
        SyncGateway::new(
            "outbox-reopened".into(),
            Arc::new(PersistentObject::new(self.p_obj.repo.clone())),
            self.protocol.clone(),
            self.master_key.clone(),
        )
    }
    pub fn input(&self) -> CanonicalDecisionInput {
        CanonicalDecisionInput {
            request_id: REQUEST_ID.into(),
            accepted: false,
        }
    }
    pub fn removal(&self) -> RemoveAction<Repo> {
        RemoveAction {
            p_obj: self.p_obj.clone(),
            member: self.member.clone(),
        }
    }
    pub async fn event(&self) -> Result<DeviceLogObject> {
        let update = self.removal().build_update(
            self.request.clone(),
            RemoveActionUpdate::Decline,
            vec![],
        )?;
        let desc = DeviceLogDescriptor::from(self.actor.user_id());
        let mut key = KvKey::from(desc.clone());
        key.obj_id = self.p_obj.find_free_id_by_obj_desc(desc).await?;
        Ok(DeviceLogObject(KvLogEvent {
            key,
            value: VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
        }))
    }
    pub async fn registry(
        &self,
        event: &DeviceLogObject,
        mode: RemovalEnvelopeMode,
    ) -> Result<LocalRemovalEnvelopeObject> {
        let record = LocalRemovalEnvelopeObject(KvLogEvent {
            key: KvKey::from(LocalRemovalEnvelopeDescriptor(self.actor.user_id())),
            value: LocalRemovalEnvelopeRegistry {
                producer_context: None,
                owner: self.actor.user_id(),
                first_managed_removal_nonce: event.obj_id().id.curr as u64,
                entries: vec![LocalRemovalEnvelopeEntry {
                    event_id: event.obj_id(),
                    mode,
                    request_id: if mode == RemovalEnvelopeMode::Guarded {
                        Some(REQUEST_ID.into())
                    } else {
                        None
                    },
                }],
            },
        });
        self.p_obj.repo.save(record.clone()).await?;
        Ok(record)
    }
    pub async fn install(&self, mode: RemovalEnvelopeMode) -> Result<DeviceLogObject> {
        let event = self.event().await?;
        self.registry(&event, mode).await?;
        self.p_obj.repo.save(event.clone()).await?;
        Ok(event)
    }
    pub async fn checked_decline(&self) -> Result<DeviceLogObject> {
        self.protocol.enable_current_guarded();
        let receipt = self.gateway().decide_remove_member(self.input()).await?;
        let events = self.events().await?;
        ensure!(events.len() == 1 && events[0].obj_id() == receipt.event_id);
        ensure!(receipt.request_id == REQUEST_ID);
        let index: LocalGuardedPreparationIndexObject = self
            .p_obj
            .repo
            .find_one(ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
                owner: self.actor.user_id(),
            }))
            .await?
            .context("checked Decline private index")?
            .try_into()?;
        let witness: LocalGuardedPreparationObject = self
            .p_obj
            .repo
            .find_one(ArtifactId::from(LocalGuardedPreparationDescriptor {
                owner: self.actor.user_id(),
                request_id: REQUEST_ID.into(),
            }))
            .await?
            .context("checked Decline private FULL")?
            .try_into()?;
        ensure!(
            index.0.value.revision > 0 && index.0.value.request_ids == vec![REQUEST_ID.to_string()]
        );
        let value = &witness.0.value;
        let payload = value
            .payload
            .as_ref()
            .context("checked Decline complete payload")?;
        ensure!(value.owner == self.actor.user_id() && value.request_id == REQUEST_ID);
        ensure!(
            payload.passes.is_empty() && payload.update.decision == RemoveMemberDecision::Declined
        );
        let origin = value
            .origin
            .as_ref()
            .context("checked Decline actual origin")?;
        ensure!(
            origin.event_id == receipt.event_id
                && origin.request_id == REQUEST_ID
                && !origin.accepted
        );
        ensure!(origin.body_commitment == meta_secret_core::node::common::model::liveboard_producer::producer_source_commitment(&events[0])?);
        let registry = meta_secret_core::node::db::objects::local_removal_envelope::PersistentRemovalEnvelope { p_obj: self.p_obj.clone() }
            .load(&self.actor.user_id()).await?.context("checked Decline Guarded registry")?;
        ensure!(
            registry
                .0
                .value
                .entries
                .iter()
                .any(|entry| entry.event_id == receipt.event_id
                    && entry.mode == RemovalEnvelopeMode::Guarded
                    && entry.request_id.as_deref() == Some(REQUEST_ID))
        );
        ensure!(
            self.writes().is_empty(),
            "checked setup must not dispatch a command"
        );
        Ok(events[0].clone())
    }
    pub async fn events(&self) -> Result<Vec<DeviceLogObject>> {
        self.p_obj
            .get_object_events_from_beginning(DeviceLogDescriptor::from(self.actor.user_id()))
            .await
    }
    pub fn writes(&self) -> Vec<SignedAction> {
        self.protocol.writes.lock().unwrap().clone()
    }
}

pub fn assert_guarded(
    action: &SignedAction,
    event: &DeviceLogObject,
    actor: &UserCreds,
) -> Result<()> {
    action.verify(&actor.device().keys.dsa_pk)?;
    let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
        bail!("RED: managed removal was downgraded to the legacy Event envelope");
    };
    anyhow::ensure!(
        guard.request_id == REQUEST_ID && guard.version == 1 && guard.event == *event,
        "guarded envelope must bind the exact stored event and occurrence"
    );
    anyhow::ensure!(
        action.nonce == event.obj_id().id.curr as u64,
        "guarded signing changed reserved nonce"
    );
    Ok(())
}
pub async fn legacy_append(h: &Harness) -> Result<()> {
    PersistentDeviceLog::from(h.p_obj.clone())
        .save_remove_member_update(h.removal().build_update(
            h.request.clone(),
            RemoveActionUpdate::Decline,
            vec![],
        )?)
        .await
}
