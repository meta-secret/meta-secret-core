//! Real retired Decline → explicit Accepted next command; no synthetic settlement or SS cut.
mod crypto;
mod harness;
mod lifecycle;
mod proof;
mod reopen;
mod repo;
mod tombstone;
mod validation_faults;
mod wire;

use super::*;
use async_trait::async_trait;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        app::meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        common::model::{ApplicationState, VaultFullInfo, crypto::aead::EncryptedMessage},
        db::{
            descriptors::local_removal_envelope::LocalRemovalEnvelopeDescriptor,
            events::vault::vault_log_event::RemoveMemberDecision,
            repo::persistent_credentials::PersistentCredentials,
        },
        security::canonical_json,
    },
    recover_from_shares,
    secret::shared_secret::UserShareDto,
};
use std::collections::{HashMap, HashSet};

type ActualCase<R> = super::super::fixture::Case<R>;
type ActualRepo<R> = ExistingRepo<super::super::repo::Observed<R>>;
type ActualWire<R> = super::super::wire::Forward<R>;
fn digest<T: serde::Serialize>(v: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(v)?)?))
}
fn tick(clock: &AtomicUsize) -> usize {
    clock.fetch_add(1, Ordering::SeqCst) + 1
}
async fn canonical<R: KvLogEventRepo>(
    case: &ActualCase<R>,
) -> Result<(VaultObject, VaultLogObject)> {
    let p = PersistentVault::from(Arc::new(PersistentObject::new(
        case.h.state.server_repo.clone(),
    )));
    Ok((
        p.get_vault(case.h.state.b.vault_name.clone()).await?,
        p.get_vault_log_artifact(case.h.state.b.vault_name.clone())
            .await?,
    ))
}
#[derive(Clone)]
struct Write {
    order: usize,
    replace: bool,
    event: GenericKvLogEvent,
}
#[derive(Clone)]
struct Read {
    order: usize,
    event: GenericKvLogEvent,
}
// Compact, payload-free expectations. No FULL/cipher history crosses a file reopen.
struct Retired {
    witness: LocalGuardedPreparationObject,
    index: LocalGuardedPreparationIndexObject,
    body_hash: String,
    marker_hash: String,
}
struct Queued {
    retired: Retired,
    witness_hash: String,
    payload_hash: String,
    index_hash: String,
    registry_hash: String,
    body_hash: String,
    packet_hashes: HashMap<ArtifactId, String>,
    event_id: ArtifactId,
}
