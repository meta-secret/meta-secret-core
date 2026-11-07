//! Actual persisted source proof; public DTOs never substitute for this record.
use super::*;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::{ServerTailRequest, ServerTailResponse},
        common::model::{
            liveboard::{LiveboardOperationKind, LiveboardOperationPhase},
            liveboard_producer::*,
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
            secret::SsDistributionId,
        },
        db::descriptors::shared_secret_descriptor::{SsDeviceLogDescriptor, SsWorkflowDescriptor},
    },
    secret::MetaDistributor,
};

pub(super) fn commitment(source: &DeviceLogObject) -> Result<String> {
    Ok(sha256_hex(&format!(
        "MetaSecret.LiveboardProducer.Source.v1\n{}",
        String::from_utf8(canonical_json(source)?)?
    )))
}
pub(super) fn reference(source: &DeviceLogObject, id: &str) -> Result<ProducerReference> {
    Ok(ProducerReference {
        version: 1,
        kind: ProducerKind::DeleteSecret,
        request_id: Some(id.into()),
        request_stage: ProducerSourceRef {
            event_id: source.obj_id(),
            body_commitment: commitment(source)?,
        },
        decision_stage: None,
    })
}
pub(super) async fn observe<R: KvLogEventRepo>(
    server: &ServerApp<R>,
    sender: meta_secret_core::node::common::model::user::common::UserData,
    input: ProducerReference,
) -> Result<ServerTailResponse> {
    server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
            ServerTailRequest {
                sender,
                producer_reference: Some(input),
            },
        ))))
        .await?
        .to_server_tail()
}
pub(super) fn source_ref(
    log: &VaultLogObject,
    source: &DeviceLogObject,
    id: &str,
) -> Result<Value> {
    let wire = serde_json::to_value(&log.0.value)?;
    let refs = wire
        .get("producerSourceRefs")
        .and_then(Value::as_array)
        .context("RED: real canonical admission did not persist verified producer refs")?;
    let source_id = serde_json::to_value(source.obj_id())?;
    let hits: Vec<_> = refs
        .iter()
        .filter(|r| r["sourceEventId"] == source_id)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "one actual source has one immutable provenance binding"
    );
    let actual = (*hits[0]).clone();
    assert_eq!(actual["requestId"], id);
    assert_eq!(actual["bodyCommitment"], commitment(source)?);
    Ok(actual)
}
pub(super) fn assert_delete_ref(
    value: &Value,
    source: &DeviceLogObject,
    signed: &SignedAction,
    id: &str,
) -> Result<()> {
    let VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) = &source.0.value
    else {
        anyhow::bail!("actual Delete source");
    };
    assert_eq!(value["kind"], "deleteRequest");
    assert_eq!(value["requestId"], id);
    assert_eq!(
        value["actorId"],
        serde_json::to_value(&request.sender.user_data.device.device_id)?
    );
    assert_eq!(
        value["vaultName"],
        serde_json::to_value(request.sender.user_data.vault_name())?
    );
    assert_eq!(
        value["subject"],
        json!({"type":"secret","id":request.pass_id.id_str()})
    );
    assert_eq!(
        value["verifiedEnvelopeDigest"],
        sha256_hex(&String::from_utf8(canonical_json(signed)?)?)
    );
    let fields = [
        "kind",
        "sourceEventId",
        "bodyCommitment",
        "actorId",
        "vaultName",
        "requestId",
        "subject",
        "verifiedEnvelopeDigest",
        "completionMode",
    ];
    assert!(
        value
            .as_object()
            .context("typed private ref")?
            .keys()
            .all(|k| fields.contains(&k.as_str()))
    );
    Ok(())
}
pub(super) async fn assert_complete<R: KvLogEventRepo>(repo: Arc<R>, state: &Resume) -> Result<()> {
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let vault = PersistentVault::from(p.clone())
        .get_vault(state.actor.vault_name.clone())
        .await?;
    let result = vault
        .0
        .value
        .deletion_results
        .get(&state.id)
        .context("same-P accepted automatic result")?;
    assert!(result.accepted);
    assert_eq!(result.request.pass_id, state.pass);
    assert_eq!(vault.0.value.deletion_results.len(), 1);
    assert!(
        vault
            .0
            .value
            .deleted_secrets
            .iter()
            .any(|p| p.id == state.pass.id)
    );
    assert!(!vault.0.value.secrets.iter().any(|p| p.id == state.pass.id));
    let log = PersistentVault::from(p)
        .get_vault_log_artifact(state.actor.vault_name.clone())
        .await?;
    assert_eq!(log.0.value.deletion_results.len(), 1);
    assert!(!log.0.value.requests.iter().any(
        |r| matches!(r, VaultActionRequestEvent::DeleteSecret(p) if p.request_id == state.id)
    ));
    let private = source_ref(&log, &state.source, &state.id)?;
    assert_delete_ref(&private, &state.source, &state.signed, &state.id)?;
    assert_eq!(
        private["completionMode"], "automaticComplete",
        "applied is not complete before terminal log/purge/marker"
    );
    assert!(
        serde_json::to_value(log.0.value)?
            .get("automaticDeleteApplication")
            .is_none_or(Value::is_null)
    );
    for id in &state.workflows {
        assert!(
            repo.find_one(id.clone()).await?.is_none(),
            "actual encrypted workflow was not purged"
        );
    }
    Ok(())
}
pub(super) struct Resume {
    pub actor: UserCreds,
    pub master: TransportSk,
    pub pass: MetaPasswordId,
    pub source: DeviceLogObject,
    pub signed: SignedAction,
    pub id: String,
    pub workflows: Vec<ArtifactId>,
}

