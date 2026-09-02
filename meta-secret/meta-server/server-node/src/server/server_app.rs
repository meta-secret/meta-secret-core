use std::sync::Arc;

use crate::server::server_data_sync::ServerSyncGateway;
use crate::server::state_invalidation::{
    NoopStateInvalidationPublisher, StateInvalidation, StateInvalidationPublisher,
    StateInvalidationScope,
};
use anyhow::{Result, bail};
use flume::{Receiver, RecvError, Sender};
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::node::api::{
    DataEventsResponse, DataSyncResponse, ReadSyncRequest, ServerTailRequest, ServerTailResponse,
    SyncRequest, WriteSyncRequest,
};
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::device::device_creds::DeviceCreds;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;
use meta_secret_core::node::db::events::generic_log_event::ToGenericEvent;
use meta_secret_core::node::db::events::object_id::Next;
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use tracing::{error, info, instrument, warn};

pub struct ServerRequest {
    request: SyncRequest,
    response_sender: Sender<DataSyncResponse>,
}

pub struct MetaServerDataTransfer {
    request_sender: Sender<ServerRequest>,
    request_receiver: Receiver<ServerRequest>,
}

impl Default for MetaServerDataTransfer {
    fn default() -> Self {
        let (request_sender, request_receiver) = flume::bounded(10);
        Self {
            request_sender,
            request_receiver,
        }
    }
}

