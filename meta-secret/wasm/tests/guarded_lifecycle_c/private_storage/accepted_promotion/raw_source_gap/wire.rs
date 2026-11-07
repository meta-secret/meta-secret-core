use super::*;
use meta_secret_core::node::db::descriptors::vault_descriptor::VaultLogDescriptor;

pub(super) struct PostErrorVaultRead {
    pub source_id: ArtifactId,
    pub source_error_order: usize,
    pub read_order: usize,
    pub response_success: bool,
    pub exact_unique_source: bool,
}

/// Forward ONLY to the wrapped actual server; never supply an ACK or change a tail.
pub(super) struct Forward {
    pub local: Arc<repo::Observed<InMemKvLogEventRepo>>,
    pub server: Arc<ServerApp<raw_repo::RawRepo>>,
    pub observations: Arc<Observations>,
    pub post_error_vault: Mutex<Vec<PostErrorVaultRead>>,
    pub request_sequence: AtomicUsize,
    pub source_error_order: AtomicUsize,
    pub source_error: Mutex<Option<Vec<String>>>,
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let order = self.request_sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let error_order = self.source_error_order.load(Ordering::SeqCst);
        let after_error_vault = error_order > 0
            && matches!(&request, SyncRequest::Read(read)
                if matches!(&**read, ReadSyncRequest::Vault(v)
                    if v.sender.user_id() == self.observations.expected()?.owner));
        let mut index = None;
        if let SyncRequest::Write(write) = &request {
            match &**write {
                WriteSyncRequest::DeviceTransportNoop(_) => {
                    self.observations.noops.fetch_add(1, Ordering::SeqCst);
                }
                WriteSyncRequest::Event(action) => match &action.payload {
                    SignedActionPayload::GuardedRemovalDecision(guard) => {
                        let e = self.observations.expected()?;
                        ensure!(
                            guard.request_id == e.uuid && guard.event.obj_id() == e.event_id,
                            "raw-gap actual source identity changed"
                        );
                        let durable_signature =
                            proof::signature_before(self.local.inner.clone(), &e, action)
                                .await
                                .unwrap_or(false);
                        let mut sent = self.observations.sent.lock().unwrap();
                        index = Some(sent.len());
                        sent.push(Sent {
                            action: action.clone(),
                            durable_signature,
                            server_ok: false,
                        });
                    }
                    SignedActionPayload::Event(event) => {
                        self.observations
                            .uploads
                            .lock()
                            .unwrap()
                            .push(event.clone());
                    }
                    _ => {}
                },
            }
        }
        let result = Box::pin(self.server.handle_client_request(request)).await;
        if let Some(index) = index {
            self.observations.sent.lock().unwrap()[index].server_ok = result
                .as_ref()
                .is_ok_and(|response| response.clone().ensure_success().is_ok());
            if result.is_err() {
                *self.source_error.lock().unwrap() = result
                    .as_ref()
                    .err()
                    .map(|error| error.chain().map(ToString::to_string).collect());
                let _ = self.source_error_order.compare_exchange(
                    0,
                    order,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
            }
        }
        if after_error_vault {
            let expected = self.observations.expected()?;
            let data = result
                .as_ref()
                .ok()
                .and_then(|response| response.to_data().ok());
            let exact_unique_source = if let (Some(data), Some(action)) =
                (data.as_ref(), self.observations.action())
            {
                let stream =
                    ArtifactId::from(VaultLogDescriptor::from(expected.owner.vault_name.clone()))
                        .fqdn;
                let newest = data
                    .0
                    .iter()
                    .filter_map(|event| match event {
                        GenericKvLogEvent::VaultLog(log) if log.obj_id().fqdn == stream => {
                            Some(log)
                        }
                        _ => None,
                    })
                    .max_by_key(|log| log.obj_id().id.curr);
                let records: Vec<_> = newest
                    .into_iter()
                    .flat_map(|log| log.0.value.liveboard_removal_results.iter())
                    .filter(|record| record.request_id == expected.uuid)
                    .collect();
                match records.as_slice() {
                    [record] => {
                        record.application_state == RemovalApplicationState::Applied
                            && record.outcome == DecisionOutcome::Accepted
                            && record.requester_id == expected.requester
                            && record.target_id == expected.target
                            && record.responder_id == expected.owner.device_id
                            && record.source.as_ref().is_some_and(|source| {
                                source.event_id == expected.event_id
                                    && source.signer == expected.owner.device_id
                                    && proof::hash(&action)
                                        .is_ok_and(|hash| hash == source.signed_action_hash)
                            })
                    }
                    _ => false,
                }
            } else {
                false
            };
            self.post_error_vault
                .lock()
                .unwrap()
                .push(PostErrorVaultRead {
                    source_id: expected.event_id,
                    source_error_order: error_order,
                    read_order: order,
                    response_success: data.is_some(),
                    exact_unique_source,
                });
        }
        result
    }
}
