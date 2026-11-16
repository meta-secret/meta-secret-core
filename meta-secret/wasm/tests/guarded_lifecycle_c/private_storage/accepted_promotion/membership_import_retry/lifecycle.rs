use super::*;

#[tokio::test]
async fn actual_join_import_error_sqlite_reopen_retry_restores_owned_secret_for_new_member_without_resplitting()
-> Result<()> {
    let mut outcomes = vec![];
    for (token, kind) in [
        FaultKind::DurableSaveLostReply,
        FaultKind::SecondVaultBefore,
    ]
    .into_iter()
    .enumerate()
    {
        outcomes.push(
            run(Identity {
                kind,
                token: token + 1,
            })
            .await,
        );
    }
    for result in outcomes {
        result?;
    }
    Ok(())
}
async fn run(identity: Identity) -> Result<()> {
    eprintln!(
        "JoinImport {:?} START fresh real SQLite variant",
        identity.kind
    );
    let dir = tempfile::tempdir()?;
    let url = format!("file:{}", dir.path().join("join-import.sqlite").display());
    let bootstrap::World {
        a,
        b,
        c,
        remote,
        server_repo,
        pass,
        original,
        original_claim,
    } = bootstrap::setup(url.clone()).await?;
    b.wire.arm(identity, c.creds.device_id().clone());
    if identity.kind == FaultKind::DurableSaveLostReply {
        b.p.repo.arm(identity, c.creds.device_id().clone());
    }
    let first = b.sync().await;
    let original_error = identity.original(&first);
    let hits = b.p.repo.hits.load(Ordering::SeqCst) + b.wire.hits.load(Ordering::SeqCst);
    let durable = b.p.repo.durable_three.load(Ordering::SeqCst)
        || b.wire.imported_three_at_fault.load(Ordering::SeqCst);
    let local = PersistentVault::from(b.p.clone())
        .get_vault(b.creds.vault_name.clone())
        .await?;
    let attempted_material =
        b.p.repo
            .writes
            .lock()
            .unwrap()
            .iter()
            .filter(|e| material(e))
            .count();
    let uploaded_material = b
        .wire
        .uploads
        .lock()
        .unwrap()
        .iter()
        .filter(|e| material(e))
        .count();
    eprintln!(
        "JoinImport {:?} original_error={original_error} actual_fault_hits={hits} actual_durable_import3={durable} posterror_local_members={} material_attempts={attempted_material} material_uploads={uploaded_material}",
        identity.kind,
        local.0.value.members().len()
    );
    ensure!(
        original_error
            && hits == 1
            && durable
            && local.0.value.members().len() == 3
            && local.0.value.is_member(c.creds.device_id()),
        "actual selected fault did not preserve original error after durable import3"
    );
    if identity.kind == FaultKind::DurableSaveLostReply {
        ensure!(
            attempted_material == 0
                && uploaded_material == 0
                && proof::packet(&b, &pass, b.creds.device_id()).await? == Some(original.clone())
                && proof::packet(&b, &pass, c.creds.device_id())
                    .await?
                    .is_none(),
            "durable import error must not publish material as an error workaround"
        );
    }
    let weak = Arc::downgrade(&b.p.repo.inner);
    let creds = b.creds.clone();
    let master = b.master.clone();
    drop(b);
    ensure!(
        weak.upgrade().is_none(),
        "ALL actual local SQLite owners must be released"
    );
    let b = client::Client::open(Arc::new(file_repo::Repo::open(url)), creds, remote, master);
    let reopened_creds = PersistentCredentials {
        p_obj: b.p.clone(),
        master_key: b.master.clone(),
    }
    .get_user_creds()
    .await?;
    ensure!(
        reopened_creds.as_ref() == Some(&b.creds),
        "actual SQLite encrypted credentials did not reopen"
    );
    let reopened = PersistentVault::from(b.p.clone())
        .get_vault(b.creds.vault_name.clone())
        .await?;
    ensure!(
        reopened.0.value.members().len() == 3,
        "actual imported3 must survive physical reopen"
    );
    let retry = b.sync().await;
    c.pull_vault().await?;
    let actual_c_data = c.pull_ss().await?;
    let c_packet = proof::received_packet(&actual_c_data, &pass, c.creds.device_id());
    let b_packet = proof::packet(&b, &pass, b.creds.device_id())
        .await?
        .context("actual B current own packet")?;
    let claim = proof::active_claim(&b, &b_packet).await?;
    let new_claim = claim.id != original_claim.id
        && claim.sender == *b.creds.device_id()
        && claim.receivers.len() == 2
        && claim.receivers.contains(a.creds.device_id())
        && claim.receivers.contains(c.creds.device_id());
    eprintln!(
        "JoinImport {:?} actual_ALL_local_Arc_close=true same_SQLite_reopen=true retry_full_sync_ok={} actual_C_addressed_packet={} actual_new_B_owned_three_member_claim={new_claim}",
        identity.kind,
        retry.is_ok(),
        c_packet.is_some()
    );
    retry?;
    ensure!(
        c_packet.is_some() && new_claim,
        "successful ordinary retry after real import error/reopen did not redistribute B-owned Secret to C"
    );
    let c_packet = c_packet.context("actual C addressed packet")?;
    let b_data = b_packet.clone().to_distribution_data()?;
    let c_data = c_packet.clone().to_distribution_data()?;
    ensure!(
        b_data.claim_id == c_data.claim_id,
        "mixed actual Join generations"
    );
    let b_share = proof::share(&b_packet, &b.creds)?;
    let c_share = proof::share(&c_packet, &c.creds)?;
    proof::config(&b_share, 2, 3)?;
    proof::config(&c_share, 2, 3)?;
    ensure!(
        meta_secret_core::recover_from_shares(vec![b_share.clone(), c_share.clone()])?.text
            == VALUE
            && meta_secret_core::recover_from_shares(vec![c_share]).is_err(),
        "actual current C+B material does not recover correctly or one share suffices"
    );
    a.pull_ss().await?;
    let tail = proof::own_tail(&b).await?;
    ensure!(
        proof::remote_packet(&server_repo, &pass, a.creds.device_id())
            .await?
            .is_none()
            && proof::remote_packet(&server_repo, &pass, c.creds.device_id())
                .await?
                .is_none(),
        "actual peer reads did not complete outbound delivery"
    );
    b.p.repo.writes.lock().unwrap().clear();
    b.wire.uploads.lock().unwrap().clear();
    b.sync().await?;
    let old_peer_ids = [
        proof::packet(&a, &pass, a.creds.device_id())
            .await?
            .context("actual delivered A packet")?
            .obj_id(),
        c_packet.obj_id(),
    ];
    let peer_receivers = [a.creds.device_id().clone(), c.creds.device_id().clone()];
    let matching_peer = |event: &GenericKvLogEvent| {
        old_peer_ids.contains(&event.obj_id())
            || matches!(event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(packet))
                    if packet.value.claim_id.pass_id == pass
                        && peer_receivers.contains(&packet.value.secret_message.cipher_text()
                            .channel.receiver().to_device_id()))
    };
    let peer_write_attempts =
        b.p.repo
            .writes
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matching_peer(event))
            .count();
    let peer_upload_attempts = b
        .wire
        .uploads
        .lock()
        .unwrap()
        .iter()
        .filter(|event| matching_peer(event))
        .count();
    ensure!(
        peer_write_attempts == 0 && peer_upload_attempts == 0,
        "repeat sync transiently recreated/uploaded a delivered peer packet"
    );
    eprintln!(
        "JoinImport {:?} LATE matching_delivered_peer_write_attempts=0 matching_delivered_peer_upload_attempts=0",
        identity.kind
    );
    ensure!(
        proof::own_tail(&b).await? == tail
            && proof::packet(&b, &pass, b.creds.device_id()).await? == Some(b_packet)
            && proof::remote_packet(&server_repo, &pass, a.creds.device_id())
                .await?
                .is_none()
            && proof::remote_packet(&server_repo, &pass, c.creds.device_id())
                .await?
                .is_none()
            && proof::packet(&b, &pass, a.creds.device_id())
                .await?
                .is_none()
            && proof::packet(&b, &pass, c.creds.device_id())
                .await?
                .is_none(),
        "further sync rerandomized generation or resurrected delivered peer outputs"
    );
    eprintln!(
        "JoinImport {:?} LATE actual_C_B_recovery=true threshold2of3=true stable_repeat=true no_delivered_peer_resurrection=true",
        identity.kind
    );
    Ok(())
}
