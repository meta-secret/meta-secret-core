use super::{
    fault_repo::{Fault, Repo},
    fixtures::*,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::{CanonicalDecisionInput, SignedActionPayload},
        app::sync::sync_gateway::SyncGateway,
        common::model::{
            liveboard_producer::producer_source_commitment, meta_pass::MetaPasswordId,
            user::common::UserId,
        },
        db::{
            descriptors::{
                local_guarded_preparation::{
                    LocalGuardedPreparationDescriptor, LocalGuardedPreparationIndexDescriptor,
                },
                vault_descriptor::DeviceLogDescriptor,
            },
            events::{
                generic_log_event::*,
                local_guarded_preparation::*,
                object_id::ArtifactId,
                vault::{device_log_event::DeviceLogObject, vault_log_event::*},
            },
            objects::persistent_object::PersistentObject,
            repo::generic_db::*,
        },
    },
    recover_from_shares,
    secret::shared_secret::UserShareDto,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::Ordering},
};

type Snapshot = HashMap<ArtifactId, GenericKvLogEvent>;
#[derive(Clone)]
enum Trace {
    Read(ArtifactId, Box<Option<GenericKvLogEvent>>),
    Write(Box<GenericKvLogEvent>),
}
#[derive(Clone)]
struct BeforeBody {
    body: DeviceLogObject,
    snapshot: Snapshot,
}
struct ObservedRepo {
    inner: Arc<Repo>,
    owner: UserId,
    trace: Mutex<Vec<Trace>>,
    before_body: Mutex<Vec<BeforeBody>>,
}
impl ObservedRepo {
    async fn write(&self, event: GenericKvLogEvent, replace: bool) -> Result<ArtifactId> {
        self.trace
            .lock()
            .unwrap()
            .push(Trace::Write(Box::new(event.clone())));
        if let GenericKvLogEvent::DeviceLog(body) = &event
            && body.obj_id().fqdn
                == ArtifactId::from(DeviceLogDescriptor::from(self.owner.clone())).fqdn
            && matches!(
                body.0.value,
                VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_))
            )
        {
            let snapshot = self.inner.get_db().await;
            self.before_body.lock().unwrap().push(BeforeBody {
                body: body.clone(),
                snapshot,
            });
        }
        // In particular, preserve the inner actual-save-then-LostReply error.
        if replace {
            self.inner.replace(event).await
        } else {
            self.inner.save(event).await
        }
    }
}
#[async_trait(?Send)]
impl FindOneQuery for ObservedRepo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let found = self.inner.find_one(id.clone()).await?;
        self.trace
            .lock()
            .unwrap()
            .push(Trace::Read(id, Box::new(found.clone())));
        Ok(found)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for ObservedRepo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), false).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.write(value.to_generic(), true).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for ObservedRepo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for ObservedRepo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for ObservedRepo {}