impl MetaServerDataTransfer {
    pub async fn send_request(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let (response_sender, response_receiver) = flume::bounded(1);
        self.request_sender
            .send_async(ServerRequest {
                request,
                response_sender,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to enqueue server request: {:?}", e))?;
        response_receiver
            .recv_async()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get server response: {:?}", e))
    }

    async fn receive_request(&self) -> Result<ServerRequest, RecvError> {
        self.request_receiver.recv_async().await
    }
}

pub struct ServerApp<Repo: KvLogEventRepo> {
    data_sync: Arc<ServerSyncGateway<Repo>>,
    p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: Arc<PersistentCredentials<Repo>>,
    data_transfer: Arc<MetaServerDataTransfer>,
    invalidation_publisher: Arc<dyn StateInvalidationPublisher>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub fn new(repo: Arc<Repo>, master_key: TransportSk) -> Result<Self> {
        Self::new_with_invalidation_publisher(
            repo,
            master_key,
            Arc::new(NoopStateInvalidationPublisher),
        )
    }

    pub fn new_with_invalidation_publisher(
        repo: Arc<Repo>,
        master_key: TransportSk,
        invalidation_publisher: Arc<dyn StateInvalidationPublisher>,
    ) -> Result<Self> {
        let p_obj = Arc::new(PersistentObject::new(repo));
        let data_sync = Arc::new(ServerSyncGateway::new(
            p_obj.clone(),
            invalidation_publisher.clone(),
        ));
        let creds_repo = Arc::new(PersistentCredentials {
            p_obj: p_obj.clone(),
            master_key: master_key.clone(),
        });
        let data_transfer = Arc::new(MetaServerDataTransfer::default());

        Ok(Self {
            data_sync,
            p_obj,
            creds_repo,
            data_transfer,
            invalidation_publisher,
        })
    }

    pub fn get_data_transfer(&self) -> Arc<MetaServerDataTransfer> {
        self.data_transfer.clone()
    }

    pub async fn run(&self) -> Result<()> {
        info!("Run server_app service");

        let device_creds = self.get_creds().await?;
        info!("Server initialized with device: {:?}", &device_creds.device);

        loop {
            match self.data_transfer.receive_request().await {
                Ok(ServerRequest {
                    request,
                    response_sender,
                }) => {
                    let response = match self.handle_client_request(request).await {
                        Ok(response) => response,
                        Err(error) => {
                            error!(?error, "Error processing request");
                            DataSyncResponse::Error {
                                msg: format!("Error processing client request: {error:?}"),
                            }
                        }
                    };
                    if response_sender.send_async(response).await.is_err() {
                        warn!("Client disconnected before its server response was ready");
                    }
                }
                Err(e) => {
                    error!("Error receiving message: {:?}", e);
                    return Err(e.into());
                }
            }

            async_std::task::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    pub async fn init(&self) -> Result<DeviceCreds> {
        let device_creds = self.get_creds().await?;
        Ok(device_creds)
    }

    #[instrument(skip(self))]
    pub async fn handle_client_request(
        &self,
        sync_message: SyncRequest,
    ) -> Result<DataSyncResponse> {
        let init_result = self.init().await;
        if let Err(err) = &init_result {
            error!("ServerApp failed to start: {:?}", err);
        }

        let server_creds = init_result?;

        match sync_message {
            SyncRequest::Read(read_request) => match *read_request {
                ReadSyncRequest::Vault(request) => {
                    let new_events = self.data_sync.vault_replication(request).await?;
                    Ok(DataSyncResponse::Data(DataEventsResponse(new_events)))
                }
                ReadSyncRequest::SsRequest(request) => {
                    let new_events = self
                        .data_sync
                        .ss_replication(request, server_creds.device.device_id.clone())
                        .await?;
                    Ok(DataSyncResponse::Data(DataEventsResponse(new_events)))
                }
                ReadSyncRequest::SsRecoveryCompletion(recovery_completion) => {
                    let vault_name = recovery_completion.vault_name;
                    let claim_id = recovery_completion.recovery_id.claim_id.id;
                    let sender_id = recovery_completion.recovery_id.sender;
                    let receiver_id = recovery_completion.recovery_id.distribution_id.receiver;
                    let receiver_status = recovery_completion.receiver_status;
                    let maybe_ss_log_event = self
                        .p_obj
                        .find_tail_event(SsLogDescriptor::from(vault_name.clone()))
                        .await?;

                    match maybe_ss_log_event {
                        None => {
                            bail!("No SS log found for vault: {:?}", &vault_name)
                        }
                        Some(ss_log_event) => {
                            let ss_log_data = ss_log_event.to_data();
                            let Some(current_claim) = ss_log_data.claims.get(&claim_id) else {
                                bail!(
                                    "Recovery completion references missing claim: {:?}",
                                    claim_id
                                );
                            };
                            info!(
                                ?claim_id,
                                ?sender_id,
                                ?receiver_id,
                                ?receiver_status,
                                statuses = ?current_claim.status.statuses,
                                "applying recovery completion"
                            );
                            let updated_ss_log_data = ss_log_data.complete_with_receiver_status(
                                claim_id.clone(),
                                sender_id,
                                receiver_id,
                                receiver_status,
                            );
                            let updated_claim = updated_ss_log_data
                                .claims
                                .get(&claim_id)
                                .expect("recovery claim was checked above");
                            info!(
                                ?claim_id,
                                statuses = ?updated_claim.status.statuses,
                                "recovery completion applied"
                            );
                            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                            let new_ss_log_obj = p_ss
                                .create_new_ss_log_object(updated_ss_log_data, vault_name.clone())
                                .await?;
                            self.p_obj
                                .repo
                                .save(new_ss_log_obj.clone().to_generic())
                                .await?;
                            let commit_log = vec![new_ss_log_obj.to_generic()];
                            self.invalidation_publisher.publish(StateInvalidation::new(
                                vault_name,
                                StateInvalidationScope::SsClaims,
                            ));
                            Ok(DataSyncResponse::Data(DataEventsResponse(commit_log)))
                        }
                    }
                }
                ReadSyncRequest::ServerTail(ServerTailRequest { sender }) => {
                    let p_device_log = PersistentDeviceLog {
                        p_obj: self.p_obj.clone(),
                    };
                    let device_log_tail = p_device_log
                        .find_tail_id(&sender.user_id())
                        .await?
                        .map(|tail_id| tail_id.next());

                    let p_ss = PersistentSharedSecret {
                        p_obj: self.p_obj.clone(),
                    };

                    let ss_device_log_free_id = p_ss
                        .find_ss_device_log_tail_id(&sender.device.device_id)
                        .await?
                        .map(|tail_id| tail_id.next());

                    let response = ServerTailResponse {
                        device_log_tail,
                        ss_device_log_tail: ss_device_log_free_id,
                    };

                    let data_sync_response = DataSyncResponse::ServerTailResponse(response);
                    Ok(data_sync_response)
                }
            },
            SyncRequest::Write(write_request) => match *write_request {
                WriteSyncRequest::Event(event) => {
                    info!("Received new event: {:?}", event);
                    self.data_sync
                        .handle_write(server_creds.device, event)
                        .await?;
                    Ok(DataSyncResponse::Empty)
                }
            },
        }
    }

    pub async fn get_creds(&self) -> Result<DeviceCreds> {
        self.creds_repo
            .get_or_generate_device_creds(DeviceName::server())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::MetaServerDataTransfer;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::api::{
        DataSyncResponse, ReadSyncRequest, ServerTailRequest, SyncRequest,
    };
    use std::sync::Arc;

    fn server_tail_request(
        sender: meta_secret_core::node::common::model::user::common::UserData,
    ) -> SyncRequest {
        SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(ServerTailRequest {
            sender,
        })))
    }

    #[tokio::test]
    async fn concurrent_requests_receive_their_own_responses() {
        let registry = FixtureRegistry::empty();
        let request_a = server_tail_request(registry.state.user_creds.client.user());
        let request_b = server_tail_request(registry.state.user_creds.client_b.user());
        let transport = Arc::new(MetaServerDataTransfer::default());

        let first_transport = transport.clone();
        let first_request = request_a.clone();
        let first = tokio::spawn(async move { first_transport.send_request(first_request).await });

        let second_transport = transport.clone();
        let second_request = request_b.clone();
        let second =
            tokio::spawn(async move { second_transport.send_request(second_request).await });

        let first_envelope = transport.receive_request().await.unwrap();
        let second_envelope = transport.receive_request().await.unwrap();

        for envelope in [second_envelope, first_envelope] {
            let response = if envelope.request == request_a {
                DataSyncResponse::Error {
                    msg: "response-a".to_owned(),
                }
            } else {
                assert_eq!(envelope.request, request_b);
                DataSyncResponse::Error {
                    msg: "response-b".to_owned(),
                }
            };
            envelope.response_sender.send_async(response).await.unwrap();
        }

        assert_eq!(
            first.await.unwrap().unwrap(),
            DataSyncResponse::Error {
                msg: "response-a".to_owned()
            }
        );
        assert_eq!(
            second.await.unwrap().unwrap(),
            DataSyncResponse::Error {
                msg: "response-b".to_owned()
            }
        );
    }
}
