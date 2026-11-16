//! Actual compact-signature persistence faults; every business request reaches real ServerApp.
use super::*;
use async_trait::async_trait;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    Before,
    Drop,
    Readback,
    LostReply,
}
#[derive(Clone)]
struct Target {
    witness: ArtifactId,
    source: ArtifactId,
    uuid: String,
}
/// Below the existing ALL-writes observer; never fabricates a transport response.
struct SignatureRepo<R: KvLogEventRepo> {
    inner: Arc<R>,
    target: Mutex<Option<Target>>,
    mode: Mutex<Option<Fault>>,
    attempts: Mutex<Vec<LocalGuardedPreparationObject>>,
    durable: AtomicUsize,
    lookups: AtomicUsize,
    read_hits: AtomicUsize,
    read_fault: AtomicBool,
}
impl<R: KvLogEventRepo> SignatureRepo<R> {
    fn new(inner: Arc<R>) -> Self {
        Self {
            inner,
            target: Mutex::new(None),
            mode: Mutex::new(None),
            attempts: Mutex::new(vec![]),
            durable: 0.into(),
            lookups: 0.into(),
            read_hits: 0.into(),
            read_fault: false.into(),
        }
    }
    fn arm(&self, e: &Expected, fault: Fault) {
        *self.target.lock().unwrap() = Some(Target {
            witness: e.witness_id.clone(),
            source: e.event_id.clone(),
            uuid: e.uuid.clone(),
        });
        *self.mode.lock().unwrap() = Some(fault);
    }
    fn clear(&self) {
        // Keep exact target/counters: a second Signed-stage write on retry stays observable.
        *self.mode.lock().unwrap() = None;
        self.read_fault.store(false, Ordering::SeqCst);
    }
    async fn write(&self, replace: bool, event: GenericKvLogEvent) -> Result<ArtifactId> {
        let target = self.target.lock().unwrap().clone();
        let selected = match (&event, target) {
            (GenericKvLogEvent::LocalGuardedPreparation(x), Some(t))
                if replace
                    && x.obj_id() == t.witness
                    && x.0.value.phase == GuardedPreparationPhase::Signed
                    && x.0.value.request_id == t.uuid
                    && x.0.value.origin.as_ref().is_some_and(|o| {
                        o.event_id == t.source
                            && o.request_id == t.uuid
                            && o.business_signature.is_some()
                    }) =>
            {
                Some(x.clone())
            }
            _ => None,
        };
        let mode = *self.mode.lock().unwrap();
        if let Some(candidate) = &selected {
            self.attempts.lock().unwrap().push(candidate.clone());
            if mode == Some(Fault::Before) {
                anyhow::bail!("signature before-write PRIVATE_SENTINEL");
            }
            if mode == Some(Fault::Drop) {
                return Ok(event.obj_id());
            }
        }
        let saved = if replace {
            self.inner.replace(event).await?
        } else {
            self.inner.save(event).await?
        };
        if selected.is_some() {
            self.durable.fetch_add(1, Ordering::SeqCst);
            if mode == Some(Fault::Readback) {
                self.read_fault.store(true, Ordering::SeqCst);
            }
            if mode == Some(Fault::LostReply) {
                anyhow::bail!("signature durable lost-reply PRIVATE_SENTINEL");
            }
        }
        Ok(saved)
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> FindOneQuery for SignatureRepo<R> {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let matching = self
            .target
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|t| t.witness == id)
            && !self.attempts.lock().unwrap().is_empty();
        if matching {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            if self.read_fault.load(Ordering::SeqCst) {
                self.read_hits.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("signature exact readback PRIVATE_SENTINEL");
            }
        }
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        Ok(self.find_one(id).await?.map(|x| x.obj_id()))
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> SaveCommand for SignatureRepo<R> {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(false, event.to_generic()).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.write(true, event.to_generic()).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DeleteCommand for SignatureRepo<R> {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl<R: KvLogEventRepo> DbCleanUpCommand for SignatureRepo<R> {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl<R: KvLogEventRepo> KvLogEventRepo for SignatureRepo<R> {}

#[tokio::test]
async fn actual_signature_before_drop_readback_lost_reply_preserve_source_and_durable_retry()
-> Result<()> {
    let mut results = vec![];
    for fault in [
        Fault::Before,
        Fault::Drop,
        Fault::Readback,
        Fault::LostReply,
    ] {
        let result = run(fault).await;
        eprintln!("C2 signature {fault:?} variant_ok={}", result.is_ok());
        results.push(result);
    }
    // Every fresh actual variant runs before any specification failure escapes.
    for result in results {
        result?;
    }
    Ok(())
}

fn signature(witness: &LocalGuardedPreparationObject) -> Option<GuardedBusinessSignature> {
    witness
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
}

async fn run(fault: Fault) -> Result<()> {
    let repo = Arc::new(SignatureRepo::new(Arc::new(InMemKvLogEventRepo::default())));
    let case = fixture::Case::new(repo.clone()).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let prepared = case.h.witness().await?;
    super::super::stage::private_crypto(&case.h.state, &prepared)?;
    let e = case.observations.expected()?;
    let body = repo
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual original queued source")?;
    ensure!(
        frozen.passes.len() == 2
            && prepared.0.value.phase == GuardedPreparationPhase::Queued
            && signature(&prepared).is_none()
            && receipt.event_id == e.event_id
            && receipt.request_id == e.uuid,
        "signature fixture must start with real unsigned Queued TWO-Pass source"
    );
    let before = repo.inner.get_db().await;
    repo.arm(&e, fault);
    let first = case.gateway().sync(case.h.state.b.user()).await;
    // Read the ACTUAL durable backend below only the injected read fault, not through a fake API.
    let stored: LocalGuardedPreparationObject = repo
        .inner
        .find_one(e.witness_id.clone())
        .await?
        .context("actual durable witness after signature fault")?
        .try_into()?;
    let attempts = repo.attempts.lock().unwrap().clone();
    let stored_signature = signature(&stored);
    let sent = case.observations.sent.lock().unwrap().len();
    let noops = case.observations.noops.load(Ordering::SeqCst);
    let unavailable = first
        .as_ref()
        .err()
        .is_some_and(|error| error.to_string() == "GuardedPreparationUnavailable");
    eprintln!(
        "C2 signature {fault:?} first_ok={} static_unavailable={unavailable} signed_write_hits={} durable_signed_writes={} matching_readbacks={} read_fault_hits={} actual_sent={sent} stored_signature={} noops={noops}",
        first.is_ok(),
        attempts.len(),
        repo.durable.load(Ordering::SeqCst),
        repo.lookups.load(Ordering::SeqCst),
        repo.read_hits.load(Ordering::SeqCst),
        stored_signature.is_some()
    );
    ensure!(
        attempts.len() == 1 && repo.lookups.load(Ordering::SeqCst) > 0,
        "actual Signed write/readback boundary UNREACHED"
    );
    ensure!(
        if fault == Fault::Readback {
            repo.read_hits.load(Ordering::SeqCst) > 0
        } else {
            repo.read_hits.load(Ordering::SeqCst) == 0
        },
        "actual exact signature readback fault boundary differs"
    );
    ensure!(
        repo.inner.find_one(e.event_id.clone()).await? == Some(body.clone()) && noops == 0,
        "signature persistence changed actual original source or substituted Noop"
    );
    if fault == Fault::LostReply {
        ensure!(
            first.is_ok() && sent == 1,
            "exact durable readback must resolve lost reply"
        );
    } else {
        ensure!(
            unavailable && sent == 0,
            "uncertain signature escaped before durable proof"
        );
        ensure!(
            stored.0.value.payload.as_ref() == Some(&frozen)
                && ordinary(&repo.inner.get_db().await) == ordinary(&before)
                && case.observations.live.lock().unwrap().is_empty()
                && case.observations.claim_writes.lock().unwrap().is_empty(),
            "withheld signature mutated live K2, claims or frozen private generation"
        );
    }
    match fault {
        Fault::Before | Fault::Drop => ensure!(
            stored == prepared && repo.durable.load(Ordering::SeqCst) == 0,
            "nondurable write changed actual Queued witness"
        ),
        Fault::Readback | Fault::LostReply => ensure!(
            stored_signature == signature(&attempts[0])
                && stored_signature.is_some()
                && repo.durable.load(Ordering::SeqCst) == 1,
            "actual durable signature differs from attempted compact metadata"
        ),
    }
    repo.clear();
    let retry = case.gateway().sync(case.h.state.b.user()).await;
    let measured = proof::measure(&case).await?;
    measured.print(
        &format!("signature {fault:?} healthy same-source retry"),
        &retry,
    );
    proof::common(&measured)?;
    retry?;
    proof::published(&case, &frozen).await?;
    let final_witness = case.h.witness().await?;
    let final_signature =
        signature(&final_witness).context("actual retained original signature")?;
    let action = case
        .observations
        .action()
        .context("actual transmitted source")?;
    ensure!(
        final_signature.signer == action.signer
            && final_signature.stream == action.stream
            && final_signature.nonce == action.nonce
            && final_signature.signature == action.signature
            && final_signature.signed_action_hash == proof::hash(&action)?
            && final_witness.0.value.request_id == e.uuid
            && final_witness
                .0
                .value
                .origin
                .as_ref()
                .is_some_and(|o| o.event_id == e.event_id)
            && repo.inner.find_one(e.event_id.clone()).await? == Some(body),
        "retry changed actual source UUID/ID/signature or full signed-action hash"
    );
    if let Some(original) = stored_signature {
        ensure!(
            final_signature == original,
            "retry replaced an ACTUALLY persisted signature"
        );
    }
    let total_attempts = repo.attempts.lock().unwrap().len();
    let expected_attempts = if matches!(fault, Fault::Before | Fault::Drop) {
        2
    } else {
        1
    };
    let after = repo.inner.get_db().await;
    let source_records = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, event)| {
                id.fqdn == e.event_id.fqdn && matches!(event, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, event)| (id.clone(), event.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    ensure!(
        total_attempts == expected_attempts
            && repo.durable.load(Ordering::SeqCst) == 1
            && source_records(&after) == source_records(&before),
        "retry created another signed generation/body or rewrote an already durable signature"
    );
    eprintln!(
        "C2 signature {fault:?} retry_ok=true total_signed_attempts={total_attempts} durable_signed_writes=1 actual_source_sends=1 exact_frozen_publication=true persisted_signature_reused={}",
        matches!(fault, Fault::Readback | Fault::LostReply)
    );
    Ok(())
}