fn observed(h: &Harness) -> Arc<ObservedRepo> {
    Arc::new(ObservedRepo {
        inner: h.p.repo.clone(),
        owner: h.approver.user_id(),
        trace: Mutex::new(vec![]),
        before_body: Mutex::new(vec![]),
    })
}
fn gateway(
    h: &Harness,
    repo: Arc<ObservedRepo>,
) -> SyncGateway<ObservedRepo, super::transport::Protocol> {
    SyncGateway::new(
        "exact-accept-observed".into(),
        Arc::new(PersistentObject::new(repo)),
        h.protocol.clone(),
        h.master.clone(),
    )
}
fn ordinary(snapshot: &Snapshot) -> Snapshot {
    snapshot
        .iter()
        .filter(|(_, event)| {
            matches!(
                event,
                GenericKvLogEvent::SsWorkflow(_)
                    | GenericKvLogEvent::SsDeviceLog(_)
                    | GenericKvLogEvent::SsLog(_)
            )
        })
        .map(|(id, event)| (id.clone(), event.clone()))
        .collect()
}
fn assert_prebody(
    h: &Harness,
    repo: &ObservedRepo,
    before: &Snapshot,
    pass: &MetaPasswordId,
    event_id: &ArtifactId,
    expected_attempts: usize,
) -> Result<GuardedPreparationPayload> {
    let observations = repo.before_body.lock().unwrap().clone();
    assert_eq!(
        observations.len(),
        expected_attempts,
        "count ALL actual body saves, including same-ID idempotent retry"
    );
    let observed = &observations[0];
    assert!(observed.body.obj_id() == *event_id);
    assert!(
        !observed.snapshot.contains_key(event_id),
        "observation must precede actual body persistence"
    );
    assert!(
        ordinary(&observed.snapshot) == ordinary(before),
        "private preparation must preserve all ordinary K2/claims/target bytes"
    );
    let index_id = ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
        owner: h.approver.user_id(),
    });
    let witness_id = ArtifactId::from(LocalGuardedPreparationDescriptor {
        owner: h.approver.user_id(),
        request_id: REQUEST.into(),
    });
    let index_raw = observed
        .snapshot
        .get(&index_id)
        .context("actual prebody index")?;
    let index: LocalGuardedPreparationIndexObject = index_raw.clone().try_into()?;
    let raw = observed
        .snapshot
        .get(&witness_id)
        .context("actual prebody complete FULL")?;
    let witness: LocalGuardedPreparationObject = raw.clone().try_into()?;
    assert!(
        index.0.value.owner == h.approver.user_id()
            && index.0.value.revision > 0
            && index.0.value.request_ids == vec![REQUEST.to_string()]
    );
    assert!(
        witness.0.value.owner == h.approver.user_id()
            && witness.0.value.request_id == REQUEST
            && witness.0.value.phase == GuardedPreparationPhase::Staged
    );
    let origin = witness
        .0
        .value
        .origin
        .as_ref()
        .context("actual prebody origin")?;
    assert!(
        origin.event_id == *event_id
            && origin.request_id == REQUEST
            && origin.actor_id == *h.approver.device_id()
            && origin.accepted
    );
    assert!(origin.body_commitment == producer_source_commitment(&observed.body)?);
    let payload = witness
        .0
        .value
        .payload
        .as_ref()
        .context("complete private payload")?;
    assert!(
        observed.body.0.value
            == VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(
                payload.update.clone()
            ))
    );
    assert_eq!(payload.passes.len(), 1);
    let prepared = &payload.passes[0];
    assert!(
        prepared.pass_id == *pass
            && prepared.claim.dist_claim_id.pass_id == *pass
            && prepared.claim.sender == *h.approver.device_id()
            && prepared.claim.receivers == vec![h.initiator.device_id().clone()]
    );
    let input = before
        .get(&prepared.input.distribution_id)
        .context("private input must be actual original packet")?
        .clone()
        .shared_secret()?;
    assert!(prepared.input.claim_id == input.clone().to_distribution_data()?.claim_id);
    assert!(
        prepared.input.distribution_commitment
            == sha256_hex(&String::from_utf8(
                meta_secret_core::node::security::canonical_json(&input)?
            )?)
    );
    assert_eq!(prepared.distributions.len(), 2);
    let mut receivers = std::collections::HashSet::new();
    for packet in &prepared.distributions {
        let data = packet.clone().to_distribution_data()?;
        assert!(data.claim_id == prepared.claim.dist_claim_id);
        let receiver = data
            .secret_message
            .cipher_text()
            .channel
            .receiver()
            .to_device_id();
        let user = if receiver == *h.approver.device_id() {
            &h.approver
        } else {
            assert!(
                receiver == *h.initiator.device_id(),
                "target/foreign recipient must get no private output"
            );
            &h.initiator
        };
        assert!(receivers.insert(receiver));
        let plain = data
            .secret_message
            .cipher_text()
            .decrypt(&user.device_creds.secret_box.transport.sk)?;
        let share = UserShareDto::try_from(&plain.msg)?;
        assert!(
            !share.share_blocks.is_empty()
                && share
                    .share_blocks
                    .iter()
                    .all(|b| b.config.threshold == 1 && b.config.number_of_shares == 2)
        );
        assert!(recover_from_shares(vec![share])?.text == VALUE);
    }
    let trace = repo.trace.lock().unwrap().clone();
    let body_at = trace
        .iter()
        .position(|x| matches!(x, Trace::Write(e) if e.obj_id() == *event_id))
        .context("actual body write trace")?;
    let witness_at = trace[..body_at]
        .iter()
        .rposition(|x| matches!(x, Trace::Write(e) if e.as_ref() == raw))
        .context("actual attached FULL write")?;
    assert!(trace[witness_at+1..body_at].iter().any(|x| matches!(x, Trace::Read(id,e) if *id == witness_id && e.as_ref().as_ref() == Some(raw))), "actual production whole FULL readback must precede body; observer snapshot is not that readback");
    assert!(trace[..witness_at].iter().any(|x| matches!(x, Trace::Read(id,e) if *id == index_id && e.as_ref().as_ref() == Some(index_raw))));
    for replay in observations.iter().skip(1) {
        assert!(replay.body == observed.body);
        assert!(replay.snapshot.get(event_id) == Some(&observed.body.clone().to_generic()));
        assert!(ordinary(&replay.snapshot) == ordinary(before));
        assert!(replay.snapshot.get(&index_id) == Some(index_raw));
        let retained: LocalGuardedPreparationObject = replay
            .snapshot
            .get(&witness_id)
            .context("actual retry FULL before same-ID body save")?
            .clone()
            .try_into()?;
        assert!(retained.0.value.payload == Some(payload.clone()));
        assert!(retained.0.value.origin == witness.0.value.origin);
        assert!(retained.0.value.phase == GuardedPreparationPhase::Queued);
    }
    Ok(payload.clone())
}

