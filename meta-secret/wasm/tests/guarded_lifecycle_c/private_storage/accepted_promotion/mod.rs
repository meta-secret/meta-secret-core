//! C2: real signed producer/server/import/publication, never synthesized proof.
mod claim_publication;
mod coherent_source;
mod fixture;
mod happy;
mod membership_import_retry;
mod ordinary_registry;
mod packet_publication;
mod peer_lifetime;
mod proof;
mod raw_source_gap;
mod reopen;
mod repo;
mod signature_persistence;
mod transport_retirement;
mod wire;

use super::{repo::Repo as ExistingRepo, support::*, *};
use meta_secret_core::node::{
    common::model::{device::common::DeviceId, meta_pass::MetaPasswordId, user::common::UserId},
    db::events::{
        shared_secret_event::{SsDeviceLogObject, SsWorkflowObject},
        vault::liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
    },
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize},
};

#[derive(Clone)]
struct Expected {
    owner: UserId,
    requester: DeviceId,
    target: DeviceId,
    uuid: String,
    event_id: ArtifactId,
    witness_id: ArtifactId,
    accepted: bool,
    passes: Vec<MetaPasswordId>,
    original_packets: Vec<SsWorkflowObject>,
    frozen: GuardedPreparationPayload,
}
#[derive(Clone)]
struct Sent {
    action: SignedAction,
    durable_signature: bool,
    server_ok: bool,
}
#[derive(Clone)]
struct LiveWrite {
    packet: SsWorkflowObject,
    local_applied: bool,
    promoting_readback: bool,
    prior_exact: bool,
    durable: bool,
}
#[derive(Default)]
struct Observations {
    owner: Mutex<Option<UserId>>,
    expected: Mutex<Option<Expected>>,
    sent: Mutex<Vec<Sent>>,
    live: Mutex<Vec<LiveWrite>>,
    claims: Mutex<Vec<SsDeviceLogObject>>,
    claim_writes: Mutex<Vec<GenericKvLogEvent>>,
    uploads: Mutex<Vec<GenericKvLogEvent>>,
    noops: AtomicUsize,
    fail_second: AtomicBool,
    post_applied_writes: AtomicUsize,
    fault_hits: AtomicUsize,
}

fn current_packet(e: &Expected, packet: &SsWorkflowObject) -> bool {
    matches!(packet, SsWorkflowObject::Distribution(x)
        if x.value.vault_name == e.owner.vault_name || e.passes.contains(&x.value.claim_id.pass_id))
}
fn current_claim_record(owner: &UserId, event: &GenericKvLogEvent) -> bool {
    use meta_secret_core::node::db::descriptors::shared_secret_descriptor::{
        SsDeviceLogDescriptor, SsLogDescriptor,
    };
    match event {
        GenericKvLogEvent::SsDeviceLog(x) => {
            x.0.value.vault_name == owner.vault_name
                || x.obj_id().fqdn
                    == ArtifactId::from(SsDeviceLogDescriptor::from(owner.device_id.clone())).fqdn
        }
        GenericKvLogEvent::SsLog(x) => {
            x.obj_id().fqdn
                == ArtifactId::from(SsLogDescriptor::from(owner.vault_name.clone())).fqdn
                || x.0
                    .value
                    .claims
                    .values()
                    .any(|c| c.vault_name == owner.vault_name)
        }
        _ => false,
    }
}
fn nonempty_claim_record(event: &GenericKvLogEvent) -> bool {
    match event {
        GenericKvLogEvent::SsDeviceLog(_) => true,
        GenericKvLogEvent::SsLog(x) => !x.0.value.claims.is_empty(),
        _ => false,
    }
}
impl Observations {
    fn expected(&self) -> Result<Expected> {
        self.expected
            .lock()
            .unwrap()
            .clone()
            .context("C2 observer not configured")
    }
    fn action(&self) -> Option<SignedAction> {
        self.sent.lock().unwrap().first().map(|s| s.action.clone())
    }
}
