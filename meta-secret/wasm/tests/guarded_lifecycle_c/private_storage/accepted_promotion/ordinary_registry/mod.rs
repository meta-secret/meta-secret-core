//! Ordinary signer/outbox ownership, distinct from explicit checked resume.
mod negative;
mod positive;
mod wire;

use super::super::stage::private_crypto;
use super::*;
use meta_secret_core::node::{
    app::orchestrator::MetaOrchestrator,
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            local_removal_envelope::RemovalEnvelopeMode,
            vault::{
                device_log_event::DeviceLogObject,
                vault_log_event::{RemoveMemberDecision, VaultActionEvent, VaultActionUpdateEvent},
            },
        },
        objects::local_removal_envelope::PersistentRemovalEnvelope,
    },
};

type Case = fixture::Case<InMemKvLogEventRepo>;
fn gateway(
    case: &Case,
    wire: Arc<wire::Forward>,
) -> SyncGateway<ExistingRepo<repo::Observed<InMemKvLogEventRepo>>, wire::Forward> {
    SyncGateway::new(
        "actual-C2-ordinary-registry".into(),
        case.h.p.clone(),
        wire,
        case.h.state.master.clone(),
    )
}
