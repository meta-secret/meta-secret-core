use super::*;

async fn send_body<R: KvLogEventRepo>(
    case: &ActualCase<R>,
    peer: Arc<PersistentObject<InMemKvLogEventRepo>>,
    actor: &UserCreds,
) -> Result<DeviceLogObject> {
    let body: DeviceLogObject = peer
        .find_tail_event(DeviceLogDescriptor::from(actor.user_id()))
        .await?
        .context("actual domain-produced peer body")?;
    let signed = sign_event(
        body.clone().to_generic(),
        actor.device_id().clone(),
        &actor.device_creds.key_manager()?.dsa,
    )?;
    case.h
        .state
        .server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            signed,
        ))))
        .await?
        .ensure_success()?;
    ensure!(
        case.h.state.server_repo.find_one(body.obj_id()).await? == Some(body.clone().to_generic()),
        "actual signed peer command did not become durable"
    );
    Ok(body)
}
async fn import_for_a<R: KvLogEventRepo>(
    case: &ActualCase<R>,
    peer: Arc<PersistentObject<InMemKvLogEventRepo>>,
) -> Result<()> {
    let tail = PersistentVault::from(peer.clone())
        .vault_tail(case.h.state.a.user())
        .await?;
    let data = case
        .h
        .state
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: case.h.state.a.user(),
                tail,
            },
        ))))
        .await?
        .to_data()?;
    for event in data.0 {
        if peer.repo.find_one(event.obj_id()).await?.is_some() {
            peer.repo.replace(event).await?;
        } else {
            peer.repo.save(event).await?;
        }
    }
    Ok(())
}

/// Real C request + real A approval; same server/key/history and original Remove UUID.
pub(super) async fn delete_first<R: KvLogEventRepo>(
    case: &ActualCase<R>,
) -> Result<RemoveMemberRequestEvent> {
    ensure!(
        case.h.state.passes.len() == 2,
        "real TWO-Secret fixture required"
    );
    let p0 = case.h.state.passes[0].clone();
    let p1 = case.h.state.passes[1].clone();
    let peer = Arc::new(PersistentObject::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    // Fixture baseline copies ACTUAL server events, including A's original signed
    // RemoveRequest prefix. No nonce/body/result/tombstone is invented.
    for event in case.h.state.server_repo.get_db().await.into_values() {
        peer.repo.save(event).await?;
    }
    let a_prefix = PersistentDeviceLog::from(peer.clone())
        .find_tail_id(&case.h.state.a.user_id())
        .await?
        .context("actual original A request prefix")?;
    let prefix_body = peer
        .repo
        .find_one(a_prefix.clone())
        .await?
        .context("actual original A source")?
        .device_log()?;
    ensure!(
        matches!(&prefix_body.0.value, VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r))
        if r.target.user_data == case.h.state.c.user()),
        "retained actual A prefix is not original RemoveC"
    );
    SecretDeletionAction {
        p_obj: peer.clone(),
        user_creds: case.h.state.c.clone(),
    }
    .request(p0.clone())
    .await?;
    let c_body = send_body(case, peer.clone(), &case.h.state.c).await?;
    let request_id = match &c_body.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request))
            if request.pass_id == p0 && request.sender.user_data == case.h.state.c.user() =>
        {
            request.request_id.clone()
        }
        _ => anyhow::bail!("actual C delete producer differs"),
    };
    import_for_a(case, peer.clone()).await?;
    SecretDeletionAction {
        p_obj: peer.clone(),
        user_creds: case.h.state.a.clone(),
    }
    .decide(&request_id, true)
    .await?;
    let a_body = send_body(case, peer, &case.h.state.a).await?;
    ensure!(
        a_body.obj_id() == a_prefix.next()
            && matches!(&a_body.0.value, VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(d))
            if d.accepted && d.request.request_id == request_id && d.sender.user_data == case.h.state.a.user()),
        "A approval did not use actual retained stream prefix and real delete request"
    );
    let (vault, log) = canonical(case).await?;
    let result = vault
        .0
        .value
        .deletion_results
        .get(&request_id)
        .context("actual accepted Delete result")?;
    ensure!(
        result.accepted
            && result.sender.user_data == case.h.state.a.user()
            && result.request.sender.user_data == case.h.state.c.user()
            && result.request.pass_id == p0
            && vault.0.value.members().len() == 3
            && !vault.0.value.secrets.contains(&p0)
            && vault.0.value.deleted_secrets.contains(&p0)
            && vault.0.value.secrets.contains(&p1)
            && !vault.0.value.deleted_secrets.contains(&p1),
        "real deletion changed incorrect membership/Pass/result"
    );
    let pending: Vec<_> = log
        .0
        .value
        .liveboard_removal_pending
        .iter()
        .filter(|b| b.request_id == case.h.state.uuid)
        .collect();
    let [pending] = pending.as_slice() else {
        anyhow::bail!("original Remove UUID not uniquely Pending")
    };
    ensure!(
        !log.0
            .value
            .liveboard_removal_results
            .iter()
            .any(|r| r.request_id == case.h.state.uuid)
            && pending
                .request
                .share_transfers
                .iter()
                .all(|t| t.pass_id != p0)
            && pending
                .request
                .share_transfers
                .iter()
                .any(|t| t.pass_id == p1),
        "actual tombstone did not scrub same-UUID Pending removal request"
    );
    eprintln!(
        "C3 actual tombstone C_request=true A_approve=true retained_actual_A_prefix=true members3=true p0_deleted=true p1_active=true SAME_Remove_UUID_pending=true no_removal_result=true"
    );
    Ok(pending.request.clone())
}
