use super::{support::*, *};
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        common::model::{secret::SsDistributionId, vault::vault::VaultMember},
        db::{
            actions::sign_up::remove::{RemoveAction, RemoveActionUpdate},
            descriptors::shared_secret_descriptor::*,
            objects::persistent_shared_secret::PersistentSharedSecret,
        },
        security::{canonical_json, sign_event},
    },
};

// CODEC ONLY: real original K2 ciphertext containers, NOT a valid prepared-K1 lifecycle.
pub async fn codec_record<R: KvLogEventRepo>(h: &H<R>) -> Result<LocalGuardedPreparationObject> {
    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.state.b.vault_name.clone())
        .await?;
    let log = PersistentVault::from(h.p.clone())
        .get_vault_log_artifact(h.state.b.vault_name.clone())
        .await?;
    let update = RemoveAction {
        p_obj: h.p.clone(),
        member: VaultMember {
            member: h.state.b.user().into(),
            vault: vault.0.value.clone(),
        },
    }
    .build_update(
        log.0.value.liveboard_removal_pending[0].request.clone(),
        RemoveActionUpdate::Accept,
        vec![],
    )?;
    let shared = PersistentSharedSecret::from(h.p.clone());
    let claims = shared
        .find_ss_log_tail_event(h.state.b.vault_name.clone())
        .await?
        .context("real initial claim metadata")?;
    let mut passes = vec![];
    for pass in &h.state.passes {
        let mut distributions = vec![];
        for user in [&h.state.a, &h.state.b] {
            distributions.push(
                h.p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: pass.clone(),
                    receiver: user.device_id().clone(),
                }))
                .await?
                .context("actual original encrypted codec input")?,
            );
        }
        let original = distributions[1].clone();
        let data = original.clone().to_distribution_data()?;
        passes.push(GuardedPreparedPass {
            pass_id: pass.clone(),
            input: GuardedInputRef {
                distribution_id: original.obj_id(),
                distribution_commitment: sha256_hex(&String::from_utf8(canonical_json(
                    &original,
                )?)?),
                claim_id: data.claim_id.clone(),
            },
            claim: claims.0.value.claims[&data.claim_id.id].clone(),
            distributions,
        });
    }
    Ok(LocalGuardedPreparationObject(KvLogEvent {
        key: KvKey::from(LocalGuardedPreparationDescriptor {
            owner: h.state.b.user_id(),
            request_id: h.state.uuid.clone(),
        }),
        value: LocalGuardedPreparation {
            version: 1,
            owner: h.state.b.user_id(),
            request_id: h.state.uuid.clone(),
            base_vault: vault.obj_id(),
            requester_id: h.state.a.device_id().clone(),
            target_id: h.state.c.device_id().clone(),
            survivors: vec![h.state.a.user(), h.state.b.user()],
            phase: GuardedPreparationPhase::Staged,
            payload: Some(GuardedPreparationPayload { update, passes }),
            origin: None,
            claims: vec![],
            packets: vec![],
            transport_noop: None,
            previous_attempts: vec![],
        },
    }))
}
#[tokio::test]
async fn s09_private_payload_debug_receipt_registry_and_actual_network_exclusion() -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let w = codec_record(&h).await?;
    let payload = w.0.value.payload.as_ref().unwrap();
    let encoded = serde_json::to_string(&w)?;
    ensure!(
        !encoded.contains("C1_PRIVATE_FIXTURE_FIRST")
            && !encoded.contains("C1_PRIVATE_FIXTURE_SECOND")
            && !encoded.contains("share_blocks"),
        "private persistence contains plaintext/share DTO"
    );
    let debug = [
        format!("{w:?}"),
        format!("{payload:?}"),
        format!("{:?}", payload.passes[0]),
    ]
    .join(" ");
    let cipher = serde_json::to_string(&payload.passes[0].distributions[0])?;
    ensure!(
        !debug.contains(&cipher)
            && !debug.contains("secretMessage")
            && !debug.contains("encryptedShare")
            && !debug.contains(&h.state.uuid),
        "Debug exposed payload/identity content"
    );
    let index = LocalGuardedPreparationIndexObject(KvLogEvent {
        key: KvKey::from(LocalGuardedPreparationIndexDescriptor {
            owner: h.state.b.user_id(),
        }),
        value: LocalGuardedPreparationIndex {
            version: 1,
            owner: h.state.b.user_id(),
            request_ids: vec![h.state.uuid.clone()],
            revision: 1,
        },
    });
    let server_repo = h.state.server_repo.clone();
    let before = server_repo.get_db().await;
    for event in [w.to_generic(), index.to_generic()] {
        let signed = sign_event(
            event,
            h.state.b.device_id().clone(),
            &h.state.b.device_creds.key_manager()?.dsa,
        )?;
        let response = h
            .wire
            .server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed,
            ))))
            .await;
        ensure!(response.is_err(), "private Generic event was accepted");
        let error = response.unwrap_err().to_string();
        ensure!(
            !error.contains(&cipher) && !error.contains("encryptedShare"),
            "private response leaked cipher"
        );
        ensure!(
            server_repo.get_db().await == before,
            "private event changed actual server"
        );
    }
    let p = Arc::new(PersistentObject::new(server_repo));
    let mut tail = PersistentVault::from(p)
        .vault_tail(h.state.b.user())
        .await?;
    tail.vault_log = h.state.witness_id();
    let response = h
        .wire
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: h.state.b.user(),
                tail,
            },
        ))))
        .await;
    ensure!(
        response
            .err()
            .is_some_and(|e| e.to_string() == "InvalidPublicReadTail"),
        "private namespace query bypassed public allowlist"
    );
    let receipt_h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let receipt = receipt_h.decide().await?;
    let dto = serde_json::to_value(receipt)?;
    ensure!(
        dto.as_object()
            .unwrap()
            .keys()
            .all(|k| ["requestId", "eventId", "phase", "errorCode"].contains(&k.as_str())),
        "public checked receipt gained private payload fields"
    );
    let registry =
        meta_secret_core::node::db::objects::local_removal_envelope::PersistentRemovalEnvelope {
            p_obj: receipt_h.p.clone(),
        }
        .load(&receipt_h.state.b.user_id())
        .await?
        .context("real actual queued envelope registry")?;
    let registry_json = serde_json::to_string(&registry)?;
    ensure!(
        !registry_json.contains("secretMessage")
            && !registry_json.contains("encryptedShare")
            && !registry_json.contains("businessSignature"),
        "payload/signature entered append-only registry"
    );
    Ok(())
}
#[tokio::test]
async fn s10_selected_private_typed_codecs_and_fixed_unambiguous_descriptor_controls() -> Result<()>
{
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let mut w = codec_record(&h).await?;
    let generic = w.clone().to_generic();
    let wire = serde_json::to_vec(&generic)?;
    ensure!(
        serde_json::from_slice::<GenericKvLogEvent>(&wire)? == generic,
        "private typed codec changed record"
    );
    let index = LocalGuardedPreparationIndexObject(KvLogEvent {
        key: KvKey::from(LocalGuardedPreparationIndexDescriptor {
            owner: h.state.b.user_id(),
        }),
        value: LocalGuardedPreparationIndex {
            version: 1,
            owner: h.state.b.user_id(),
            request_ids: vec![h.state.uuid.clone()],
            revision: 1,
        },
    })
    .to_generic();
    ensure!(
        serde_json::from_slice::<GenericKvLogEvent>(&serde_json::to_vec(&index)?)? == index,
        "private index codec changed record"
    );
    h.p.repo.save(w.clone()).await?;
    w.0.value.payload = None;
    w.0.value.phase = GuardedPreparationPhase::RetiredUnqueued;
    h.p.repo.replace(w.clone()).await?;
    ensure!(
        h.p.repo.inner.find_one(h.state.witness_id()).await? == Some(w.to_generic())
            && h.p
                .repo
                .inner
                .find_one(h.state.witness_id().next())
                .await?
                .is_none(),
        "fixed-key replace appended/retained old payload"
    );
    let mut owner = h.state.b.user_id();
    owner.vault_name =
        meta_secret_core::node::common::model::vault::vault::VaultName("я:界".into());
    // Pure descriptor encoding ONLY; colon request text is NOT accepted canonical UUID.
    let desc = LocalGuardedPreparationDescriptor {
        owner: owner.clone(),
        request_id: "u:v".into(),
    };
    use meta_secret_core::node::db::descriptors::object_descriptor::ObjectName;
    let expected = format!(
        "6:я:界{}:{}3:u:v",
        owner.device_id.to_string().len(),
        owner.device_id
    );
    ensure!(
        desc.object_name() == expected,
        "namespace did not use UTF8 byte lengths"
    );
    let id = ArtifactId::from(desc);
    let sequence = serde_json::to_value(&id)?;
    ensure!(
        sequence["id"]["curr"] == 1 && sequence["id"]["prev"] == 0,
        "private key not initial fixed sequence"
    );
    Ok(())
}
