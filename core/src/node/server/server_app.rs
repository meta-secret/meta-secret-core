use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info, instrument};

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::ServerPersistentGlobalIndex;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::SyncRequest;
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncApi, DataSyncRequest, DataSyncResponse, ServerDataSync, ServerTailResponse,
};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerDataSync<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: CredentialsRepo<Repo>,
    server_dt: Arc<ServerDataTransfer>,
}

pub struct ServerDataTransfer {
    pub dt: MpscDataTransfer<DataSyncRequest, DataSyncResponse>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub fn new(repo: Arc<Repo>, server_dt: Arc<ServerDataTransfer>) -> Result<Self> {
        let p_obj = {
            let obj = PersistentObject::new(repo);
            Arc::new(obj)
        };

        let data_sync = ServerDataSync {
            persistent_obj: p_obj.clone(),
        };

        let creds_repo = CredentialsRepo { p_obj: p_obj.clone() };

        Ok(Self { data_sync, p_obj, creds_repo, server_dt })
    }

    async fn init(&self) -> Result<DeviceCredentials> {
        let device_creds = self.get_creds().await?;

        let gi_obj = ServerPersistentGlobalIndex {
            p_obj: self.data_sync.persistent_obj.clone(),
            server_device: device_creds.device.clone(),
        };

        gi_obj.init().await?;

        Ok(device_creds)
    }

    #[instrument(skip_all)]
    pub async fn run(&self) -> Result<()> {
        info!("Run server app");

        let _ = self.init().await?;

        while let Ok(sync_message) = self.server_dt.dt.service_receive().await {
            self.handle_client_request(sync_message).await?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn handle_client_request(&self, sync_message: DataSyncRequest) -> Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events = self.handle_sync_request(request).await;

                self.server_dt
                    .dt
                    .send_to_client(DataSyncResponse::Data(DataEventsResponse(new_events)))
                    .await;
            }
            DataSyncRequest::Event(event) => {
                self.handle_new_event(event).await?;
            }
            DataSyncRequest::ServerTailRequest(user_id) => {
                let p_device_log = PersistentDeviceLog {
                    p_obj: self.p_obj.clone(),
                };
                let device_log_tail = p_device_log.find_tail_id(&user_id).await?;

                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                let ss_device_log_tail = p_ss.find_device_tail_id(&user_id.device_id).await?;

                let response = ServerTailResponse {
                    device_log_tail,
                    ss_device_log_tail,
                };
                let response = DataSyncResponse::ServerTailResponse(response);

                self.server_dt.dt.send_to_client(response).await;
            }
        }
        Ok(())
    }

    async fn handle_new_event(&self, event: GenericKvLogEvent) -> Result<()> {
        self.data_sync.send(event).await?;
        Ok(())
    }

    pub async fn handle_sync_request(&self, request: SyncRequest) -> Vec<GenericKvLogEvent> {
        let new_events_result = self.data_sync.replication(request).await;

        match new_events_result {
            Ok(data) => {
                //debug!(format!("New events for a client: {:?}", data).as_str());
                data
            }
            Err(_) => {
                error!("Server. Sync Error");
                vec![]
            }
        }
    }

    pub async fn get_creds(&self) -> Result<DeviceCredentials> {
        self.creds_repo
            .get_or_generate_device_creds(DeviceName::server())
            .await
    }
}

#[cfg(test)]
pub mod fixture {
    use std::sync::Arc;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::server::server_app::{ServerApp, ServerDataTransfer};

    pub struct ServerAppFixture {
        pub server_app: Arc<ServerApp<InMemKvLogEventRepo>>,
    }

    impl ServerAppFixture {
        pub fn try_from(registry: &FixtureRegistry<BaseState>) -> anyhow::Result<Self> {
            let repo = registry.state.p_obj.server.repo.clone();
            let server_dt = registry.state.server_dt.server_dt.clone();
            let server_app = Arc::new(ServerApp::new(repo, server_dt)?);
            Ok(Self { server_app })
        }
    }

    pub struct ServerDataTransferFixture {
        pub server_dt: Arc<ServerDataTransfer>,
    }

    impl ServerDataTransferFixture {
        pub fn generate() -> Self {
            let server_dt = Arc::new(ServerDataTransfer { dt: MpscDataTransfer::new() });

            Self { server_dt }
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;
    use tracing::{info, Instrument};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::setup_tracing;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::meta_tracing::server_span;
    use crate::node::db::actions::sign_up_claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up_claim::test_action::SignUpClaimTestAction;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;

    #[tokio::test]
    async fn test_sign_up() -> anyhow::Result<()> {
        setup_tracing()?;

        let registry = FixtureRegistry::extended()?;

        info!("Executing 'sign up' claim");
        let client_p_obj = registry.state.base.p_obj.client.clone();
        let client_user_creds = &registry.state.base.user_creds;
        let vault_status = SignUpClaimTestAction::sign_up(client_p_obj.clone(), client_user_creds)
            .await?;
        
        info!("Verify SignUpClaim");
        let user = vault_status.user();
        let client_claim_spec = SignUpClaimSpec {
            p_obj: registry.state.base.p_obj.client.clone(),
            user: user.clone(),
        };
        client_claim_spec.verify().await?;

        info!("Starting meta client and server");
        tokio::spawn(async {
            registry.state.meta_client_service.client.run()
        });
        let _ = registry.state.server_app.server_app.run();
        info!("meta client and server has started");
        async_std::task::sleep(Duration::from_secs(1)).await;
        
        let _ = registry.state.meta_client_service.sync_gateway.client_gw.sync();
        async_std::task::sleep(Duration::from_secs(1)).await;
        
        let server_app = registry.state.server_app.server_app.clone();
        let server_ss_device_log_events = {
            let ss_desc = SharedSecretDescriptor::SSDeviceLog(user.device.id.clone())
                .to_obj_desc();
            
            server_app
                .p_obj
                .get_object_events_from_beginning(ss_desc)
                .instrument(server_span())
                .await?
        };
        info!("SERVER SS device log EVENTS: {:?}", server_ss_device_log_events.len());

        let db = server_app.p_obj.repo.get_db().await;
        assert_eq!(db.len(), 16);

        let server_claim_spec = SignUpClaimSpec {
            p_obj: server_app.p_obj.clone(),
            user,
        };

        server_claim_spec.verify().await?;

        Ok(())
    }
}
