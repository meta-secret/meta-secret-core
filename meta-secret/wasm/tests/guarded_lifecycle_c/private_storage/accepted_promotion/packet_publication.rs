//! Real LOCAL-Applied first/later-Pass packet interruptions, without synthetic outcomes.
use super::*;
use async_trait::async_trait;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;

#[derive(Clone)]
struct Target {
    expected: Expected,
    packet: SsWorkflowObject,
    observations: Arc<Observations>,
    durable_lost_reply: bool,
}
#[derive(Clone)]
struct PacketAttempt {
    order: usize,
    returned: usize,
    event: Box<GenericKvLogEvent>,
    replace: bool,
    persisted: bool,
    error_after_persist: bool,
}
#[derive(Clone)]
struct PacketRead {
    order: usize,
    returned: usize,
    id: ArtifactId,
    actual: Option<Box<GenericKvLogEvent>>,
    success: bool,
}

/// Every write remains visible to Observed<R> ABOVE this exact backend fault.
struct PacketRepo<R: KvLogEventRepo> {
    inner: Arc<R>,
    target: Mutex<Option<Target>>,
    armed: AtomicBool,
    hits: AtomicUsize,
    selected: Mutex<Vec<(SsWorkflowObject, bool)>>,
    clock: AtomicUsize,
    attempts: Mutex<Vec<PacketAttempt>>,
    reads: Mutex<Vec<PacketRead>>,
}
impl<R: KvLogEventRepo> PacketRepo<R> {
    fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            target: Mutex::new(None),
            armed: false.into(),
            hits: 0.into(),
            selected: Mutex::new(vec![]),
            clock: 0.into(),
            attempts: Mutex::new(vec![]),
            reads: Mutex::new(vec![]),
        }
    }
    fn arm(&self, expected: Expected, packet: SsWorkflowObject, observations: Arc<Observations>) {
        *self.target.lock().unwrap() = Some(Target {
            expected,
            packet,
            observations,
            durable_lost_reply: false,
        });
        self.armed.store(true, Ordering::SeqCst);
    }
    fn arm_lost_reply(
        &self,
        expected: Expected,
        packet: SsWorkflowObject,
        observations: Arc<Observations>,
    ) {
        *self.target.lock().unwrap() = Some(Target {
            expected,
            packet,
            observations,
            durable_lost_reply: true,
        });
        self.armed.store(true, Ordering::SeqCst);
    }
    fn tick(&self) -> usize {
        self.clock.fetch_add(1, Ordering::SeqCst) + 1
    }
    fn lost_reply_case(&self) -> bool {
        self.target
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|t| t.durable_lost_reply)
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        // Unfiltered ALL-write ledger for the separate durable-reply-loss case.
        let attempt = if self.lost_reply_case() {
            let mut attempts = self.attempts.lock().unwrap();
            let index = attempts.len();
            attempts.push(PacketAttempt {
                order: self.tick(),
                returned: 0,
                event: Box::new(event.clone()),
                replace,
                persisted: false,
                error_after_persist: false,
            });
            Some(index)
        } else {
            None
        };
        let target = self.target.lock().unwrap().clone();
        if let (
            Some(target),
            GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)),
        ) = (target, &event)
        {
            let actual = packet.clone().to_distribution_data()?;
            let intended = target.packet.clone().to_distribution_data()?;
            // Full packet ID + actual Pass/generation/recipient; changed cipher still selects.
            if packet.obj_id() == target.packet.obj_id()
                && actual.vault_name == target.expected.owner.vault_name
                && actual.claim_id == intended.claim_id
                && actual.secret_message.cipher_text().channel.receiver()
                    == intended.secret_message.cipher_text().channel.receiver()
            {
                let applied = if let Some(action) = target.observations.action() {
                    proof::applied(self.inner.clone(), &target.expected, &action).await?
                } else {
                    false
                };
                self.selected
                    .lock()
                    .unwrap()
                    .push((packet.clone(), applied));
                if self.armed.load(Ordering::SeqCst) && applied {
                    if target.durable_lost_reply {
                        ensure!(
                            packet == &target.packet,
                            "selected packet changed frozen cipher"
                        );
                        let saved = if replace {
                            self.inner.replace(event.clone()).await?
                        } else {
                            self.inner.save(event.clone()).await?
                        };
                        // Direct physical durability evidence is NOT a production Find.
                        ensure!(
                            saved == event.obj_id()
                                && self.inner.find_one(saved).await? == Some(event.clone()),
                            "actual selected Distribution did not persist before lost reply"
                        );
                        self.hits.fetch_add(1, Ordering::SeqCst);
                        if let Some(index) = attempt {
                            let returned = self.tick();
                            let mut attempts = self.attempts.lock().unwrap();
                            attempts[index].persisted = true;
                            attempts[index].error_after_persist = true;
                            attempts[index].returned = returned;
                        }
                        anyhow::bail!("packet publication durable lost reply PRIVATE_SENTINEL");
                    }
                    self.hits.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("packet publication before-write PRIVATE_SENTINEL");
                }
            }
        }
        let outcome = if replace {
            self.inner.replace(event).await
        } else {
            self.inner.save(event).await
        };
        if let Some(index) = attempt {
            let returned = self.tick();
            let mut attempts = self.attempts.lock().unwrap();
            attempts[index].returned = returned;
            attempts[index].persisted = outcome.is_ok();
        }
        outcome
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for PacketRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        if self.lost_reply_case() {
            let order = self.tick();
            let outcome = self.inner.find_one(id.clone()).await;
            let returned = self.tick();
            self.reads.lock().unwrap().push(PacketRead {
                order,
                returned,
                id,
                actual: outcome.as_ref().ok().and_then(Clone::clone).map(Box::new),
                success: outcome.is_ok(),
            });
            return outcome;
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for PacketRepo<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for PacketRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for PacketRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for PacketRepo<R> {}

type Case = fixture::Case<PacketRepo<InMemKvLogEventRepo>>;
#[tokio::test]
async fn first_new_packet_fault_retains_full_stage_and_exact_ordinary_retry() -> Result<()> {
    run(0).await
}
#[tokio::test]
async fn later_pass_first_packet_fault_reuses_actual_prior_claim_keys_and_frozen_outputs()
-> Result<()> {
    run(1).await
}

async fn actual_claim(
    case: &Case,
    pass: &GuardedPreparedPass,
    ownership: &GuardedClaimOwnership,
) -> Result<()> {
    let device_id = ownership
        .ss_device_event_id
        .clone()
        .context("actual nonempty SsDeviceLog key")?;
    let log_id = ownership
        .ss_log_event_id
        .clone()
        .context("actual nonempty SsLog key")?;
    let device: SsDeviceLogObject = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(device_id.clone())
        .await?
        .context("actual durable prior Pass SsDeviceLog")?
        .try_into()?;
    let log = case
        .h
        .p
        .repo
        .inner
        .inner
        .find_one(log_id.clone())
        .await?
        .context("actual durable prior Pass SsLog")?
        .ss_log()?;
    let expected_key = KvKey::from(SsLogDescriptor::from(case.h.state.b.vault_name.clone()));
    let claim = log
        .0
        .value
        .claims
        .get(&pass.claim.id)
        .context("actual prior intended Split claim")?;
    let mut stable = claim.clone();
    stable.status = pass.claim.status.clone();
    stable.client_status = pass.claim.client_status.clone();
    ensure!(
        device.obj_id() == device_id
            && device.0.value == pass.claim
            && ownership.claim == pass.claim
            && ownership.handed_off
            && log.obj_id() == log_id
            && log.obj_id().fqdn == expected_key.obj_id.fqdn
            && log.key().obj_desc == expected_key.obj_desc
            && stable == pass.claim,
        "actual prior Pass keys/type/Vault/generation/claim differ"
    );
    Ok(())
}

async fn run(pass_index: usize) -> Result<()> {
    let repo = Arc::new(PacketRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let case = fixture::Case::new(repo.clone()).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let prepared = case.h.witness().await?;
    super::super::stage::private_crypto(&case.h.state, &prepared)?;
    ensure!(
        frozen.passes.len() == 2 && pass_index < 2,
        "actual TWO-Pass fixture"
    );
    let target = frozen.passes[pass_index]
        .distributions
        .first()
        .context("actual first packet of selected prepared Pass")?
        .clone();
    let e = case.observations.expected()?;
    let body = repo
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual original queued source")?;
    let before = repo.inner.get_db().await;
    repo.arm(e.clone(), target.clone(), case.observations.clone());
    let result = case.gateway().sync(case.h.state.b.user()).await;
    let m = proof::measure(&case).await?;
    m.print(
        &format!("packet Pass-index{pass_index} interrupted"),
        &result,
    );
    let hits = repo.hits.load(Ordering::SeqCst);
    let partial = case.h.witness().await?;
    let allocated: Vec<_> = partial
        .0
        .value
        .claims
        .iter()
        .filter(|c| c.ss_device_event_id.is_some() || c.ss_log_event_id.is_some())
        .cloned()
        .collect();
    let writes: Vec<_> = case
        .observations
        .live
        .lock()
        .unwrap()
        .iter()
        .filter(|w| proof::intended(&e, &w.packet))
        .cloned()
        .collect();
    eprintln!(
        "C2 packet Pass-index{pass_index} actual_fault_hits={hits} intended_attempts={} durable_before_fault={} allocated_claims={} full_stage={} sync_error={}",
        writes.len(),
        writes.iter().filter(|w| w.durable).count(),
        allocated.len(),
        partial.0.value.payload.as_ref() == Some(&frozen),
        result.is_err()
    );
    ensure!(
        hits > 0,
        "actual selected post-LOCAL-Applied packet fault UNREACHED"
    );
    proof::common(&m)?;
    ensure!(
        result.is_err(),
        "partial packet publication falsely completed"
    );
    ensure!(
        partial.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
            && partial.0.value.payload.as_ref() == Some(&frozen)
            && partial
                .0
                .value
                .packets
                .iter()
                .any(|p| p.event_id == target.obj_id() && !p.handed_off),
        "partial stage was completed/scrubbed before selected packet became durable"
    );
    super::super::stage::private_crypto(&case.h.state, &partial)?;
    let selected = repo.selected.lock().unwrap().clone();
    ensure!(
        selected.len() == 1 && selected[0].0 == target && selected[0].1,
        "fault did not capture exact intended cipher after actual LOCALApplied"
    );
    let expected_prior: Vec<_> = frozen
        .passes
        .iter()
        .take(pass_index)
        .flat_map(|p| p.distributions.iter().cloned())
        .collect();
    ensure!(
        writes.len() == expected_prior.len() + 1,
        "first packet boundary of selected prepared Pass differs"
    );
    for (write, intended) in writes.iter().zip(&expected_prior) {
        ensure!(
            write.packet == *intended
                && write.durable
                && write.local_applied
                && write.promoting_readback
                && repo.inner.find_one(intended.obj_id()).await?
                    == Some(intended.clone().to_generic()),
            "prior frozen output was not actually durable/exact before later Pass fault"
        );
    }
    let failed = writes.last().context("actual failed packet observation")?;
    ensure!(
        failed.packet == target
            && !failed.durable
            && failed.local_applied
            && failed.promoting_readback,
        "selected fault was not an actual incomplete promotion write"
    );
    if pass_index == 1 {
        let prior = allocated
            .iter()
            .find(|c| c.pass_id == frozen.passes[0].pass_id)
            .context("later-Pass fault must reach actual prior Pass allocated keys")?;
        actual_claim(&case, &frozen.passes[0], prior).await?;
        eprintln!(
            "C2 packet later-Pass actual prior SsDeviceLog+SsLog BOTH durable=true keys_nonempty=true"
        );
    }
    let signature = partial
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("actual original durable signature")?;
    ensure!(
        repo.inner.find_one(e.event_id.clone()).await? == Some(body.clone()),
        "interruption changed actual original source"
    );
    repo.armed.store(false, Ordering::SeqCst);
    let retry = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print(
        &format!("packet Pass-index{pass_index} actual retry"),
        &retry,
    );
    proof::common(&measured)?;
    retry?;
    proof::published(&case, &frozen).await?;
    let completed = case.h.witness().await?;
    for old in &partial.0.value.claims {
        let current = completed
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == old.pass_id)
            .context("same intended Pass ownership after retry")?;
        ensure!(
            current.claim == old.claim
                && old
                    .ss_device_event_id
                    .as_ref()
                    .is_none_or(|id| current.ss_device_event_id.as_ref() == Some(id))
                && old
                    .ss_log_event_id
                    .as_ref()
                    .is_none_or(|id| current.ss_log_event_id.as_ref() == Some(id)),
            "retry changed generation or reallocated actual prior claim keys"
        );
    }
    if pass_index == 1 {
        let prior = completed
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == frozen.passes[0].pass_id)
            .context("actual prior Pass retained after retry")?;
        actual_claim(&case, &frozen.passes[0], prior).await?;
    }
    let action = case
        .observations
        .action()
        .context("actual original signed send")?;
    let origin = completed
        .0
        .value
        .origin
        .as_ref()
        .context("actual retained origin")?;
    ensure!(
        origin.business_signature.as_ref() == Some(&signature)
            && origin.request_id == e.uuid
            && origin.event_id == e.event_id
            && signature.signed_action_hash == proof::hash(&action)?
            && repo.inner.find_one(e.event_id.clone()).await? == Some(body),
        "ordinary retry changed actual source/UUID/signature/full hash"
    );
    let after = repo.inner.get_db().await;
    let bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, x)| {
                id.fqdn == e.event_id.fqdn && matches!(x, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, x)| (id.clone(), x.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        bodies(&after) == bodies(&before) && repo.hits.load(Ordering::SeqCst) == hits,
        "retry generated another body or fault was not cleared"
    );
    eprintln!(
        "C2 packet Pass-index{pass_index} retry_ok=true same_source_sig_generation_cipher=true actual_prior_claim_key_pairs_reused={} phase_published_payload_scrubbed=true",
        allocated.len()
    );
    Ok(())
}

