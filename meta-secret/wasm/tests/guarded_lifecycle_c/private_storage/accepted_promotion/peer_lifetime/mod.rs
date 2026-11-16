//! Real peer receives its packets while B is paused; retry must not resurrect them.
mod checks;
mod peer_repo;
mod wire;

use super::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    LastDelete,
    ScrubDrop,
}
#[derive(Default)]
struct Acks {
    successes: Mutex<Vec<SsWorkflowObject>>,
}
impl Acks {
    fn exact(&self, e: &Expected) -> HashMap<ArtifactId, SsWorkflowObject> {
        let intended = peer_packets(e);
        self.successes
            .lock()
            .unwrap()
            .iter()
            .filter(|p| intended.contains(p))
            .map(|p| (p.obj_id(), p.clone()))
            .collect()
    }
}
fn peer_packets(e: &Expected) -> Vec<SsWorkflowObject> {
    e.frozen
        .passes
        .iter()
        .flat_map(|p| p.distributions.iter())
        .filter(|p| {
            (*p).clone().to_distribution_data().is_ok_and(|d| {
                d.secret_message
                    .cipher_text()
                    .channel
                    .receiver()
                    .to_device_id()
                    == e.requester
            })
        })
        .cloned()
        .collect()
}
/// Negative retry scope must include a WRONG/new generation or changed cipher.
fn scoped_peer(e: &Expected, p: &SsWorkflowObject) -> bool {
    matches!(p, SsWorkflowObject::Distribution(_))
        && (peer_packets(e)
            .iter()
            .any(|frozen| frozen.obj_id() == p.obj_id())
            || p.clone().to_distribution_data().is_ok_and(|data| {
                data.vault_name == e.owner.vault_name
                    && e.passes.contains(&data.claim_id.pass_id)
                    && data
                        .secret_message
                        .cipher_text()
                        .channel
                        .receiver()
                        .to_device_id()
                        == e.requester
            }))
}
type Case = fixture::Case<peer_repo::PeerRepo<InMemKvLogEventRepo>>;
fn gateway(
    case: &Case,
    wire: Arc<wire::Forward>,
) -> SyncGateway<
    ExistingRepo<repo::Observed<peer_repo::PeerRepo<InMemKvLogEventRepo>>>,
    wire::Forward,
> {
    SyncGateway::new(
        "actual-C2-peer-lifetime".into(),
        case.h.p.clone(),
        wire,
        case.h.state.master.clone(),
    )
}
#[derive(Clone, Copy)]
struct Counts {
    writes: usize,
    uploads: usize,
    acks: usize,
}
fn counts(case: &Case, e: &Expected, acks: &Acks) -> Counts {
    Counts {
        // Stable physical scope, with NO generation/cipher predicate.
        writes: case
            .h
            .p
            .repo
            .inner
            .inner
            .packet_write_attempts
            .lock()
            .unwrap()
            .iter()
            .filter(|packet| scoped_peer(e, packet))
            .count(),
        uploads: case
            .observations
            .uploads
            .lock()
            .unwrap()
            .iter()
            .filter(|x| matches!(x,GenericKvLogEvent::SsWorkflow(p) if scoped_peer(e,p)))
            .count(),
        acks: acks.successes.lock().unwrap().len(),
    }
}

