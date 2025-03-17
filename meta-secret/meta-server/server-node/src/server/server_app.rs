use std::sync::Arc;

use anyhow::{bail, Result};
use meta_secret_core::node::common::model::device::device_creds::DeviceCredentials;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use tracing::{error, info, instrument};
use meta_secret_core::node::api::{DataEventsResponse, DataSyncResponse, ReadSyncRequest, ServerTailRequest, ServerTailResponse, SyncRequest, WriteSyncRequest};
use meta_secret_core::node::common::model::device::common::{DeviceId, DeviceName};
use meta_secret_core::node::common::model::secret::{SecretDistributionType, SsClaim};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;
use meta_secret_core::node::db::events::generic_log_event::ToGenericEvent;
use meta_secret_core::node::db::events::object_id::Next;
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::server::server_data_sync::{DataSyncApi, ServerSyncGateway};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerSyncGateway<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: PersistentCredentials<Repo>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub fn new(repo: Arc<Repo>) -> Result<Self> {
        let p_obj = Arc::new(PersistentObject::new(repo));
        let data_sync = ServerSyncGateway::from(p_obj.clone());
        let creds_repo = PersistentCredentials::from(p_obj.clone());

        Ok(Self {
            data_sync,
            p_obj,
            creds_repo,
        })
    }

    pub async fn init(&self) -> Result<DeviceCredentials> {
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
            SyncRequest::Read(read_request) => match read_request {
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
                            let updated_ss_log_data = ss_log_data.complete(
                                recovery_completion.recovery_id.claim_id.id,
                                recovery_completion.recovery_id.sender,
                            );

                            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
                            let new_ss_log_obj = p_ss
                                .create_new_ss_log_object(updated_ss_log_data, vault_name)
                                .await?;
                            self.p_obj
                                .repo
                                .save(new_ss_log_obj.clone().to_generic())
                                .await?;
                            let commit_log = vec![new_ss_log_obj.to_generic()];
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
            SyncRequest::Write(WriteSyncRequest::Event(event)) => {
                info!("Received new event: {:?}", event);
                self.data_sync
                    .handle_write(server_creds.device, event)
                    .await?;
                Ok(DataSyncResponse::Empty)
            }
        }
    }

    pub async fn get_creds(&self) -> Result<DeviceCredentials> {
        self.creds_repo
            .get_or_generate_device_creds(DeviceName::server())
            .await
    }
}
