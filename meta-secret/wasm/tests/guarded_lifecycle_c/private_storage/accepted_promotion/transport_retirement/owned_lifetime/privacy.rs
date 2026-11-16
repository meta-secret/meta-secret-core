use super::*;

#[tokio::test]
async fn owned_lifetime_actual_nested_published_current_previous_noop_codec_and_privacy_delta()
-> Result<()> {
    let mut phase = "actual_packetbearing_previous_N_current_Published_Nplus1";
    let result: Result<()> = async {
        let h = harness::Harness::fresh(true).await?;
        let record = proof::member2_twice(&h).await?;
        let index = h.case.h.index().await?;
        ensure!(
            record.0.value.payload.is_none()
                && record.0.value.transport_noop.is_none()
                && record.0.value.phase == GuardedPreparationPhase::AcceptedPublished
                && record.0.value.previous_attempts.len() == 1
                && !record.0.value.packets.is_empty()
                && !record.0.value.claims.is_empty(),
            "actual nested current Published/previous packet-bearing shape absent"
        );
        let prior = &record.0.value.previous_attempts[0];
        let technical = prior
            .transport_noop
            .as_ref()
            .context("actual previous Noop proof")?;
        technical
            .authorization
            .verify(&h.case.h.state.b.user().device.keys.dsa_pk)?;
        ensure!(
            technical.receipt.is_some()
                && technical.confirmed_next_tail == Some(prior.origin.event_id.clone().next())
                && !prior.packets.is_empty()
                && prior.origin.business_signature.is_none()
                && record
                    .0
                    .value
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.business_signature.is_some()),
            "actual current business signature/previous technical fields differ"
        );
        phase = "nested_Debug_public_receipt_registry_read_and_generic_write_privacy";
        privacy(&h, &record, &index).await?;
        // Retain only produced compact/index as codec inputs; no FULL payload replay fixture.
        let weak = Arc::downgrade(&h.local.inner);
        drop(h);
        ensure!(
            weak.upgrade().is_none(),
            "local lifecycle owners retained before CODEC-only table"
        );
        phase = "typed_serde_SQLite_ReDB_CODEC_only";
        codec::table(&record, &index).await?;
        Ok(())
    }
    .await;
    eprintln!("OWNED_LIFETIME CODEC phase={phase} ok={}", result.is_ok());
    result
}

async fn privacy(
    h: &harness::Harness,
    record: &LocalGuardedPreparationObject,
    index: &LocalGuardedPreparationIndexObject,
) -> Result<()> {
    let debug = format!(
        "{record:?} {:?} {:?} {:?} {:?} {:?}",
        record.0.value,
        record.0.value.previous_attempts[0],
        record.0.value.previous_attempts[0].origin,
        record.0.value.previous_attempts[0].transport_noop,
        index
    );
    let signature = &record
        .0
        .value
        .origin
        .as_ref()
        .unwrap()
        .business_signature
        .as_ref()
        .unwrap()
        .signature;
    let signature_json = serde_json::to_string(signature)?;
    let private = serde_json::to_string(record)?;
    ensure!(
        !debug.contains(&h.case.h.state.uuid)
            && !debug.contains(&signature_json)
            && !debug.contains("previousAttempts")
            && !private.contains("C1_PRIVATE_FIXTURE")
            && !private.contains("secretMessage")
            && !private.contains("share_blocks"),
        "nested Debug/private serde leaked sensitive contents"
    );
    let state = serde_json::to_string(&h.service().get_app_state().await?)?;
    let registry = PersistentRemovalEnvelope { p_obj: h.p.clone() }
        .load(&h.case.h.state.b.user_id())
        .await?
        .context("actual public-mode registry")?;
    let registry = serde_json::to_string(&registry)?;
    for public in [&state, &registry] {
        ensure!(
            !public.contains(&signature_json)
                && !public.contains("businessSignature")
                && !public.contains("previousAttempts")
                && !public.contains("secretMessage")
                && !public.contains("encryptedShare"),
            "nested private history/signature entered public state/registry"
        );
    }
    // Actual checked receipt projection uses the same original emitted source, never the private nested object.
    let receipt = serde_json::to_value(&h.receipt)?;
    ensure!(
        receipt
            .as_object()
            .context("receipt object")?
            .keys()
            .all(|k| ["requestId", "eventId", "phase", "errorCode"].contains(&k.as_str())),
        "public receipt gained nested private fields"
    );
    let before = h.case.h.state.server_repo.get_db().await;
    for event in [record.clone().to_generic(), index.clone().to_generic()] {
        let signed = sign_event(
            event,
            h.case.h.state.b.device_id().clone(),
            &h.case.h.state.b.device_creds.key_manager()?.dsa,
        )?;
        let response = h
            .case
            .h
            .state
            .server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed,
            ))))
            .await;
        ensure!(
            response.is_err() && h.case.h.state.server_repo.get_db().await == before,
            "actual private nested Generic write accepted/changed server"
        );
        let error = response.unwrap_err().to_string();
        ensure!(
            !error.contains(&signature_json)
                && !error.contains(&private)
                && !error.contains("encryptedShare"),
            "actual private-write error leaked nested contents"
        );
    }
    let mut tail = PersistentVault::from(h.p.clone())
        .vault_tail(h.case.h.state.b.user())
        .await?;
    // Actual unit request tails yield nonempty authorized canonical data; no store edit.
    use meta_secret_core::node::db::descriptors::vault_descriptor::{
        VaultDescriptor, VaultLogDescriptor,
    };
    tail.vault = ArtifactId::from(VaultDescriptor::from(h.case.h.state.b.vault_name.clone()));
    tail.vault_log = ArtifactId::from(VaultLogDescriptor::from(
        h.case.h.state.b.vault_name.clone(),
    ));
    let read = h
        .case
        .h
        .state
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: h.case.h.state.b.user(),
                tail,
            },
        ))))
        .await?
        .to_data()?;
    ensure!(
        !read.0.is_empty()
        && read.0.iter().any(|e| matches!(e, GenericKvLogEvent::Vault(v) if v.0.value.vault_name == h.case.h.state.b.vault_name && v.0.value.members().len() == 2))
        && read.0.iter().any(|e| matches!(e, GenericKvLogEvent::VaultLog(_)))
        && read.0.iter().all(|e| !matches!(
            e,
            GenericKvLogEvent::LocalGuardedPreparation(_)
                | GenericKvLogEvent::LocalGuardedPreparationIndex(_)
        )),
        "actual allowed public read exposed private namespace"
    );
    eprintln!(
        "OWNED_LIFETIME PRIVACY actual_nested_Debug_redacted=true public_state_receipt_registry_clean=true real_Generic_write_rejected_server_unchanged=true actual_allowed_read_private0=true"
    );
    Ok(())
}
