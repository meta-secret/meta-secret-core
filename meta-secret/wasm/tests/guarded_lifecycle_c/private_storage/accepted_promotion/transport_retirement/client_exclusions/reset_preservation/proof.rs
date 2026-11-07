use super::*;

pub(super) fn local(result: &Result<ApplicationState>) -> bool {
    matches!(result, Ok(ApplicationState::Local(_)))
}
pub(super) async fn unchanged_server(data: &prior::Data, hash: &str) -> Result<()> {
    ensure!(
        data.hash().await? == hash,
        "local Reset/writer changed canonical winner"
    );
    Ok(())
}
pub(super) fn count_writes(e: &fixture::Env) -> usize {
    e.repo
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| matches!(c.op, observer::Op::Write(..)))
        .count()
}
pub(super) fn late_writes(e: &fixture::Env, done: usize) -> usize {
    e.repo
        .writes
        .lock()
        .unwrap()
        .iter()
        .filter(|w| w.forward > done || w.completed > done)
        .count()
}
pub(super) fn writer_order(e: &fixture::Env) -> (usize, usize, bool) {
    let clean = e.repo.cleans.lock().unwrap();
    let done = clean.last().map(|c| c.done).unwrap_or(0);
    let intent = e.repo.saved_intent.lock().unwrap().clone();
    let writes = e.repo.writes.lock().unwrap();
    let actual = writes.iter().find(|w| Some(&w.event) == intent.as_ref());
    let before = actual.is_some_and(|w| {
        w.success && w.exact && done > 0 && w.forward < w.completed && w.completed < clean[0].entry
    });
    let late = writes
        .iter()
        .filter(|w| done > 0 && (w.forward > done || w.completed > done))
        .count();
    (done, late, before)
}
pub(super) fn audit(e: &fixture::Env) -> Result<()> {
    let mut last = 0;
    let (mut finds, mut keys, mut saves, mut replaces, mut deletes, mut recovery) =
        (0, 0, 0, 0, 0, 0);
    for c in e.repo.calls.lock().unwrap().iter() {
        ensure!(c.order > last, "Reset ALLmethods trace order");
        last = c.order;
        match &c.op {
            observer::Op::Find(id) => {
                finds += 1;
                let _same_stream = id.fqdn == ArtifactId::from(UserCredsDescriptor).fqdn;
            }
            observer::Op::Key(id) => {
                keys += 1;
                let _same_stream = id.fqdn == ArtifactId::from(UserCredsDescriptor).fqdn;
            }
            observer::Op::Write(replace, event) => {
                let _actual_attempt_id = event.obj_id();
                if *replace { replaces += 1 } else { saves += 1 }
            }
            observer::Op::Delete(id) | observer::Op::TryDelete(id) => {
                deletes += 1;
                let _physical_id = id.id.curr;
            }
            observer::Op::Recovery => recovery += 1,
            _ => {}
        }
    }
    let mut previous_wire = 0;
    for call in e.wire.calls.lock().unwrap().iter() {
        ensure!(call.order > previous_wire, "ALL wire call order");
        previous_wire = call.order;
        let _actual_request = &call.request;
    }
    for reply in e.wire.replies.lock().unwrap().iter() {
        let _actual_response = &reply.response;
    }
    eprintln!(
        "RESET_PRESERVATION ALL_methods finds={finds} keys={keys} saves={saves} replaces={replaces} deletes={deletes} recovery={recovery}"
    );
    Ok(())
}
pub(super) async fn cleanup(e: &fixture::Env) -> Result<usize> {
    let db = e.peer.physical.get_db().await;
    let cleans = e.repo.cleans.lock().unwrap();
    ensure!(
        cleans.len() == 1,
        "SEMANTIC exactly one actual terminal cleanup required"
    );
    let c = &cleans[0];
    ensure!(
        c.same_creds
            && c.exact_declined
            && c.old_member
            && c.before_sensitive
            && c.after_empty
            && c.client_held
            && c.sync_held,
        "terminal cleanup lacked exact physical proof/owner/real storage effect"
    );
    let boundary = e
        .wire
        .replies
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.order < c.entry)
        .map(|r| r.order)
        .chain(e.wire.unauthorized.lock().unwrap().iter().copied())
        .max()
        .context("actual prior sync boundary")?;
    let reads = e.repo.reads.lock().unwrap();
    let creds=reads.iter().any(|r|r.order>boundary && r.order<c.entry && matches!(&r.event,Some(GenericKvLogEvent::UserCreds(v))
        if v.0.value.vault_name==e.peer.actor.vault_name && v.0.value.device_creds.clone().decrypt(&e.peer.master).is_ok_and(|d|d==e.peer.actor.device_creds)));
    let status=reads.iter().any(|r|r.order>boundary && r.order<c.entry && matches!(&r.event,Some(GenericKvLogEvent::VaultStatus(s))
        if matches!(s.clone().status(),VaultStatus::Outsider(o) if o.user_data==e.peer.actor.user() && o.status==UserDataOutsiderStatus::Declined)));
    let member=reads.iter().any(|r|r.order>boundary && r.order<c.entry && matches!(&r.event,Some(GenericKvLogEvent::Vault(v))
        if v.0.value.vault_name==e.peer.actor.vault_name && v.0.value.is_member(e.peer.actor.device_id())));
    ensure!(
        creds && status && member,
        "terminal cleanup lacked actual fresh same-identity/status/oldMember production rereads"
    );
    ensure!(
        db.values().all(|v| !matches!(
            v,
            GenericKvLogEvent::UserCreds(_)
                | GenericKvLogEvent::DeviceCreds(_)
                | GenericKvLogEvent::SsWorkflow(_)
                | GenericKvLogEvent::LocalGuardedPreparation(_)
                | GenericKvLogEvent::LocalGuardedPreparationIndex(_)
                | GenericKvLogEvent::LocalRemovalEnvelope(_)
        )),
        "actual Reset resurrected credentials/shares/private stage"
    );
    let done = c.done;
    drop(reads);
    drop(cleans);
    audit(e)?;
    Ok(done)
}
pub(super) async fn original_error<T>(
    e: &fixture::Env,
    result: &Result<T>,
    before: &HashMap<ArtifactId, GenericKvLogEvent>,
) -> Result<()> {
    ensure!(
        e.wire.original(result)
            && e.repo.cleans.lock().unwrap().is_empty()
            && e.peer.physical.get_db().await == *before,
        "generic sync error was replaced/swallowed or cleared local state"
    );
    eprintln!(
        "RESET_PRESERVATION ORIGINAL_error_identity_and_chain=true cleanup0=true local_exact=true"
    );
    Ok(())
}
