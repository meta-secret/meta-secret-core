//! Actual receipt/tail uncertainty and real file reopens; bounded before shared-secret import.
mod harness;
mod lifecycle;
mod proof;
mod reopen;
mod repo;
mod tombstone;
mod wire;

use super::*;
use async_trait::async_trait;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        common::model::{
            secret::{SecretDistributionType, SsClaim},
            user::common::UserData,
        },
        db::{
            descriptors::{
                local_removal_envelope::LocalRemovalEnvelopeDescriptor,
                shared_secret_descriptor::SsLogDescriptor,
            },
            repo::persistent_credentials::PersistentCredentials,
        },
        security::canonical_json,
    },
};
use std::collections::HashMap;

type GenericCase<R> = super::super::fixture::Case<R>;
type ActualRepo<R> = ExistingRepo<super::super::repo::Observed<R>>;
type ActualWire<R> = super::super::wire::Forward<R>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    ServerReplyLoss,
    ReceiptBefore,
    ReceiptDrop,
    ReceiptReadback,
    ReceiptLostReply,
    TailLoss,
    ConfirmedReadback,
}
impl Mode {
    fn repo_fault(self) -> bool {
        matches!(
            self,
            Self::ReceiptBefore
                | Self::ReceiptDrop
                | Self::ReceiptReadback
                | Self::ReceiptLostReply
                | Self::ConfirmedReadback
        )
    }
}
fn tick(clock: &AtomicUsize) -> usize {
    clock.fetch_add(1, Ordering::SeqCst) + 1
}
fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(value)?)?))
}
// Mutable delivery/client status is intentionally excluded; full immutable claim identity is not.
fn claim_identity(claim: &SsClaim) -> Result<String> {
    digest(&(
        &claim.id,
        &claim.dist_claim_id,
        &claim.vault_name,
        &claim.sender,
        &claim.distribution_type,
        &claim.receivers,
    ))
}
async fn canonical<R: KvLogEventRepo>(
    case: &GenericCase<R>,
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
    event: GenericKvLogEvent,
}
#[derive(Clone)]
struct Read {
    order: usize,
    event: GenericKvLogEvent,
}
#[derive(Clone)]
struct BodyChange {
    after_auth_readback: bool,
    deletion: bool,
}
