//! Actual generic mobile manager caller; locally owned real service.run and replies.
mod cases;
mod fixture;
mod observer;
mod proof;
mod runtime;
mod wire;
use super::*;
use super::{fixture as prior, observer as prior_repo, wire as prior_wire};
use meta_secret_core::node::{
    app::meta_app::{client_transfer::DiagnosticTransferError, messaging::GenericAppStateRequest},
    common::model::{VaultFullInfo, secret::RecoveryClientStatus, user::user_creds::UserCreds},
    db::{
        descriptors::{creds::UserCredsDescriptor, shared_secret_descriptor::SsWorkflowDescriptor},
        objects::persistent_object::PersistentObject,
    },
    security::verify_action,
};
use mobile_common::app_manager::ApplicationManager;
use std::{future::Future, pin::Pin, task::Poll};
type Setup = super::super::super::fixture::Case<super::super::local_repo::Repo>;
async fn once<F: Future>(mut f: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(f.as_mut().poll(cx))).await
}
#[derive(Debug)]
struct TailError;
impl std::fmt::Display for TailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selected actual mobile pre-sync Tail error")
    }
}
impl std::error::Error for TailError {}
#[derive(Debug)]
struct ProofReadError;
impl std::fmt::Display for ProofReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selected actual terminal handoff proof read error")
    }
}
impl std::error::Error for ProofReadError {}
fn result_line_mobile(name: &str, phase: &str, r: &Result<()>) {
    eprintln!(
        "MOBILE_PRESYNC_RESET variant={name} phase={phase} ok={}",
        r.is_ok()
    );
}
