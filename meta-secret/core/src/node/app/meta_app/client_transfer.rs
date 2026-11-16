//! One local service FIFO with move-only, per-call reply ownership.
use super::messaging::{GenericAppStateRequest, GenericAppStateResponse};
use flume::{Receiver, RecvError, Sender, TrySendError};
use std::{
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum DiagnosticTransferError {
    Full,
    AlreadyOwned,
    Closed,
    Unavailable,
}
impl fmt::Display for DiagnosticTransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DiagnosticTransferError {}

pub struct MetaClientRequestTransfer {
    sender: Sender<MetaClientCall>,
    ownership: Arc<Ownership>,
}
struct Ownership {
    state: Mutex<ReceiverOwnership>,
    closed: AtomicBool,
}
enum ReceiverOwnership {
    Unclaimed(Receiver<MetaClientCall>),
    Active,
    Closed,
}
pub struct MetaClientRunGuard {
    receiver: Receiver<MetaClientCall>,
    ownership: Arc<Ownership>,
}
pub struct MetaClientReply(Sender<GenericAppStateResponse>);
pub struct MetaClientCall {
    request: GenericAppStateRequest,
    reply: Option<MetaClientReply>,
}
impl fmt::Debug for MetaClientCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MetaClientCall").field(&self.kind()).finish()
    }
}
impl MetaClientCall {
    pub fn kind(&self) -> &'static str {
        self.request.kind()
    }
    pub fn into_parts(self) -> (GenericAppStateRequest, Option<MetaClientReply>) {
        (self.request, self.reply)
    }
}
impl MetaClientReply {
    pub fn respond(self, response: GenericAppStateResponse) {
        // A dropped caller is normal. Never block or format a payload-bearing error.
        let _ = self.0.try_send(response);
    }
}
impl MetaClientRunGuard {
    pub async fn receive_call(&self) -> Result<MetaClientCall, RecvError> {
        self.receiver.recv_async().await
    }
}
impl Drop for MetaClientRunGuard {
    fn drop(&mut self) {
        self.ownership.closed.store(true, Ordering::SeqCst);
        // Poison recovery is cleanup-only: no admission or resumed execution.
        let mut state = self
            .ownership
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = ReceiverOwnership::Closed;
        while self.receiver.try_recv().is_ok() {}
        // Dropping the sole receiver also releases capacity-blocked producers.
    }
}
impl Default for MetaClientRequestTransfer {
    fn default() -> Self {
        Self::new()
    }
}
impl MetaClientRequestTransfer {
    pub fn new() -> Self {
        let (sender, receiver) = flume::bounded(10);
        Self {
            sender,
            ownership: Arc::new(Ownership {
                state: Mutex::new(ReceiverOwnership::Unclaimed(receiver)),
                closed: AtomicBool::new(false),
            }),
        }
    }
    pub async fn send_to_service_and_get(
        &self,
        request: GenericAppStateRequest,
    ) -> Result<GenericAppStateResponse, RecvError> {
        let (sender, receiver) = flume::bounded(1);
        let call = MetaClientCall {
            request,
            reply: Some(MetaClientReply(sender)),
        };
        self.enqueue(call).await?;
        receiver.recv_async().await
    }
    pub async fn send_to_service(&self, request: GenericAppStateRequest) {
        let _ = self
            .enqueue(MetaClientCall {
                request,
                reply: None,
            })
            .await;
    }
    async fn enqueue(&self, call: MetaClientCall) -> Result<(), RecvError> {
        if self.ownership.closed.load(Ordering::SeqCst) {
            return Err(RecvError::Disconnected);
        }
        self.sender
            .send_async(call)
            .await
            .map_err(|_| RecvError::Disconnected)?;
        // Last-receiver drop need not clear flume's buffer. A correlated caller
        // still owns its reply receiver and releases it if enqueue raced close.
        if self.ownership.closed.load(Ordering::SeqCst) {
            return Err(RecvError::Disconnected);
        }
        Ok(())
    }
    pub fn service_drain(
        &self,
    ) -> Result<std::vec::IntoIter<MetaClientCall>, DiagnosticTransferError> {
        let state = self
            .ownership
            .state
            .lock()
            .map_err(|_| DiagnosticTransferError::Unavailable)?;
        match &*state {
            ReceiverOwnership::Unclaimed(receiver)
                if !self.ownership.closed.load(Ordering::SeqCst) =>
            {
                Ok(receiver.drain().collect::<Vec<_>>().into_iter())
            }
            ReceiverOwnership::Active => Err(DiagnosticTransferError::AlreadyOwned),
            _ => Err(DiagnosticTransferError::Closed),
        }
    }
    pub fn requeue_call(&self, call: MetaClientCall) -> Result<(), DiagnosticTransferError> {
        let state = self
            .ownership
            .state
            .lock()
            .map_err(|_| DiagnosticTransferError::Unavailable)?;
        match &*state {
            ReceiverOwnership::Unclaimed(_) if !self.ownership.closed.load(Ordering::SeqCst) => {
                self.sender.try_send(call).map_err(|error| match error {
                    TrySendError::Full(_) => DiagnosticTransferError::Full,
                    TrySendError::Disconnected(_) => DiagnosticTransferError::Closed,
                })
            }
            ReceiverOwnership::Active => Err(DiagnosticTransferError::AlreadyOwned),
            _ => Err(DiagnosticTransferError::Closed),
        }
    }
    pub fn claim_run(&self) -> Result<MetaClientRunGuard, DiagnosticTransferError> {
        let mut state = self
            .ownership
            .state
            .lock()
            .map_err(|_| DiagnosticTransferError::Unavailable)?;
        if self.ownership.closed.load(Ordering::SeqCst) {
            return Err(DiagnosticTransferError::Closed);
        }
        match &*state {
            ReceiverOwnership::Active => return Err(DiagnosticTransferError::AlreadyOwned),
            ReceiverOwnership::Closed => return Err(DiagnosticTransferError::Closed),
            ReceiverOwnership::Unclaimed(_) => {}
        }
        let ReceiverOwnership::Unclaimed(receiver) =
            std::mem::replace(&mut *state, ReceiverOwnership::Active)
        else {
            unreachable!("ownership checked under the same lock")
        };
        Ok(MetaClientRunGuard {
            receiver,
            ownership: self.ownership.clone(),
        })
    }
}
