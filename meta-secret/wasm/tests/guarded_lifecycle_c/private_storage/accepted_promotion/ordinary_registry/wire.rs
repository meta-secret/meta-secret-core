use super::*;

/// Capture EVERY DeviceLog envelope at the real write boundary, then forward it.
pub(super) struct Forward {
    pub inner: Arc<super::super::wire::Forward<InMemKvLogEventRepo>>,
    pub device_actions: Mutex<Vec<SignedAction>>,
    pub writes: AtomicUsize,
    pub noops: AtomicUsize,
}
impl Forward {
    pub fn new(case: &Case) -> Self {
        Self {
            inner: case.wire.clone(),
            device_actions: Mutex::new(vec![]),
            writes: 0.into(),
            noops: 0.into(),
        }
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if let SyncRequest::Write(write) = &request {
            self.writes.fetch_add(1, Ordering::SeqCst);
            match &**write {
                WriteSyncRequest::DeviceTransportNoop(_) => {
                    self.noops.fetch_add(1, Ordering::SeqCst);
                }
                WriteSyncRequest::Event(action)
                    if matches!(
                        action.payload,
                        SignedActionPayload::GuardedRemovalDecision(_)
                            | SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(_))
                    ) =>
                {
                    self.device_actions.lock().unwrap().push(action.clone());
                }
                _ => {}
            }
        }
        // No successful stub, error normalization or fake acknowledgement.
        self.inner.send(request).await
    }
}
