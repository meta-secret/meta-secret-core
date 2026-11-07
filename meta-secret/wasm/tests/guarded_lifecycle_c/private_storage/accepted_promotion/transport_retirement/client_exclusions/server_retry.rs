use super::*;
use serde_json::json;

async fn unchanged_original(
    p: &fixture::Peer,
    baseline: &Baseline,
    auth: &DeviceTransportNoop,
) -> Result<()> {
    let w = p.witness(&baseline.origin.request_id).await?;
    ensure!(
        w.0.value.payload.is_none()
            && w.0
                .value
                .transport_noop
                .as_ref()
                .is_some_and(|n| n.authorization == *auth
                    && n.receipt.is_none()
                    && n.confirmed_next_tail.is_none()),
        "failed original retry changed ever-durable auth or fabricated settlement"
    );
    ensure!(
        p.wire.actions.lock().unwrap().is_empty()
            && p.wire
                .noops
                .lock()
                .unwrap()
                .iter()
                .all(|n| n.authorization == *auth
                    && n.durable
                    && n.readback
                    && n.private_clean
                    && n.ordinary_clean
                    && n.receipt.is_none()),
        "retry signed/emitted different authorization or Business source"
    );
    baseline.binding(p, true).await
}
async fn competitor(
    data: &fixture::Data,
    p: &fixture::Peer,
    auth: &DeviceTransportNoop,
) -> Result<DeviceTransportNoopReceipt> {
    let next = data
        .tail(&data.b)
        .await?
        .unwrap_or_else(|| ArtifactId::from(DeviceLogDescriptor::from(data.b.user_id())));
    ensure!(
        next == auth.event_id,
        "DEPENDENCY original N not next before competing admission"
    );
    let mut unsigned = json!({"version":1,"purpose":"meta-secret.device-transport-noop.v1",
        "vaultName":p.actor.vault_name,"actorId":p.actor.device_id(),"eventId":auth.event_id,
        "stream":auth.stream,"nonce":auth.nonce,
        "clientEntryCommitment":sha256_hex("different test-only opaque correlation; no Business authority")});
    let text = String::from_utf8(canonical_json(&unsigned)?)?;
    unsigned["signature"] =
        serde_json::to_value(p.actor.device_creds.key_manager()?.dsa.sign(text))?;
    // Same strict public wire grammar and real current B key; no private sign API or seeded marker.
    let request: SyncRequest =
        serde_json::from_value(json!({"write":{"deviceTransportNoop":unsigned}}))?;
    let SyncRequest::Write(w) = &request else {
        anyhow::bail!("strict competitor decode")
    };
    let WriteSyncRequest::DeviceTransportNoop(other) = &**w else {
        anyhow::bail!("strict competitor purpose")
    };
    other.verify(&p.actor.user().device.keys.dsa_pk)?;
    ensure!(
        other.event_id == auth.event_id && other.actor_id == auth.actor_id && other != auth,
        "competitor not a different valid actual B:N authorization"
    );
    let other = other.clone();
    let response = data.server.handle_client_request(request).await?;
    let DataSyncResponse::DeviceTransportNoop(receipt) = response else {
        anyhow::bail!("DEPENDENCY competitor not admitted with actual receipt")
    };
    let raw = data
        .server_repo
        .find_one(auth.event_id.clone())
        .await?
        .context("actual competing marker")?;
    let GenericKvLogEvent::DeviceTransportNoop(marker) = raw else {
        anyhow::bail!("actual competing marker kind")
    };
    marker.validate(&p.actor.user_id(), &p.actor.user().device.keys.dsa_pk)?;
    ensure!(
        marker.0.value.authorization == other
            && marker.receipt() == receipt
            && receipt.authorization_hash == other.authorization_hash()?
            && data.tail(&data.b).await? == Some(auth.event_id.clone().next()),
        "actual competitor marker/receipt/tail proof differs"
    );
    Ok(receipt)
}
#[tokio::test]
async fn client_exclusions_real_slot_conflict_retries_only_original_authorization() -> Result<()> {
    let mut phase = "setup";
    let result: Result<()> = async {
        let (data, p, baseline) = fixture::retirement().await?;
        let auth = fixture::presend(&data, &p, &baseline).await?;
        competitor(&data, &p, &auth).await?;
        let canonical_hash = data.hash().await?;
        let marker = data.server_repo.find_one(auth.event_id.clone()).await?;
        let next = data.tail(&data.b).await?;
        phase = "real_conflict_retry";
        eprintln!("CLIENT_EXCLUSIONS conflict actual_competitor_marker=true actual_receipt=true actual_tail_N_plus1=true");
        let first = p.sync().await; let second = p.sync().await;
        let reached = {
            let actual = p.wire.noops.lock().unwrap();
            let reached = actual.len() == 2 && actual.iter().all(|n| n.forwarded && n.conflict);
            eprintln!("CLIENT_EXCLUSIONS conflict real_client_attempts={} actual_ServerApp_conflict_reached={reached}", actual.len());
            reached
        };
        ensure!(reached, "DEPENDENCY original public retry did not reach both real server conflicts");
        ensure!(first.is_err() && second.is_err() && data.hash().await? == canonical_hash
            && data.server_repo.find_one(auth.event_id.clone()).await? == marker && data.tail(&data.b).await? == next,
            "conflict retry overwrote competitor/canonical/tail or succeeded");
        unchanged_original(&p, &baseline, &auth).await
    }.await;
    result_line("real_slot_conflict", phase, &result);
    result
}

