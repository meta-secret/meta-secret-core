use super::*;

#[tokio::test]
async fn owned_lifetime_real_metadata_current_pending_upload_and_deleted_history_raw_absence()
-> Result<()> {
    let mut phase = "actual_Accepted_publication";
    let result: Result<()> = async {
        let h = harness::Harness::fresh(false).await?;
        let w = h.case.h.witness().await?;
        let p0 = h.case.h.state.passes[0].clone();
        let ownership = w.0.value.claims.iter().find(|c| c.pass_id == p0).context("real p0 own claim")?;
        let claim = ownership.claim.clone();
        let own_id = ownership.ss_device_event_id.clone().context("actual own sequential claim ID")?;
        current_positive(&h, &claim)?;
        phase = "actual_member2_Delete_and_later_delivery";
        let a = peers::Peer::existing(&h, h.case.h.state.a.clone()).await?;
        peers::delete_at_two(&h, &a).await?;
        a.pull_vault(&h).await?;
        let delivery = a.pull_ss(&h).await?;
        ensure!(delivery.iter().any(|e| e.obj_id() == packet_id(&h.case.h.state.passes[1], &a.creds)),
            "DEPENDENCY actual later p1 delivery event not reached");
        let repo_cursor = h.repo.calls.lock().unwrap().len();
        let wire_cursor = h.wire.calls.lock().unwrap().len();
        let start = h.repo.tick();
        phase = "actual_filtered_aggregate_and_history_lookup";
        let sync = h.sync().await;
        // Evaluate the actual reached selector path even when a later finalizer returns Err.
        let history: Result<()> = async {
        let response = h.wire.replies.lock().unwrap().iter().filter(|r| r.order > start)
            .find_map(|r| match &r.response {
                DataSyncResponse::Data(data) if matches!(&r.request, SyncRequest::Read(q) if matches!(&**q, ReadSyncRequest::SsRequest(_))) =>
                    data.0.iter().filter_map(|e| match e { GenericKvLogEvent::SsLog(l) => Some((r.order, l.clone())), _ => None })
                        .find(|(_, l)| !l.0.value.claims.contains_key(&claim.id)),
                _ => None,
            }).context("DEPENDENCY no actual returned filtered SsLog aggregate; absent fallback not reached")?;
        let (returned, log) = response;
        let imported = h.repo.writes.lock().unwrap().iter().find(|w| w.forward > returned && w.success && w.exact
            && w.event == log.clone().to_generic()).map(|w| w.done).context("actual filtered aggregate successful production import")?;
        let raw_lookups = {
            let reads = h.repo.reads.lock().unwrap();
            let candidate = reads.iter().find(|r| r.order > imported && r.id == own_id
                && matches!(&r.event, Some(GenericKvLogEvent::SsDeviceLog(c)) if c.0.value == claim))
                .context("actual retained owned-history candidate read UNREACHED")?;
            let mut lookups = vec![];
            for who in [&h.case.h.state.a, &h.case.h.state.b] {
                let id = packet_id(&p0, who);
                let absent = reads.iter().any(|r| r.order > candidate.order && r.id == id && r.event.is_none());
                lookups.push((id, absent));
            }
            lookups
        };
        for (id, absent) in raw_lookups {
            ensure!(absent, "actual historical candidate raw lookup absent UNREACHED");
            ensure!(h.local.inner.find_one(id).await?.is_none(), "deleted raw survived real purge");
        }
        no_p0_attempts(&h, repo_cursor, wire_cursor, &p0)?;
        let tail: meta_secret_core::node::db::events::shared_secret_event::SsLogObject = h.p
            .find_tail_event(SsLogDescriptor::from(h.case.h.state.b.vault_name.clone())).await?.context("actual imported tail")?;
        ensure!(!tail.0.value.claims.contains_key(&claim.id)
            && h.local.inner.find_one(own_id.clone()).await?.is_some(), "filtered current absent/retained sequential metadata differs");
        eprintln!("OWNED_LIFETIME SELECTORS actual_returned_imported_filtered_SsLog=true own_history_candidate_raw_lookup_absent=true ALL_deleted_raw_writes_uploads0=true positive_fallback_NOT_claimed=true");
        Ok(())
        }.await;
        observed_history(&h, start, &claim, &own_id);
        eprintln!("OWNED_LIFETIME SELECTORS WHOLE_sync_ok={} ordered_history_proof_ok={}", sync.is_ok(), history.is_ok());
        sync.context("SEMANTIC whole sync after real Delete/delivery")?;
        history?;
        Ok(())
    }.await;
    eprintln!(
        "OWNED_LIFETIME SELECTORS phase={phase} ok={}",
        result.is_ok()
    );
    result
}

