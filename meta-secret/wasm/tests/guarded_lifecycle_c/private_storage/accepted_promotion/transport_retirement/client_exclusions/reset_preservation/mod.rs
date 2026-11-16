//! Existing removed-device Reset; actual public service and real local writer intervals.
mod behavior;
mod fixture;
mod observer;
mod old_writers;
mod owner;
mod proof;
mod wire;

use super::*;
use super::{fixture as prior, observer as prior_repo, wire as prior_wire};
use meta_secret_core::node::{
    app::{
        app_manager_shared::recover_plain_text_by_claim,
        meta_app::messaging::GenericAppStateRequest,
    },
    common::model::{
        VaultFullInfo,
        secret::{ClaimId, RecoveryClientStatus, SsDistributionStatus},
        user::user_creds::UserCreds,
    },
    db::descriptors::{creds::UserCredsDescriptor, shared_secret_descriptor::SsWorkflowDescriptor},
};
use std::{future::Future, pin::Pin, task::Poll};

async fn once<F: Future>(mut f: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(f.as_mut().poll(cx))).await
}
fn outcome(name: &str, phase: &str, result: &Result<()>) {
    eprintln!(
        "RESET_PRESERVATION variant={name} phase={phase} result_ok={}",
        result.is_ok()
    );
}
#[derive(Debug)]
struct TailError;
impl std::fmt::Display for TailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("selected original Reset ServerTail error")
    }
}
impl std::error::Error for TailError {}

struct Latch {
    armed: AtomicBool,
    entered: AtomicBool,
    released: AtomicBool,
    waker: Mutex<Option<std::task::Waker>>,
}
impl Latch {
    fn new() -> Self {
        Self {
            armed: false.into(),
            entered: false.into(),
            released: false.into(),
            waker: Mutex::new(None),
        }
    }
    fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }
    async fn wait(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.entered.store(true, Ordering::SeqCst);
            std::future::poll_fn(|cx| {
                *self.waker.lock().unwrap() = Some(cx.waker().clone());
                if self.released.load(Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
    }
    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
}
