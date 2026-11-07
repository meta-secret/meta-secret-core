use super::*;
use meta_server_node::server::server_app::ServerApp;

pub(super) struct Forward<R: KvLogEventRepo> {
    pub local: Arc<repo::Observed<R>>,
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub observations: Arc<Observations>,
}
impl<R: KvLogEventRepo> SyncProtocol for Forward<R> {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let mut source_index = None;
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::DeviceTransportNoop(_) => {
                    self.observations.noops.fetch_add(1, Ordering::SeqCst);
                }
                WriteSyncRequest::Event(action) => {
                    if let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload {
                        let e = self.observations.expected()?;
                        ensure!(
                            guard.request_id == e.uuid && guard.event.obj_id() == e.event_id,
                            "C2 actual source changed its request/event identity"
                        );
                        // RECORD then forward even on missing metadata. No synthetic success.
                        let durable_signature =
                            proof::signature_before(self.local.inner.clone(), &e, action)
                                .await
                                .unwrap_or(false);
                        let mut sent = self.observations.sent.lock().unwrap();
                        source_index = Some(sent.len());
                        sent.push(Sent {
                            action: action.clone(),
                            durable_signature,
                            server_ok: false,
                        });
                    }
                    if let SignedActionPayload::Event(event) = &action.payload {
                        self.observations
                            .uploads
                            .lock()
                            .unwrap()
                            .push(event.clone());
                    }
                }
            }
        }
        let result = Box::pin(self.server.handle_client_request(request)).await;
        if let Some(index) = source_index {
            self.observations.sent.lock().unwrap()[index].server_ok = result
                .as_ref()
                .is_ok_and(|response| response.clone().ensure_success().is_ok());
        }
        result
    }
}
