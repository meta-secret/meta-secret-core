//! Real public sync setup; only the exact original signed Recovery is withheld.
use super::*;
use anyhow::ensure;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::SignedActionPayload,
        app::sync::sync_protocol::SyncProtocol,
        db::events::vault::liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
    },
};
use std::sync::{Mutex, atomic::AtomicUsize};

#[derive(Debug)]
struct OriginalRecoveryWithheld;
impl std::fmt::Display for OriginalRecoveryWithheld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("inflight setup: exact original Recovery withheld before forwarding")
    }
}
impl std::error::Error for OriginalRecoveryWithheld {}

pub(super) struct Wire<R: KvLogEventRepo> {
    server: Arc<ServerApp<R>>,
    source: SsDeviceLogObject,
    original: SignedAction,
    expected: Mutex<Option<SignedAction>>,
    guarded: Mutex<Vec<(SignedAction, bool)>>,
    held: AtomicUsize,
    forwarded_ss: Mutex<Vec<ArtifactId>>,
}
impl<R: KvLogEventRepo> Wire<R> {
    pub fn new(
        server: Arc<ServerApp<R>>,
        source: SsDeviceLogObject,
        original: SignedAction,
    ) -> Self {
        Self {
            server,
            source,
            original,
            expected: Mutex::new(None),
            guarded: Mutex::new(Vec::new()),
            held: AtomicUsize::new(0),
            forwarded_ss: Mutex::new(Vec::new()),
        }
    }
    pub fn expect_guarded(&self, expected: SignedAction) -> Result<()> {
        let mut slot = self.expected.lock().unwrap();
        ensure!(
            slot.is_none(),
            "inflight setup guarded expectation already set"
        );
        *slot = Some(expected);
        Ok(())
    }
    pub fn assert_boundary(&self, result: Result<()>, expected: &SignedAction) -> Result<()> {
        let error = result.expect_err("inflight setup must reach exact original Recovery barrier");
        ensure!(
            error.downcast_ref::<OriginalRecoveryWithheld>().is_some(),
            "inflight setup stopped before the selected Recovery barrier"
        );
        ensure!(
            self.held.load(Ordering::SeqCst) == 1,
            "inflight original R hit count differs"
        );
        let guarded = self.guarded.lock().unwrap();
        ensure!(
            guarded.len() == 1 && guarded[0].0 == *expected && guarded[0].1,
            "inflight exact signed guarded approval did not reach successful real ServerApp"
        );
        let forwarded = self.forwarded_ss.lock().unwrap();
        ensure!(
            !forwarded.contains(&self.source.obj_id()),
            "inflight original R was forwarded"
        );
        ensure!(
            forwarded.is_empty(),
            "inflight later own SS source bypassed original R"
        );
        Ok(())
    }
}
impl<R: KvLogEventRepo> SyncProtocol for Wire<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let mut guarded_index = None;
        if let SyncRequest::Write(write) = &request
            && let WriteSyncRequest::Event(action) = &**write
        {
            match &action.payload {
                SignedActionPayload::Event(event) => {
                    if event.obj_id() == self.source.obj_id() {
                        ensure!(
                            action == &self.original && event == &self.source.clone().to_generic(),
                            "inflight original R full source/action/signature changed"
                        );
                        self.held.fetch_add(1, Ordering::SeqCst);
                        return Err(OriginalRecoveryWithheld.into());
                    }
                    if matches!(event, GenericKvLogEvent::SsDeviceLog(_))
                        && action.signer == self.original.signer
                    {
                        self.forwarded_ss.lock().unwrap().push(event.obj_id());
                    }
                }
                SignedActionPayload::GuardedRemovalDecision(_) => {
                    ensure!(
                        self.expected.lock().unwrap().as_ref() == Some(action),
                        "inflight signed guarded action/UUID/full ID/nonce changed"
                    );
                    let mut sent = self.guarded.lock().unwrap();
                    guarded_index = Some(sent.len());
                    sent.push((action.clone(), false));
                }
                _ => {}
            }
        }
        let response = Box::pin(self.server.handle_client_request(request)).await;
        if let Some(index) = guarded_index {
            self.guarded.lock().unwrap()[index].1 = response
                .as_ref()
                .is_ok_and(|value| value.clone().ensure_success().is_ok());
        }
        response
    }
}