#[tokio::test]
async fn exact_accept_reuses_real_two_of_three_material_and_only_remaining_devices_receive_copies()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    h.protocol.enable_current_guarded();
    let pass = h.seed_secret("exact-accept").await?;
    let before_target = h.distribution(&pass, &h.target).await?;
    let initial = h.share(&pass, &h.approver).await?;
    assert_eq!(initial.share_blocks[0].config.threshold, 2);
    assert!(recover_from_shares(vec![initial]).is_err());
    let request = h.seed_removal().await?;
    assert_eq!(request.share_transfers.len(), 1);
    assert_eq!(request.share_transfers[0].receiver, *h.approver.device_id());
    let before = h.p.repo.get_db().await;
    let repo = observed(&h);
    let gateway = gateway(&h, repo.clone());
    let receipt = gateway
        .decide_remove_member(CanonicalDecisionInput {
            request_id: REQUEST.into(),
            accepted: true,
        })
        .await
        .context("supported exact Accept must freeze actual private material")?;
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    assert_eq!(events.len(), 1, "one actual guarded decision body");
    assert!(events[0].obj_id() == receipt.event_id && receipt.request_id == REQUEST);
    let payload = assert_prebody(&h, &repo, &before, &pass, &receipt.event_id, 1)?;
    let action = gateway
        .sign_queued_device_event(events[0].clone(), &h.approver.user())
        .await?;
    action.verify(&h.approver.device().keys.dsa_pk)?;
    let SignedActionPayload::GuardedRemovalDecision(guard) = action.payload else {
        anyhow::bail!("exact Accept downgraded to legacy Event")
    };
    assert_eq!(guard.request_id, REQUEST);
    let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
        guard.event.0.value
    else {
        anyhow::bail!("wrong command")
    };
    assert!(update.request == request && update == payload.update);
    assert_eq!(update.decision, RemoveMemberDecision::Accepted);
    assert!(h.distribution(&pass, &h.target).await? == before_target);
    assert!(ordinary(&h.p.repo.get_db().await) == ordinary(&before));
    assert!(
        h.protocol.writes.lock().unwrap().is_empty(),
        "private readiness is not a fabricated server Applied"
    );
    Ok(())
}

#[tokio::test]
async fn exact_accept_unsupported_endpoint_does_not_mutate_real_material_or_queue() -> Result<()> {
    let mut h = Harness::new(3).await?;
    h.seed_secret("unsupported-exact-accept").await?;
    h.seed_removal().await?;
    h.protocol.capability.store(0, Ordering::SeqCst);
    let before = h.p.repo.get_db().await;
    assert!(
        h.gateway()
            .decide_remove_member(CanonicalDecisionInput {
                request_id: REQUEST.into(),
                accepted: true
            })
            .await
            .is_err()
    );
    assert!(h.p.repo.get_db().await == before);
    assert!(h.protocol.writes.lock().unwrap().is_empty());
    assert_eq!(h.protocol.probes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn exact_accept_lost_body_reply_and_same_pending_uuid_retry_keep_one_command_and_one_generation()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    h.protocol.enable_current_guarded();
    let pass = h.seed_secret("accept-response-loss").await?;
    h.seed_removal().await?;
    let before = h.p.repo.get_db().await;
    let repo = observed(&h);
    h.p.repo.arm(Fault::LostDeviceLogReply);
    let input = CanonicalDecisionInput {
        request_id: REQUEST.into(),
        accepted: true,
    };
    let first = gateway(&h, repo.clone())
        .decide_remove_member(input.clone())
        .await
        .context("exact Accept must resolve actual lost body reply")?;
    assert_eq!(h.p.repo.hits.load(Ordering::SeqCst), 1);
    let frozen = assert_prebody(&h, &repo, &before, &pass, &first.event_id, 1)?;
    let second = gateway(&h, repo.clone())
        .decide_remove_member(input)
        .await
        .context("same pending reference retry must resolve existing guarded command")?;
    assert_eq!(
        first, second,
        "same exact pending decision cannot allocate a fresh command ID"
    );
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    assert_eq!(events.len(), 1);
    assert!(events[0].obj_id() == first.event_id);
    assert!(assert_prebody(&h, &repo, &before, &pass, &first.event_id, 2)? == frozen);
    let id = ArtifactId::from(LocalGuardedPreparationDescriptor {
        owner: h.approver.user_id(),
        request_id: REQUEST.into(),
    });
    let retained: LocalGuardedPreparationObject =
        h.p.repo
            .find_one(id)
            .await?
            .context("retained private payload")?
            .try_into()?;
    assert!(
        retained.0.value.payload == Some(frozen),
        "retry must preserve exact frozen cipher/claim/generation"
    );
    assert!(ordinary(&h.p.repo.get_db().await) == ordinary(&before));
    let action = h
        .reopened_gateway()
        .sign_queued_device_event(events[0].clone(), &h.approver.user())
        .await?;
    action.verify(&h.approver.device().keys.dsa_pk)?;
    assert!(matches!(
        action.payload,
        SignedActionPayload::GuardedRemovalDecision(_)
    ));
    Ok(())
}
