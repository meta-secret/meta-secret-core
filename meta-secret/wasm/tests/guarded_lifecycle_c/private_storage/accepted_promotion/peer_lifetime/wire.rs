use super::*;
use meta_server_node::server::server_app::ServerApp;

pub(super) struct Forward {
    pub local: Arc<repo::Observed<peer_repo::PeerRepo<InMemKvLogEventRepo>>>,
    pub server: Arc<ServerApp<InMemKvLogEventRepo>>,
    pub observations: Arc<Observations>,
    pub acks: Arc<Acks>,
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let mut source_index = None;
        let mut peer = None;
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::DeviceTransportNoop(_) => {
                    self.observations.noops.fetch_add(1, Ordering::SeqCst);
                }
                WriteSyncRequest::Event(action) => {
                    if let SignedActionPayload::GuardedRemovalDecision(g) = &action.payload {
                        let e = self.observations.expected()?;
                        ensure!(
                            g.request_id == e.uuid && g.event.obj_id() == e.event_id,
                            "actual peer source changed"
                        );
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
                        if let GenericKvLogEvent::SsWorkflow(p) = event {
                            let e = self.observations.expected()?;
                            if scoped_peer(&e, p) {
                                peer = Some(p.clone());
                            }
                        }
                    }
                }
            }
        }
        let result = Box::pin(self.server.handle_client_request(request)).await;
        let successful = result
            .as_ref()
            .is_ok_and(|r| r.clone().ensure_success().is_ok());
        if let Some(index) = source_index {
            self.observations.sent.lock().unwrap()[index].server_ok = successful;
        }
        // These are actual response ACKs; attempts alone never enter this vector/set.
        if successful && let Some(peer) = peer {
            self.acks.successes.lock().unwrap().push(peer);
        }
        result // Always original response/error, never a successful stub.
    }
}