#[tokio::test]
async fn client_exclusions_really_removed_actor_fails_closed_and_observes_actual_get_state()
-> Result<()> {
    let mut phase = "setup";
    let result: Result<()> = async {
        let (data, p, baseline) = fixture::retirement().await?;
        let auth = fixture::presend(&data, &p, &baseline).await?;
        let ap = fixture::Peer::from_server(&data, data.a.clone()).await?;
        let cp = fixture::Peer::from_server(&data, data.c.clone()).await?;
        let (uuid, _) = data.request(&ap, &data.b).await?;
        let c_prefix = PersistentDeviceLog::from(cp.p.clone()).find_tail_id(&cp.actor.user_id()).await?
            .context("actual C retained Delete(p0) prefix")?;
        let c_body = data.approve(&cp, &uuid).await?;
        let (vault, log) = data.canonical().await?;
        ensure!(c_body.obj_id() == c_prefix.next() && !vault.0.value.is_member(data.b.device_id())
            && vault.0.value.is_member(data.a.device_id()) && vault.0.value.is_member(data.c.device_id())
            && matches!(vault.0.value.find_user(data.b.device_id()), Some(UserMembership::Outsider(o))
                if o.status == UserDataOutsiderStatus::Declined)
            && log.0.value.liveboard_removal_results.iter().any(|r| r.request_id == uuid
                && r.application_state == RemovalApplicationState::Applied)
            && data.server_repo.find_one(auth.event_id.clone()).await?.is_none(),
            "DEPENDENCY real A request/C eligible approval/B removed proof missing");
        let canonical_hash = data.hash().await?; let next = data.tail(&data.b).await?;
        phase = "removed_client_retry";
        eprintln!("CLIENT_EXCLUSIONS removed actual_A_request=true actual_C_next_approval=true canonical_B_Outsider_Declined=true original_N_empty=true");
        let first = p.sync().await; let second = p.sync().await;
        let (forwarded, unauthorized) = {
            let noops = p.wire.noops.lock().unwrap();
            let forwarded = noops.iter().filter(|n| n.forwarded).count();
            let unauthorized = noops.iter().filter(|n| n.unauthorized).count();
            ensure!(forwarded == unauthorized, "reached removed-actor Noop did not receive real server Unauthorized");
            (forwarded, unauthorized)
        };
        ensure!(first.is_err() && second.is_err() && data.hash().await? == canonical_hash
            && data.tail(&data.b).await? == next && data.server_repo.find_one(auth.event_id.clone()).await?.is_none(),
            "removed actor succeeded/settled/changed canonical or another server slot");
        unchanged_original(&p, &baseline, &auth).await?;
        phase = "actual_get_state";
        let before_clean = p.repo.calls.lock().unwrap().iter().filter(|c| matches!(c.operation, observer::Operation::Clean)).count();
        let actual = p.service().get_app_state().await;
        let clean_count = p.repo.calls.lock().unwrap().iter().filter(|c| matches!(c.operation, observer::Operation::Clean)).count();
        match actual {
            Ok(ApplicationState::Local(_)) => {
                ensure!(clean_count > before_clean && PersistentCredentials { p_obj: p.p.clone(), master_key: p.master.clone() }
                    .get_user_creds().await?.is_none()
                    && p.physical.get_db().await.values().all(|e| !matches!(e,
                        GenericKvLogEvent::SsWorkflow(_) | GenericKvLogEvent::UserCreds(_))),
                    "actual Reset lacks real cleanup/credential/share absence");
                eprintln!("CLIENT_EXCLUSIONS removed GetState_called=true Reset_REACHED=true actual_cleanup=true");
            }
            Err(_) => {
                ensure!(clean_count == before_clean, "failed GetState unexpectedly claimed cleanup");
                unchanged_original(&p, &baseline, &auth).await?;
                eprintln!("CLIENT_EXCLUSIONS removed GetState_called=true Reset_UNREACHED=true earlier_sync_or_finalizer_error=true forwarded_Noops={forwarded} actual_Unauthorized={unauthorized}");
            }
            Ok(_) => anyhow::bail!("removed actor GetState returned non-Local success without Reset"),
        }
        ensure!(data.hash().await? == canonical_hash && data.server_repo.find_one(auth.event_id).await?.is_none(),
            "GetState changed server removal/settled old slot");
        Ok(())
    }.await;
    result_line("really_removed_actor", phase, &result);
    result
}
