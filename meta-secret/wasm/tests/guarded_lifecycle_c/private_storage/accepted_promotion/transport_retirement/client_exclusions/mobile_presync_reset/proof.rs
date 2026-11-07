use super::*;
pub(super) fn local(r: &Result<ApplicationState>) -> bool {
    matches!(r, Ok(ApplicationState::Local(_)))
}
pub(super) fn no_material(e: &fixture::Env, after: usize) -> Result<()> {
    for c in e
        .repo
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.order > after)
    {
        if let observer::Op::Write(_, event) = &c.op {
            ensure!(
                !matches!(
                    event.as_ref(),
                    GenericKvLogEvent::SsWorkflow(_)
                        | GenericKvLogEvent::SsLog(_)
                        | GenericKvLogEvent::SsDeviceLog(_)
                        | GenericKvLogEvent::DeviceLog(_)
                        | GenericKvLogEvent::LocalGuardedPreparation(_)
                        | GenericKvLogEvent::LocalGuardedPreparationIndex(_)
                        | GenericKvLogEvent::LocalRemovalEnvelope(_)
                ),
                "posterror handoff published material/source/private metadata"
            );
        }
    }
    for c in e
        .wire
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.order > after)
    {
        ensure!(
            !matches!(&c.request, SyncRequest::Write(_)),
            "posterror domain send"
        );
    }
    Ok(())
}
pub(super) async fn cleanup(e: &fixture::Env, boundary: usize) -> Result<()> {
    {
        let clean = e.repo.cleans.lock().unwrap();
        let [c] = clean.as_slice() else {
            anyhow::bail!("actual manager terminal cleanup not exactly once")
        };
        ensure!(
            c.entry > boundary
                && c.done > c.entry
                && c.client
                && c.sync
                && c.same
                && c.declined
                && c.old_member
                && c.emptied,
            "cleanup lacks actual SAME identity/status/member/owner/effect proof"
        );
        let reads = e.repo.reads.lock().unwrap();
        let creds=reads.iter().any(|r|r.order>boundary&&r.order<c.entry&&matches!(&r.event,Some(GenericKvLogEvent::UserCreds(u))if u.0.value.vault_name==e.peer.actor.vault_name&&u.0.value.device_creds.clone().decrypt(&e.peer.master).is_ok_and(|d|d==e.peer.actor.device_creds)));
        let status=reads.iter().any(|r|r.order>boundary&&r.order<c.entry&&matches!(&r.event,Some(GenericKvLogEvent::VaultStatus(s))if matches!(s.clone().status(),VaultStatus::Outsider(o)if o.user_data==e.peer.actor.user()&&o.status==UserDataOutsiderStatus::Declined)));
        ensure!(creds && status, "fresh production terminal rereads missing");
        ensure!(
            e.repo
                .writes
                .lock()
                .unwrap()
                .iter()
                .all(|w| w.forward < c.done && w.done < c.done),
            "local write after actual cleanup"
        );
    }
    ensure!(
        e.peer.physical.get_db().await.is_empty(),
        "sensitive local DB not physically cleared"
    );
    no_material(e, boundary)
}
pub(super) fn audit(e: &fixture::Env) -> Result<()> {
    let mut previous = 0;
    for c in e.repo.calls.lock().unwrap().iter() {
        ensure!(c.order > previous, "ALL repo order");
        previous = c.order;
        match &c.op {
            observer::Op::Find(id)
            | observer::Op::Key(id)
            | observer::Op::Delete(id)
            | observer::Op::TryDelete(id) => {
                let _ = &id.fqdn;
            }
            observer::Op::Write(replace, event) => {
                let _ = (*replace, event.obj_id());
            }
            _ => {}
        }
    }
    for r in e.repo.reads.lock().unwrap().iter() {
        let _ = (&r.id, r.order);
    }
    for w in e.repo.writes.lock().unwrap().iter() {
        let _ = (&w.event, w.success);
    }
    for r in e.wire.replies.lock().unwrap().iter() {
        let _ = &r.response;
    }
    Ok(())
}

pub(super) fn imported_declined(e: &fixture::Env) -> bool {
    let before = e
        .repo
        .cleans
        .lock()
        .unwrap()
        .first()
        .map(|c| c.entry)
        .unwrap_or(usize::MAX);
    let typed = |event: &GenericKvLogEvent| matches!(event,GenericKvLogEvent::VaultStatus(s)if matches!(s.clone().status(),VaultStatus::Outsider(o)if o.user_data==e.peer.actor.user()&&o.status==UserDataOutsiderStatus::Declined));
    e.repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .any(|w| w.success && w.exact && w.done < before && typed(&w.event))
        && e.repo
            .reads
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.order < before && r.event.as_ref().is_some_and(typed))
}
pub(super) fn original_transport(e: &fixture::Env, auth: &DeviceTransportNoop) -> Result<()> {
    let calls = e.wire.calls.lock().unwrap();
    let writes = calls
        .iter()
        .filter_map(|c| match &c.request {
            SyncRequest::Write(w) => Some(&**w),
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        writes.len() == 1
            && matches!(writes[0],WriteSyncRequest::DeviceTransportNoop(actual)if actual==auth),
        "manager signed/sent another authorization or Business source"
    );
    for call in e.repo.calls.lock().unwrap().iter() {
        if let observer::Op::Write(_, event) = &call.op
            && let GenericKvLogEvent::LocalGuardedPreparation(w) = event.as_ref()
        {
            ensure!(
                w.0.value.payload.is_none()
                    && w.0
                        .value
                        .transport_noop
                        .as_ref()
                        .is_some_and(|n| n.authorization == *auth
                            && n.receipt.is_none()
                            && n.confirmed_next_tail.is_none()),
                "manager rewrote auth/resurrected FULL/falsely settled transport"
            );
        }
    }
    Ok(())
}
