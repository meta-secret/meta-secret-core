//! Unsigned source references do not grant metadata read context to removed actors.
use super::{
    correlation_support::{commitment, observe},
    recovery_inflight_fixture::Wire,
    *,
};
use meta_secret_core::{
    crypto::key_pair::{KeyPair, TransportDsaKeyPair},
    node::{
        api::{CanonicalDecisionInput, ServerTailResponse},
        app::{orchestrator::MetaOrchestrator, sync::sync_gateway::SyncGateway},
        common::model::liveboard_producer::*,
        db::{
            events::vault::vault_status::VaultStatusObject,
            repo::{generic_db::FindOneQuery, persistent_credentials::PersistentCredentials},
        },
    },
};

async fn reference(h: &Harness, actor: &UserCreds) -> Result<ProducerReference> {
    let source: DeviceLogObject = h
        .p_obj
        .find_tail_event(DeviceLogDescriptor::from(actor.user_id()))
        .await?
        .context("actual admitted own source")?;
    let log = PersistentVault::from(h.p_obj.clone())
        .get_vault_log_artifact(actor.vault_name.clone())
        .await?;
    let record = log
        .0
        .value
        .producer_source(&source.obj_id())?
        .context("actual admitted provenance")?;
    Ok(ProducerReference {
        version: 1,
        kind: ProducerKind::RemoveDevice,
        request_id: Some(record.request_id.clone()),
        request_stage: ProducerSourceRef {
            event_id: source.obj_id(),
            body_commitment: commitment(&source)?,
        },
        decision_stage: None,
    })
}

async fn pull_actual(
    h: &Harness,
    local: &Arc<PersistentObject<InMemKvLogEventRepo>>,
) -> Result<()> {
    for event in h.p_obj.repo.get_db().await.into_values() {
        if local.repo.find_one(event.obj_id()).await?.is_some() {
            local.repo.replace(event).await?;
        } else {
            local.repo.save(event).await?;
        }
    }
    Ok(())
}

async fn removed_actor() -> Result<(Harness, ProducerReference, ProducerReference)> {
    let h = Harness::new().await?;
    h.request().await?;
    let old_ref = reference(&h, &h.a).await?;
    h.decide(false).await?;
    // Actual accepted C→remove A, approved by the distinct remaining B.
    let local = Arc::new(PersistentObject::in_mem());
    pull_actual(&h, &local).await?;
    let vault = PersistentVault::from(h.p_obj.clone())
        .get_vault(h.a.vault_name.clone())
        .await?;
    for actor in [&h.a, &h.b, &h.c] {
        local
            .repo
            .save(VaultStatusObject::new(
                vault.0.value.status(actor.user()),
                ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
            ))
            .await?;
    }
    MetaOrchestrator {
        p_obj: local.clone(),
        user_creds: h.c.clone(),
    }
    .request_remove_member(member(&h.a))
    .await?;
    let source: DeviceLogObject = local
        .find_tail_event(DeviceLogDescriptor::from(h.c.user_id()))
        .await?
        .context("actual Core C Remove A")?;
    send(
        &h.server,
        sign_event(
            source.to_generic(),
            h.c.device_id().clone(),
            &h.c.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    let peer_ref = reference(&h, &h.c).await?;
    pull_actual(&h, &local).await?;
    let master = TransportDsaKeyPair::generate().sk();
    PersistentCredentials {
        p_obj: local.clone(),
        master_key: master.clone(),
    }
    .save_user_creds(h.b.clone())
    .await?;
    let server = Arc::new(ServerApp::new(h.p_obj.repo.clone(), h.master_key.clone())?);
    let gateway = SyncGateway::new(
        "real-member-context".into(),
        local.clone(),
        Arc::new(Wire(server.clone())),
        master,
    );
    let receipt = gateway
        .decide_remove_member(CanonicalDecisionInput {
            request_id: peer_ref
                .request_id
                .clone()
                .context("actual C Remove UUID")?,
            accepted: true,
        })
        .await?;
    let body = local
        .repo
        .find_one(receipt.event_id)
        .await?
        .context("actual queued B decision")?
        .device_log()?;
    let signed = gateway.sign_queued_device_event(body, &h.b.user()).await?;
    assert!(matches!(
        signed.payload,
        meta_secret_core::node::api::SignedActionPayload::GuardedRemovalDecision(_)
    ));
    send(&server, signed).await?;
    let vault = PersistentVault::from(h.p_obj.clone())
        .get_vault(h.a.vault_name.clone())
        .await?;
    assert_eq!(vault.0.value.members().len(), 2);
    assert!(!vault.0.value.is_member(h.a.device_id()));
    assert!(vault.0.value.is_member(h.b.device_id()));
    assert!(vault.0.value.is_member(h.c.device_id()));
    // The immutable old source remains retained; absence of proof cannot mask the defect.
    let log = PersistentVault::from(h.p_obj.clone())
        .get_vault_log_artifact(h.a.vault_name.clone())
        .await?;
    assert!(
        log.0
            .value
            .producer_source(&old_ref.request_stage.event_id)?
            .is_some()
    );
    Ok((h, old_ref, peer_ref))
}

fn unavailable(result: Result<ServerTailResponse>) -> Result<()> {
    if let Ok(response) = result
        && let Some(observed) = response.producer_observation
    {
        assert_eq!(
            observed.canonical_state,
            ProducerCanonicalState::Unknown,
            "RED: retained source ref is not removed-actor or mismatched-key read context"
        );
        assert!(observed.request_id.is_none());
        assert!(observed.secret_id.is_none());
        assert!(observed.target_device_id.is_none());
        assert_eq!(
            observed.request_stage.observation,
            ProducerSourceObservation::Unconfirmed
        );
    }
    Ok(())
}

#[tokio::test]
async fn actually_removed_actor_cannot_observe_subject_or_result_using_retained_valid_reference()
-> Result<()> {
    let (h, old, current) = removed_actor().await?;
    let peer = observe(&h.server, h.c.user(), current)
        .await?
        .producer_observation
        .context("actual current peer context remains valid")?;
    assert_eq!(peer.canonical_state, ProducerCanonicalState::Applied);
    assert_eq!(peer.target_device_id, Some(h.a.device_id().to_string()));
    unavailable(observe(&h.server, h.a.user(), old).await)
}

#[tokio::test]
async fn current_peer_observes_own_actual_applied_source_after_other_actor_removed_control()
-> Result<()> {
    let (h, _, current) = removed_actor().await?;
    let observed = observe(&h.server, h.c.user(), current.clone())
        .await?
        .producer_observation
        .context("current canonical peer metadata")?;
    assert_eq!(observed.canonical_state, ProducerCanonicalState::Applied);
    assert_eq!(observed.request_id, current.request_id);
    assert_eq!(observed.target_device_id, Some(h.a.device_id().to_string()));
    Ok(())
}

#[tokio::test]
async fn current_actor_id_with_wrong_canonical_key_binding_cannot_observe_known_source()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let reference = reference(&h, &h.a).await?;
    h.decide(false).await?;
    let mut claimed = h.a.user();
    claimed.device.keys = h.b.device().keys.clone();
    assert_eq!(claimed.device.device_id, *h.a.device_id());
    assert_ne!(claimed, h.a.user());
    unavailable(observe(&h.server, claimed, reference).await)
}