fn current_positive(h: &harness::Harness, claim: &SsClaim) -> Result<()> {
    ensure!(
        claim.sender == *h.case.h.state.b.device_id()
            && claim.distribution_type == SecretDistributionType::Split,
        "actual B-own Split required"
    );
    let replies = h.wire.replies.lock().unwrap();
    let current = replies.iter().find(|r| matches!(&r.response, DataSyncResponse::Data(d) if d.0.iter()
        .any(|e| matches!(e, GenericKvLogEvent::SsLog(l) if l.0.value.claims.get(&claim.id).is_some_and(|c|
            c.dist_claim_id == claim.dist_claim_id && c.status.statuses.values().any(|s| *s == SsDistributionStatus::Pending))))))
        .context("actual returned own current Pending Split UNREACHED")?;
    let raw = replies
        .iter()
        .find(|r| {
            r.order > current.order
                && proof::signed_event(&r.request).is_some_and(|e| {
                    matches!(e, GenericKvLogEvent::SsWorkflow(p)
            if p.clone().to_distribution_data().is_ok_and(|d| d.claim_id == claim.dist_claim_id))
                })
                && r.response.clone().ensure_success().is_ok()
        })
        .context("actual current Pending raw upload/ACK")?;
    ensure!(h.repo.reads.lock().unwrap().iter().any(|r| r.order > current.order && r.order < raw.order
        && matches!(&r.event, Some(GenericKvLogEvent::SsWorkflow(p)) if p.clone().to_distribution_data()
            .is_ok_and(|d| d.claim_id == claim.dist_claim_id))), "actual current selector raw read not reached");
    eprintln!(
        "OWNED_LIFETIME SELECTORS actual_current_Pending=true real_raw_lookup_signed_upload_ACK=true"
    );
    Ok(())
}
fn no_p0_attempts(h: &harness::Harness, rc: usize, wc: usize, p0: &MetaPasswordId) -> Result<()> {
    let scoped = |e: &GenericKvLogEvent| {
        matches!(e, GenericKvLogEvent::SsWorkflow(p @ SsWorkflowObject::Distribution(_)) if
            [&h.case.h.state.a, &h.case.h.state.b, &h.case.h.state.c].iter().any(|who| p.obj_id() == packet_id(p0, who))
            || p.clone().to_distribution_data().is_ok_and(|d| d.vault_name == h.case.h.state.b.vault_name && d.claim_id.pass_id == *p0))
    };
    ensure!(
        h.repo.calls.lock().unwrap()[rc..]
            .iter()
            .all(|c| !matches!(&c.op, repo::Op::Write(_, e) if scoped(e)))
            && h.wire.calls.lock().unwrap()[wc..]
                .iter()
                .all(|c| proof::signed_event(&c.request).is_none_or(|e| !scoped(&e))),
        "ALL deleted raw attempts must be zero (physical IDs OR typed Pass/Vault, no generation filter)"
    );
    Ok(())
}

fn observed_history(h: &harness::Harness, start: usize, claim: &SsClaim, own_id: &ArtifactId) {
    let replies = h.wire.replies.lock().unwrap();
    let responses = replies
        .iter()
        .filter(|r| {
            r.order > start
                && matches!(&r.request,
        SyncRequest::Read(q) if matches!(&**q, ReadSyncRequest::SsRequest(_)))
        })
        .count();
    let filtered = replies
        .iter()
        .filter(|r| r.order > start)
        .flat_map(|r| match &r.response {
            DataSyncResponse::Data(d) => d
                .0
                .iter()
                .filter_map(|e| match e {
                    GenericKvLogEvent::SsLog(l) if !l.0.value.claims.contains_key(&claim.id) => {
                        Some((r.order, l.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect::<Vec<_>>();
    let writes = h.repo.writes.lock().unwrap();
    let imported = filtered
        .iter()
        .filter_map(|(reply, l)| {
            writes
                .iter()
                .find(|w| {
                    w.forward > *reply && w.success && w.exact && w.event == l.clone().to_generic()
                })
                .map(|w| w.done)
        })
        .min();
    let reads = h.repo.reads.lock().unwrap();
    let candidate = imported.and_then(|import| reads.iter().find(|r| r.order > import && r.id == *own_id
        && matches!(&r.event, Some(GenericKvLogEvent::SsDeviceLog(c)) if c.0.value == *claim)).map(|r|r.order));
    let absent = [&h.case.h.state.a, &h.case.h.state.b]
        .iter()
        .filter(|who| {
            candidate.is_some_and(|candidate| {
                reads.iter().any(|r| {
                    r.order > candidate
                        && r.id == packet_id(&claim.dist_claim_id.pass_id, who)
                        && r.event.is_none()
                })
            })
        })
        .count();
    let last_p0_missing = reads.last().is_some_and(|r| {
        r.id == packet_id(&claim.dist_claim_id.pass_id, &h.case.h.state.b) && r.event.is_none()
    });
    eprintln!(
        "OWNED_LIFETIME SELECTORS OBSERVED real_SS_replies={responses} returned_filtered_aggregates={} successful_import={} owned_history_candidate_after_import={} later_raw_absence_lookups={absent} last_successful_read_p0_own_absent={last_p0_missing}",
        filtered.len(),
        imported.is_some(),
        candidate.is_some()
    );
}