#[tokio::test]
async fn actual_durable_packet_lost_reply_resolves_publisher_readback_and_repeats_exactly()
-> Result<()> {
    let repo = Arc::new(PacketRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let case = fixture::Case::new(repo.clone()).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let prepared = case.h.witness().await?;
    super::super::stage::private_crypto(&case.h.state, &prepared)?;
    let target = frozen
        .passes
        .first()
        .context("actual first frozen Pass")?
        .distributions
        .iter()
        .find(|packet| {
            (**packet).clone().to_distribution_data().is_ok_and(|d| {
                d.secret_message
                    .cipher_text()
                    .channel
                    .receiver()
                    .to_device_id()
                    == *case.h.state.b.device_id()
            })
        })
        .cloned()
        .context("actual own-addressed frozen packet")?;
    let e = case.observations.expected()?;
    let before = repo.inner.get_db().await;
    let body = before
        .get(&receipt.event_id)
        .cloned()
        .context("actual original source")?;
    repo.arm_lost_reply(e.clone(), target.clone(), case.observations.clone());
    let outcome = case.gateway().sync(case.h.state.b.user()).await;
    // These ledgers contain only actual production calls: take them BEFORE
    // test-side measurement/witness reads. The fault's direct probe bypassed them.
    let attempts = repo.attempts.lock().unwrap().clone();
    let reads = repo.reads.lock().unwrap().clone();
    let measured = proof::measure(&case).await?;
    measured.print("packet durable lost reply", &outcome);
    proof::common(&measured)?;
    ensure!(
        repo.hits.load(Ordering::SeqCst) == 1,
        "actual saved-then-Err packet boundary UNREACHED/repeated"
    );
    let lost: Vec<_> = attempts.iter().filter(|a| a.error_after_persist).collect();
    ensure!(
        lost.len() == 1,
        "durable reply-loss must select exactly one actual output"
    );
    let lost = lost[0];
    ensure!(
        lost.order < lost.returned
            && lost.persisted
            && lost.event.as_ref() == &target.clone().to_generic()
            && lost.replace == before.contains_key(&target.obj_id()),
        "fault did not forward the exact intended packet via the actual save/replace choice"
    );
    let readback = reads
        .iter()
        .find(|r| r.id == target.obj_id() && r.order > lost.returned)
        .context("FIRST actual publisher Find after selected saved-then-Err")?;
    ensure!(
        readback.success
            && readback.returned > readback.order
            && readback.actual.as_deref() == Some(lost.event.as_ref()),
        "publisher did not resolve saved-then-Err with its exact whole-object Find"
    );
    let completions: Vec<_> = attempts
        .iter()
        .filter(|a| {
            matches!(a.event.as_ref(), GenericKvLogEvent::LocalGuardedPreparation(w)
            if w.obj_id() == e.witness_id
                && (w.0.value.packets.iter().any(|p| p.event_id == target.obj_id() && p.handed_off)
                    || w.0.value.phase == GuardedPreparationPhase::AcceptedPublished
                    || w.0.value.payload.is_none()))
        })
        .collect();
    ensure!(
        !completions.is_empty()
            && completions
                .iter()
                .all(|a| a.persisted && a.order > readback.returned),
        "packet handed-off/scrubbed completion preceded required actual publisher readback"
    );
    let live = case.observations.live.lock().unwrap().clone();
    let selected: Vec<_> = live
        .iter()
        .filter(|w| w.packet.obj_id() == target.obj_id())
        .collect();
    ensure!(
        selected.len() == 1
            && selected[0].packet == target
            && selected[0].local_applied
            && selected[0].promoting_readback
            && !selected[0].durable,
        "fault escaped ALL-attempt observer or its skipped success read was mislabelled publisher proof"
    );
    ensure!(
        attempts
            .iter()
            .filter(|a| a.event.obj_id() == target.obj_id())
            .count()
            == 1,
        "publication rewrote the selected packet after its resolved persistence"
    );
    ensure!(
        outcome.is_ok(),
        "actual exact readback did not resolve durable packet reply-loss"
    );
    outcome?;
    proof::published(&case, &frozen).await?;
    let completed = case.h.witness().await?;
    ensure!(
        completed.0.value.previous_attempts == prepared.0.value.previous_attempts,
        "first packet completion changed the original compact history"
    );
    let action = case
        .observations
        .action()
        .context("actual original signed send")?;
    action.verify(&case.h.state.b.device().keys.dsa_pk)?;
    let origin = completed
        .0
        .value
        .origin
        .as_ref()
        .context("retained exact origin")?;
    let signature = origin
        .business_signature
        .as_ref()
        .context("actual durable signature")?;
    ensure!(
        origin.request_id == receipt.request_id
            && origin.event_id == receipt.event_id
            && action.nonce == receipt.event_id.id.curr as u64
            && signature.signed_action_hash == proof::hash(&action)?
            && origin.body_commitment
                == prepared
                    .0
                    .value
                    .origin
                    .as_ref()
                    .context("initial actual origin")?
                    .body_commitment
            && repo.inner.find_one(target.obj_id()).await? == Some(target.clone().to_generic()),
        "resolved packet publication changed original source/nonce/frozen bytes"
    );
    ensure!(
        !serde_json::to_string(&receipt)?.contains("PRIVATE_SENTINEL"),
        "private backend sentinel escaped the public receipt"
    );
    let initial_claims = completed.0.value.claims.clone();
    let initial_attempts = attempts.len();
    repo.armed.store(false, Ordering::SeqCst);
    let repeat = case.gateway().sync(case.h.state.b.user()).await;
    let repeated_attempts = repo.attempts.lock().unwrap().clone();
    super::claim_publication::exact_claim_attempts(
        repeated_attempts.iter().map(|a| a.event.as_ref()),
        &before,
        &frozen,
        &completed,
        &case.h.state.b.user_id(),
    )?;
    let measured = proof::measure(&case).await?;
    measured.print(
        "packet resolved durable reply-loss ordinary repeat",
        &repeat,
    );
    proof::common(&measured)?;
    repeat?;
    proof::published(&case, &frozen).await?;
    ensure!(
        case.h.witness().await? == completed && repo.hits.load(Ordering::SeqCst) == 1,
        "ordinary repeat changed current/previous compact/source/signature or reexecuted the fault"
    );
    for attempt in &repeated_attempts[initial_attempts..] {
        ensure!(
            attempt.event.obj_id() != target.obj_id(),
            "ordinary repeat duplicated a resolved packet save/replace attempt"
        );
        if let GenericKvLogEvent::SsDeviceLog(device) = attempt.event.as_ref() {
            ensure!(
                !initial_claims
                    .iter()
                    .any(|c| c.claim.id == device.0.value.id),
                "ordinary repeat reallocated an existing intended claim"
            );
        }
    }
    for attempt in &repeated_attempts {
        if let GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) =
            attempt.event.as_ref()
        {
            let data = packet.clone().to_distribution_data()?;
            if data.vault_name == e.owner.vault_name && e.passes.contains(&data.claim_id.pass_id) {
                ensure!(
                    frozen
                        .passes
                        .iter()
                        .any(|p| p.distributions.contains(packet))
                        || e.original_packets.contains(packet),
                    "ALL-attempt ledger found changed generation/cipher/removed-recipient output"
                );
            }
        }
    }
    let after = repo.inner.get_db().await;
    let bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, event)| {
                id.fqdn == receipt.event_id.fqdn && matches!(event, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, event)| (id.clone(), event.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        bodies(&after) == bodies(&before) && after.get(&receipt.event_id) == Some(&body),
        "ordinary repeat submitted another user command/source"
    );
    eprintln!(
        "C2 packet durable_loss_hit=1 actual_persist_before_err=true FIRSTpublisherFind_whole_exact=true readback_before_completion=true sync_repeat_ok=true same_source_signature_nonce_cipher=true no_second_packet_write=true"
    );
    Ok(())
}
