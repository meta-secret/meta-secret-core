//! Genuine pending-context retirement; no synthesized Noop or canonical result.
mod auth_faults;
mod checks;
mod client_exclusions;
mod incoming_current_material;
mod local_repo;
mod opposite_decision;
mod origin_only;
mod owned_lifetime;
mod pending;
mod receipt_tail_faults;
mod tombstone;
mod wire;

use super::super::stage::private_crypto;
use super::*;
use meta_secret_core::node::{
    common::model::{
        IdString, liveboard_producer::producer_source_commitment, user::user_creds::UserCreds,
    },
    db::{
        actions::secret_deletion::SecretDeletionAction,
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            local_removal_envelope::{LocalRemovalEnvelopeObject, RemovalEnvelopeMode},
            vault::{
                device_log_event::DeviceLogObject,
                vault_event::VaultObject,
                vault_log_event::{
                    RemoveMemberRequestEvent, VaultActionEvent, VaultActionRequestEvent,
                    VaultActionUpdateEvent, VaultLogObject,
                },
            },
        },
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_device_log::PersistentDeviceLog,
        },
    },
    device_transport_noop::{
        DeviceTransportNoop, DeviceTransportNoopReceipt, TransportNoopEffect, TransportNoopState,
    },
    security::sign_event,
};

type Case = fixture::Case<local_repo::Repo>;
async fn case() -> Result<(Case, Arc<local_repo::Repo>)> {
    let repo = Arc::new(local_repo::Repo::new());
    Ok((Case::new(repo.clone()).await?, repo))
}
fn gateway(
    case: &Case,
    wire: Arc<wire::Forward>,
) -> SyncGateway<ExistingRepo<repo::Observed<local_repo::Repo>>, wire::Forward> {
    SyncGateway::new(
        "actual-C3-transport-retirement".into(),
        case.h.p.clone(),
        wire,
        case.h.state.master.clone(),
    )
}

async fn canonical(case: &Case) -> Result<(VaultObject, VaultLogObject)> {
    let p = PersistentVault::from(Arc::new(PersistentObject::new(
        case.h.state.server_repo.clone(),
    )));
    Ok((
        p.get_vault(case.h.state.b.vault_name.clone()).await?,
        p.get_vault_log_artifact(case.h.state.b.vault_name.clone())
            .await?,
    ))
}