async fn recreate_after_tombstone<R: KvLogEventRepo>(
    repo: Arc<R>,
    server: &ServerApp<R>,
    actor: &UserCreds,
    old: &MetaPasswordId,
    previous_source: ArtifactId,
) -> Result<MetaPasswordId> {
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let vault = PersistentVault::from(p.clone())
        .get_vault(actor.vault_name.clone())
        .await?;
    assert!(vault.0.value.deleted_secrets.contains(old));
    let vm = vault.clone().to_data().to_vault_member(member(actor))?;
    let descriptor = DeviceLogDescriptor::from(actor.user_id());
    let history = p
        .get_object_events_from_beginning(descriptor.clone())
        .await?;
    assert_eq!(
        history.last().map(ObjIdExtractor::obj_id),
        Some(previous_source.clone())
    );
    // Restore the real client prefix; the producer must not prewrite the server raw tail.
    let local = Arc::new(PersistentObject::in_mem());
    local.repo.save(vault).await?;
    for event in &history {
        local.repo.save(event.clone()).await?;
    }
    let claims = p
        .get_object_events_from_beginning(SsDeviceLogDescriptor::from(actor.device_id().clone()))
        .await?;
    for claim in claims {
        local.repo.save(claim).await?;
    }
    // This is the app's actual tombstone-aware producer, including its real encryption.
    let receipt = MetaDistributor {
        p_obj: local.clone(),
        user_creds: Arc::new(actor.clone()),
        vault_member: vm.clone(),
    }
    .distribute_checked(
        vm.clone(),
        SecurePassInfo::from(PlainPassInfo::new(
            old.name.clone(),
            "SYNTHETIC-RECREATED-SECRET".into(),
        )),
    )
    .await?;
    assert_eq!(receipt.kind, LiveboardOperationKind::Create);
    assert_eq!(receipt.phase, LiveboardOperationPhase::Queued);
    assert!(receipt.error_code.is_none());
    let emitted = local.get_object_events_from_beginning(descriptor).await?;
    assert_eq!(emitted.len(), history.len() + 1);
    assert!(emitted[..history.len()] == history);
    let source = emitted.last().context("actual saved recreation source")?;
    assert_eq!(source.obj_id(), previous_source.next());
    let VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add)) = &source.0.value
    else {
        anyhow::bail!("actual distributor must queue AddMetaPass");
    };
    let recreated = add.meta_pass_id.clone();
    assert_eq!(add.sender, member(actor));
    assert_eq!(recreated.name, old.name);
    assert_ne!(recreated.id, old.id);
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(recreated.id_str().as_str())
    );
    for receiver in vm.vault.members() {
        local
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: recreated.clone(),
                receiver: receiver.user_data.device.device_id.clone(),
            }))
            .await?
            .context("real new encrypted distribution for every current member")?;
    }
    assert!(repo.find_one(source.obj_id()).await?.is_none());
    send(
        server,
        sign_event(
            source.clone().to_generic(),
            actor.device_id().clone(),
            &actor.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    assert!(repo.find_one(source.obj_id()).await? == Some(source.clone().to_generic()));
    Ok(recreated)
}

pub(super) async fn finish<R: KvLogEventRepo>(repo: Arc<R>, state: Resume) -> Result<()> {
    let signature = canonical_json(&state.signed)?;
    let server = ServerApp::new(repo.clone(), state.master.clone())?;
    let before = observe(
        &server,
        state.actor.user(),
        reference(&state.source, &state.id)?,
    )
    .await?;
    // A read may finish only an ALREADY prepared maintenance phase. An unadmitted R remains fresh.
    if repo.find_one(state.source.obj_id()).await?.is_none() {
        send(&server, state.signed.clone()).await?;
    }
    assert_complete(repo.clone(), &state).await?;
    assert!(repo.find_one(state.source.obj_id()).await? == Some(state.source.clone().to_generic()));
    let after = observe(
        &server,
        state.actor.user(),
        reference(&state.source, &state.id)?,
    )
    .await?;
    assert_eq!(after.device_log_tail, Some(state.source.obj_id().next()));
    let observed = after
        .producer_observation
        .context("same real source observation")?;
    assert_eq!(observed.request_id.as_deref(), Some(state.id.as_str()));
    assert_eq!(
        observed.request_stage.observation,
        ProducerSourceObservation::RawAcknowledged
    );
    assert_eq!(observed.canonical_state, ProducerCanonicalState::Applied);
    assert!(canonical_json(&state.signed)? == signature);
    state.signed.verify(&state.actor.device().keys.dsa_pk)?;
    assert!(
        send(&server, state.signed.clone()).await.is_err(),
        "raw stored old Event remains replay-rejected"
    );
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let bodies = p
        .get_object_events_from_beginning(DeviceLogDescriptor::from(state.actor.user_id()))
        .await?;
    assert_eq!(
        bodies.len(),
        state.source.obj_id().id.curr,
        "no extra producer source beyond the original R"
    );
    assert!(bodies.last() == Some(&state.source));
    assert!(
        before.device_log_tail == Some(state.source.obj_id())
            || before.device_log_tail == Some(state.source.obj_id().next())
    );
    // A separate NEW business command after completion must progress, even with the same name.
    let recreated = recreate_after_tombstone(
        repo.clone(),
        &server,
        &state.actor,
        &state.pass,
        state.source.obj_id(),
    )
    .await?;
    let vault = PersistentVault::from(p)
        .get_vault(state.actor.vault_name.clone())
        .await?;
    assert!(vault.0.value.secrets.contains(&recreated));
    assert!(
        vault
            .0
            .value
            .deleted_secrets
            .iter()
            .any(|old| old.id == state.pass.id)
    );
    assert!(
        !vault
            .0
            .value
            .deleted_secrets
            .iter()
            .any(|old| old.id == recreated.id)
    );
    assert!(
        send(&server, state.signed).await.is_err(),
        "old P never consumes the same-name new exact Secret ID"
    );
    Ok(())
}

#[tokio::test]
async fn tombstoned_name_fixture_uses_real_distributor_and_actual_next_source() -> Result<()> {
    let seed =
        super::integration_binding_fixture::Seed::new(Arc::new(InMemKvLogEventRepo::default()), 2)
            .await?;
    let server = seed.server().await?;
    let (source, signed, request_id) = seed.deletion().await?;
    send(&server, signed.clone()).await?;
    let recreated = recreate_after_tombstone(
        seed.repo.clone(),
        &server,
        &seed.actor,
        &seed.pass,
        source.obj_id(),
    )
    .await?;
    assert!(send(&server, signed).await.is_err());
    let vault = PersistentVault::from(seed.p)
        .get_vault(seed.actor.vault_name)
        .await?;
    assert!(vault.0.value.secrets.contains(&recreated));
    assert!(vault.0.value.deleted_secrets.contains(&seed.pass));
    assert!(!vault.0.value.deleted_secrets.contains(&recreated));
    let result = vault
        .0
        .value
        .deletion_results
        .get(&request_id)
        .context("actual original automatic Delete result")?;
    assert!(result.accepted);
    assert_eq!(result.request.pass_id, seed.pass);
    Ok(())
}