async fn material<R: KvLogEventRepo>(h: &Inflight<R>) -> Result<Vec<GenericKvLogEvent>> {
    let mut records = Vec::new();
    for user in [&h.base.a, &h.base.b, &h.base.c] {
        records.push(
            h.local
                .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: h.pass.clone(),
                    receiver: user.device_id().clone(),
                }))
                .await?
                .context("inflight actual original encrypted distribution")?
                .to_generic(),
        );
    }
    Ok(records)
}
pub(super) async fn original_material<R: KvLogEventRepo>(
    h: &Inflight<R>,
) -> Result<Vec<GenericKvLogEvent>> {
    let original = material(h).await?;
    for user in [&h.base.a, &h.base.b, &h.base.c] {
        let share = local_share(&h.local, &h.pass, user).await?;
        ensure!(
            share.share_blocks[0].config.threshold == 2,
            "inflight initial ordinary input is not K2"
        );
        ensure!(
            meta_secret_core::recover_from_shares(vec![share]).is_err(),
            "inflight initial single K2 unexpectedly recovers"
        );
    }
    Ok(original)
}
pub(super) async fn assert_prepared_unchanged<R: KvLogEventRepo>(
    h: &Inflight<R>,
    original: &[GenericKvLogEvent],
) -> Result<()> {
    ensure!(
        material(h).await? == original,
        "inflight preparation changed ordinary ciphertext"
    );
    h.assert_withheld().await?;
    ensure!(
        h.signed.payload == SignedActionPayload::Event(h.source.clone().to_generic()),
        "inflight original signed source changed"
    );
    h.signed.verify(&h.base.a.device().keys.dsa_pk)?;
    Ok(())
}
pub(super) async fn assert_removed_material_unchanged<R: KvLogEventRepo>(
    h: &Inflight<R>,
    original: &[GenericKvLogEvent],
) -> Result<()> {
    ensure!(
        material(h).await?[2] == original[2],
        "inflight removed C ciphertext changed"
    );
    h.signed.verify(&h.base.a.device().keys.dsa_pk)?;
    Ok(())
}
pub(super) async fn assert_applied<R: KvLogEventRepo>(
    h: &Inflight<R>,
    action: &SignedAction,
) -> Result<()> {
    let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
        anyhow::bail!("inflight expected actual guarded envelope");
    };
    ensure!(
        guard.request_id == h.removal_id && action.signer == *h.base.a.device_id(),
        "inflight actual guarded UUID/owner differs"
    );
    let server = PersistentVault::from(Arc::new(PersistentObject::new(h.repo.clone())));
    let local = PersistentVault::from(h.local.clone());
    let server_vault = server.get_vault(h.base.a.vault_name.clone()).await?;
    let local_vault = local.get_vault(h.base.a.vault_name.clone()).await?;
    ensure!(
        local_vault == server_vault,
        "inflight genuine canonical Vault not imported exactly"
    );
    let members = local_vault.0.value.members();
    ensure!(
        members.len() == 2
            && members.contains(&member(&h.base.a))
            && members.contains(&member(&h.base.b)),
        "inflight imported survivor identities differ"
    );
    ensure!(
        matches!(local_vault.0.value.find_user(h.base.c.device_id()),
        Some(UserMembership::Outsider(user)) if user.user_data == h.base.c.user()
            && user.status == UserDataOutsiderStatus::Declined),
        "inflight removed C status differs"
    );
    let server_log = server
        .get_vault_log_artifact(h.base.a.vault_name.clone())
        .await?;
    let local_log = local
        .get_vault_log_artifact(h.base.a.vault_name.clone())
        .await?;
    let results: Vec<_> = server_log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .filter(|r| r.request_id == h.removal_id)
        .collect();
    let [result] = results.as_slice() else {
        anyhow::bail!("inflight exact Applied result absent/ambiguous");
    };
    let local_results: Vec<_> = local_log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .filter(|r| r.request_id == h.removal_id)
        .collect();
    ensure!(
        local_results == results,
        "inflight exact ownApplied result not imported"
    );
    let hash = sha256_hex(&String::from_utf8(canonical_json(action)?)?);
    ensure!(
        result.requester_id == *h.base.b.device_id()
            && result.target_id == *h.base.c.device_id()
            && result.responder_id == *h.base.a.device_id()
            && result.outcome == DecisionOutcome::Accepted
            && result.application_state == RemovalApplicationState::Applied
            && result
                .source
                .as_ref()
                .is_some_and(|s| s.event_id == guard.event.obj_id()
                    && s.signer == action.signer
                    && s.signed_action_hash == hash),
        "inflight exact Applied roles/source/hash differ"
    );
    let body = guard.event.clone().to_generic();
    ensure!(
        h.repo.find_one(guard.event.obj_id()).await? == Some(body.clone())
            && h.local.repo.find_one(guard.event.obj_id()).await? == Some(body),
        "inflight actual guarded source body differs locally or on server"
    );
    Ok(())
}