#[tokio::test]
async fn last_local_peer_delete_failure_after_two_real_acks_cannot_resurrect_after_a_delivery()
-> Result<()> {
    run(Fault::LastDelete).await
}
#[tokio::test]
async fn final_full_to_none_witness_drop_retains_full_stage_until_retry_without_peer_resurrection()
-> Result<()> {
    run(Fault::ScrubDrop).await
}
async fn run(fault: Fault) -> Result<()> {
    let inner = Arc::new(peer_repo::PeerRepo::new(Arc::new(
        InMemKvLogEventRepo::default(),
    )));
    let case = Case::new(inner.clone()).await?;
    let (receipt, frozen) = case.prepare(true).await?;
    let e = case.observations.expected()?;
    let peers = peer_packets(&e);
    ensure!(
        peers.len() == 2
            && peers
                .iter()
                .map(ObjIdExtractor::obj_id)
                .collect::<HashSet<_>>()
                .len()
                == 2,
        "actual distinct TWO frozen A outputs required"
    );
    super::super::stage::private_crypto(&case.h.state, &case.h.witness().await?)?;
    let body = inner
        .inner
        .find_one(receipt.event_id.clone())
        .await?
        .context("actual original B source")?;
    let before = inner.inner.get_db().await;
    let acks = Arc::new(Acks::default());
    inner.arm(e.clone(), fault, acks.clone(), case.observations.clone());
    let wire = Arc::new(wire::Forward {
        local: case.h.p.repo.inner.clone(),
        server: case.h.state.server.clone(),
        observations: case.observations.clone(),
        acks: acks.clone(),
    });
    let first = gateway(&case, wire.clone())
        .sync(case.h.state.b.user())
        .await;
    // Capture before test witness reads: this is production's failed None readback.
    let production_drop_reads = inner.drop_readbacks.load(Ordering::SeqCst);
    let m = proof::measure(&case).await?;
    m.print(&format!("peer {fault:?} pause"), &first);
    let partial = case.h.witness().await?;
    let retained = checks::local_peers(inner.inner.clone(), &e).await?;
    let hits = inner.hits.load(Ordering::SeqCst);
    let forwarded = inner.forwarded_deletes.lock().unwrap().len();
    eprintln!(
        "C2 peer {fault:?} actual_fault_hits={hits} distinct_real_A_ACKs={} successful_A_responses={} forwarded_peer_try_deletes={forwarded} retained_local_peers={} production_full_readbacks={production_drop_reads} full_stage={}",
        acks.exact(&e).len(),
        acks.successes.lock().unwrap().len(),
        retained.len(),
        partial.0.value.payload.as_ref() == Some(&frozen)
    );
    ensure!(
        hits == 1,
        "actual selected post-two-ACK peer fault UNREACHED/repeated"
    );
    proof::common(&m)?;
    ensure!(
        first
            .as_ref()
            .err()
            .is_some_and(|x| x.to_string() == "GuardedPreparationUnavailable"),
        "peer cleanup/scrub interruption did not return static unavailable"
    );
    ensure!(
        partial.0.value.phase == GuardedPreparationPhase::AcceptedPromoting
            && partial.0.value.payload.as_ref() == Some(&frozen)
            && acks.exact(&e).len() == 2
            && acks.successes.lock().unwrap().len() == 2,
        "peer handoff failure scrubbed FULL stage or lacked TWO actual ACKs"
    );
    super::super::stage::private_crypto(&case.h.state, &partial)?;
    checks::ordinary(inner.inner.clone(), &e, &partial).await?;
    match fault {
        Fault::LastDelete => ensure!(
            retained.len() == 1
                && forwarded == 1
                && inner.blocked_id.lock().unwrap().as_ref() == Some(&retained[0].obj_id())
                && inner
                    .handed_readbacks
                    .lock()
                    .unwrap()
                    .contains(&retained[0].obj_id()),
            "last exact peer copy was not retained AFTER actual handedOff readback"
        ),
        Fault::ScrubDrop => ensure!(
            retained.is_empty() && forwarded == 2 && production_drop_reads > 0,
            "silent final None replacement did not leave actual FULL before readback error"
        ),
    }
    let signature = partial
        .0
        .value
        .origin
        .as_ref()
        .and_then(|o| o.business_signature.clone())
        .context("original signature")?;
    let paused = counts(&case, &e, &acks);
    ensure!(
        paused.writes >= 2,
        "unfiltered backend observer missed actual TWO A packet writes"
    );
    checks::deliver_to_a(&case, &e).await?;
    inner.armed.store(false, Ordering::SeqCst);
    let retry = gateway(&case, wire).sync(case.h.state.b.user()).await;
    let final_m = proof::measure(&case).await?;
    final_m.print(&format!("peer {fault:?} actual retry"), &retry);
    proof::common(&final_m)?;
    retry?;
    let done = case.h.witness().await?; // Actual stored None readback, never a candidate-only claim.
    let end = counts(&case, &e, &acks);
    ensure!(
        end.writes == paused.writes && end.uploads == paused.uploads && end.acks == paused.acks,
        "retry wrote/reuploaded an already acknowledged intended A packet"
    );
    ensure!(
        done.0.value.phase == GuardedPreparationPhase::AcceptedPublished
            && done.0.value.payload.is_none()
            && checks::local_peers(inner.inner.clone(), &e)
                .await?
                .is_empty(),
        "healthy retry did not durably scrub final None/clear retained outbound copy"
    );
    checks::ordinary(inner.inner.clone(), &e, &done).await?;
    for prior in &partial.0.value.claims {
        let current = done
            .0
            .value
            .claims
            .iter()
            .find(|c| c.pass_id == prior.pass_id)
            .context("same claim ownership")?;
        ensure!(
            prior.ss_device_event_id.is_some()
                && prior.ss_log_event_id.is_some()
                && current.ss_device_event_id == prior.ss_device_event_id
                && current.ss_log_event_id == prior.ss_log_event_id
                && current.claim == prior.claim,
            "retry reallocated actual nonempty claim keys/generation"
        );
    }
    for peer in &peers {
        ensure!(
            case.h
                .state
                .server_repo
                .find_one(peer.obj_id())
                .await?
                .is_none(),
            "server peer queue resurrected after A delivery"
        );
    }
    ensure!(
        done.0
            .value
            .origin
            .as_ref()
            .and_then(|o| o.business_signature.as_ref())
            == Some(&signature)
            && done.0.value.request_id == e.uuid
            && signature.signed_action_hash
                == proof::hash(&case.observations.action().context("actual source")?)?
            && inner.inner.find_one(e.event_id.clone()).await? == Some(body),
        "retry changed actual source/UUID/signature"
    );
    let after = inner.inner.get_db().await;
    let bodies = |db: &super::super::super::fixture::Snapshot| {
        db.iter()
            .filter(|(id, x)| {
                id.fqdn == e.event_id.fqdn && matches!(x, GenericKvLogEvent::DeviceLog(_))
            })
            .map(|(id, x)| (id.clone(), x.clone()))
            .collect::<HashMap<_, _>>()
    };
    ensure!(
        bodies(&before) == bodies(&after),
        "peer retry queued another source/N+1"
    );
    eprintln!(
        "C2 peer {fault:?} retry_ok=true intended_A_write_delta=0 upload_attempt_delta=0 successful_ACK_delta=0 both_actual_claim_pairs_reused=true server_queue_empty=true actual_None_scrub=true"
    );
    Ok(())
}
