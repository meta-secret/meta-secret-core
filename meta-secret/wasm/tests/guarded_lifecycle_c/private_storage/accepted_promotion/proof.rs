use super::*;
use meta_secret_core::{crypto::utils::sha256_hex, node::security::canonical_json};

pub(super) fn hash(action: &SignedAction) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(action)?)?))
}
pub(super) fn intended(e: &Expected, packet: &SsWorkflowObject) -> bool {
    let Ok(data) = packet.clone().to_distribution_data() else {
        return false;
    };
    e.frozen.passes.iter().any(|pass| {
        pass.pass_id == data.claim_id.pass_id
            && pass.claim.dist_claim_id == data.claim_id
            && pass.distributions.iter().any(|frozen| {
                frozen.obj_id() == packet.obj_id()
                    && frozen.clone().to_distribution_data().is_ok_and(|original| {
                        original.secret_message.cipher_text().channel.receiver()
                            == data.secret_message.cipher_text().channel.receiver()
                    })
            })
    })
}
pub(super) async fn signature_before<R: KvLogEventRepo>(
    repo: Arc<R>,
    e: &Expected,
    action: &SignedAction,
) -> Result<bool> {
    let witness: LocalGuardedPreparationObject = repo
        .find_one(e.witness_id.clone())
        .await?
        .context("actual signature witness")?
        .try_into()?;
    let Some(origin) = &witness.0.value.origin else {
        return Ok(false);
    };
    let Some(signature) = &origin.business_signature else {
        return Ok(false);
    };
    let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
        return Ok(false);
    };
    Ok(origin.event_id == e.event_id
        && origin.request_id == e.uuid
        && signature.signer == action.signer
        && signature.stream == action.stream
        && signature.nonce == action.nonce
        && signature.signature == action.signature
        && signature.signed_action_hash == hash(action)?
        && action.signer == e.owner.device_id
        && guard.request_id == e.uuid
        && repo.find_one(e.event_id.clone()).await? == Some(guard.event.clone().to_generic())
        && matches!(
            witness.0.value.phase,
            GuardedPreparationPhase::Signed
                | GuardedPreparationPhase::AcceptedPromoting
                | GuardedPreparationPhase::AcceptedPublished
        ))
}
pub(super) async fn applied<R: KvLogEventRepo>(
    repo: Arc<R>,
    e: &Expected,
    action: &SignedAction,
) -> Result<bool> {
    let p = Arc::new(PersistentObject::new(repo));
    let vault = PersistentVault::from(p);
    let log = vault
        .get_vault_log_artifact(e.owner.vault_name.clone())
        .await?;
    let results: Vec<_> = log
        .0
        .value
        .liveboard_removal_results
        .iter()
        .filter(|r| r.request_id == e.uuid)
        .collect();
    let [r] = results.as_slice() else {
        return Ok(false);
    };
    let members = vault
        .get_vault(e.owner.vault_name.clone())
        .await?
        .0
        .value
        .members()
        .len();
    Ok(r.requester_id == e.requester
        && r.target_id == e.target
        && r.responder_id == e.owner.device_id
        && r.outcome
            == if e.accepted {
                DecisionOutcome::Accepted
            } else {
                DecisionOutcome::Declined
            }
        && r.application_state == RemovalApplicationState::Applied
        && r.source.as_ref().is_some_and(|s| {
            s.event_id == e.event_id
                && s.signer == action.signer
                && hash(action).is_ok_and(|h| s.signed_action_hash == h)
        })
        && members == if e.accepted { 2 } else { 3 })
}
pub(super) struct Measures {
    pub sent: usize,
    pub signature: bool,
    pub server_applied: bool,
    pub local_applied: bool,
    pub raw_ack: bool,
    pub live: usize,
    pub frozen_writes: usize,
    pub intended_writes: usize,
    pub promoting_writes: usize,
    pub faults: usize,
    pub noops: usize,
}
impl Measures {
    pub fn print(&self, label: &str, sync: &Result<()>) {
        eprintln!(
            "C2 {label} sync_ok={} actual_sent={} signature_before={} server_applied={} local_applied={} raw_ack={} all_distribution_writes={} exact_frozen_writes={} intended_generation_writes={} promoting_readback_writes={} fault_hits={} noops={}",
            sync.is_ok(),
            self.sent,
            self.signature,
            self.server_applied,
            self.local_applied,
            self.raw_ack,
            self.live,
            self.frozen_writes,
            self.intended_writes,
            self.promoting_writes,
            self.faults,
            self.noops
        );
    }
}
pub(super) async fn measure<R: KvLogEventRepo>(case: &fixture::Case<R>) -> Result<Measures> {
    let e = case.observations.expected()?;
    let sent = case.observations.sent.lock().unwrap().clone();
    let live = case.observations.live.lock().unwrap().clone();
    let mut m = Measures {
        sent: sent.len(),
        signature: !sent.is_empty() && sent.iter().all(|s| s.durable_signature),
        server_applied: false,
        local_applied: false,
        raw_ack: false,
        live: live.len(),
        frozen_writes: live
            .iter()
            .filter(|w| {
                e.frozen
                    .passes
                    .iter()
                    .any(|p| p.distributions.contains(&w.packet))
            })
            .count(),
        intended_writes: live.iter().filter(|w| intended(&e, &w.packet)).count(),
        promoting_writes: live.iter().filter(|w| w.promoting_readback).count(),
        faults: case.observations.fault_hits.load(Ordering::SeqCst),
        noops: case.observations.noops.load(Ordering::SeqCst),
    };
    if let Some(s) = sent.first() {
        m.server_applied = applied(case.h.state.server_repo.clone(), &e, &s.action).await?;
        m.local_applied = applied(case.h.p.repo.inner.inner.clone(), &e, &s.action).await?;
        if let SignedActionPayload::GuardedRemovalDecision(guard) = &s.action.payload {
            m.raw_ack = s.server_ok
                && case.h.state.server_repo.find_one(e.event_id).await?
                    == Some(guard.event.clone().to_generic());
        }
    }
    Ok(m)
}
pub(super) fn common(m: &Measures) -> Result<()> {
    ensure!(
        m.sent == 1,
        "C2 must send one actual original signed decision"
    );
    ensure!(
        m.signature,
        "C2 original signature was NOT durable before actual send"
    );
    ensure!(
        m.server_applied && m.local_applied,
        "C2 exact actual Applied source not imported locally"
    );
    ensure!(
        m.raw_ack && m.noops == 0,
        "C2 original Business ACK differs or technical Noop substituted"
    );
    Ok(())
}
pub(super) async fn empty_outputs<R: KvLogEventRepo>(case: &fixture::Case<R>) -> Result<bool> {
    let e = case.observations.expected()?;
    ensure!(
        e.passes.is_empty() && e.frozen.passes.is_empty(),
        "empty oracle needs actual empty context"
    );
    if !case.observations.live.lock().unwrap().is_empty() {
        return Ok(false);
    }
    let attempted = case.observations.claim_writes.lock().unwrap().clone();
    let uploaded = case.observations.uploads.lock().unwrap().clone();
    let stored = case.h.p.repo.snapshot().await?;
    Ok(!attempted
        .iter()
        .chain(uploaded.iter())
        .chain(stored.values())
        .any(|event| {
            matches!(event, GenericKvLogEvent::SsWorkflow(packet) if current_packet(&e, packet))
                || (current_claim_record(&e.owner, event) && nonempty_claim_record(event))
        }))
}
async fn actual_log_claim<R: KvLogEventRepo>(
    case: &fixture::Case<R>,
    ownership: &GuardedClaimOwnership,
    pass: &GuardedPreparedPass,
) -> Result<()> {
    use meta_secret_core::node::{
        common::model::secret::SecretDistributionType,
        db::descriptors::shared_secret_descriptor::SsLogDescriptor,
    };
    let id = ownership
        .ss_log_event_id
        .clone()
        .context("C2 actual allocated SsLog key absent")?;
    let log = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(id.clone())
        .await?
        .context("C2 recorded SsLog key has no durable object")?
        .ss_log()?;
    let key = KvKey::from(SsLogDescriptor::from(case.h.state.b.vault_name.clone()));
    ensure!(
        log.obj_id() == id
            && log.obj_id().fqdn == key.obj_id.fqdn
            && log.key().obj_desc == key.obj_desc,
        "C2 recorded SsLog key/type/Vault differs"
    );
    let actual = log
        .0
        .value
        .claims
        .get(&pass.claim.id)
        .context("C2 intended Split claim absent at actual SsLog key")?;
    // Delivery/client status may legitimately progress. Every stable field stays exact.
    let mut stable = actual.clone();
    stable.status = pass.claim.status.clone();
    stable.client_status = pass.claim.client_status.clone();
    ensure!(
        actual.distribution_type == SecretDistributionType::Split
            && stable == pass.claim
            && actual.vault_name == case.h.state.b.vault_name,
        "C2 durable SsLog intended Split identity/generation/content differs"
    );
    Ok(())
}
pub(super) async fn published<R: KvLogEventRepo>(
    case: &fixture::Case<R>,
    frozen: &GuardedPreparationPayload,
) -> Result<()> {
    let writes = case.observations.live.lock().unwrap().clone();
    let e = case.observations.expected()?;
    let mut new_outputs = 0;
    for write in &writes {
        if intended(&e, &write.packet) {
            new_outputs += 1;
            ensure!(
                write.local_applied && (write.promoting_readback || write.prior_exact),
                "C2 new live write preceded LOCAL exactApplied/durable promoting stage"
            );
            ensure!(
                frozen
                    .passes
                    .iter()
                    .any(|p| p.distributions.contains(&write.packet)),
                "C2 NEW live packet was reencrypted or changed frozen bytes"
            );
        } else {
            ensure!(
                write.prior_exact && e.original_packets.contains(&write.packet),
                "C2 unexpected current-Vault packet is neither exact original import nor intended NEW output"
            );
        }
    }
    ensure!(
        new_outputs > 0,
        "C2 accepted promotion UNREACHED: no actual live publication"
    );
    let witness = case.h.witness().await?;
    for pass in &frozen.passes {
        for packet in &pass.distributions {
            let data = packet.clone().to_distribution_data()?;
            let receiver = data
                .secret_message
                .cipher_text()
                .channel
                .receiver()
                .to_device_id();
            ensure!(
                receiver != *case.h.state.c.device_id(),
                "C2 targets removed peer"
            );
            if receiver == *case.h.state.b.device_id() {
                ensure!(
                    case.h.p.repo.inner.inner.find_one(packet.obj_id()).await?
                        == Some(packet.clone().to_generic()),
                    "C2 own packet is not exact frozen material"
                );
            } else {
                ensure!(
                    receiver == *case.h.state.a.device_id(),
                    "C2 foreign survivor"
                );
                ensure!(
                    case.observations
                        .uploads
                        .lock()
                        .unwrap()
                        .contains(&packet.clone().to_generic())
                        && case.h.state.server_repo.find_one(packet.obj_id()).await?
                            == Some(packet.clone().to_generic()),
                    "C2 peer packet not handed off with exact frozen bytes"
                );
            }
        }
        let ownership = witness
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == pass.pass_id)
            .context("C2 exact claim ownership")?;
        let claim_id = ownership
            .ss_device_event_id
            .clone()
            .context("C2 actual allocated SsDeviceLog key absent")?;
        ensure!(ownership.claim == pass.claim && case.h.p.repo.inner.inner.find_one(claim_id).await?
            .is_some_and(|x| matches!(x, GenericKvLogEvent::SsDeviceLog(c) if c.0.value == pass.claim)),
            "C2 intended claim/key/generation differs");
        actual_log_claim(case, ownership, pass).await?;
        ensure!(ownership.handed_off, "C2 ordinary claim handoff not proven");
    }
    ensure!(
        witness.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && witness.0.value.payload.is_none(),
        "C2 completed ordinary ownership did not scrub private duplicate"
    );
    Ok(())
}
