use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, error, info, instrument, trace};
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::objects::global_index::ServerPersistentGlobalIndex;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::SyncRequest;
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncApi, DataSyncRequest, DataSyncResponse, ServerDataSync, ServerTailResponse,
};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerDataSync<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: PersistentCredentials<Repo>,
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

        let creds_repo = PersistentCredentials { p_obj: p_obj.clone() };

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

    #[instrument(skip(self))]
    pub async fn run(&self) -> Result<()> {
        info!("Run server app");

        let init_result = self.init().await;
        if let Err(err) = &init_result {
            error!("ServerApp failed to start: {:?}", err);
        }

        let device_creds = init_result?;

        info!("Started ServerApp with: {:?}", device_creds.device);

        while let Ok(sync_message) = self.server_dt.dt.service_receive().await {
            let result = self.handle_client_request(sync_message.clone()).await;
            if let Err(err) = &result {
                error!("Failed handling incoming request: {:?}, with error: {}", sync_message, err);
            }

            result?
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_client_request(&self, sync_message: DataSyncRequest) -> Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events = self.handle_sync_request(request).await?;

                self
                    .server_dt
                    .dt
                    .send_to_client(DataSyncResponse::Data(DataEventsResponse(new_events)))
                    .await;
            }
            DataSyncRequest::Event(event) => {
                self.handle_new_event(event).await?;
            }
            DataSyncRequest::ServerTailRequest(user) => {
                let p_device_log = PersistentDeviceLog {
                    p_obj: self.p_obj.clone(),
                };
                let device_log_tail = p_device_log
                    .find_tail_id(&user.user_id())
                    .await?;

                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };
                let ss_device_log_tail = p_ss
                    .find_device_tail_id(&user.device.device_id)
                    .await?;

                let response = ServerTailResponse { device_log_tail, ss_device_log_tail };

                let data_sync_response = DataSyncResponse::ServerTailResponse(response);

                self
                    .server_dt
                    .dt
                    .send_to_client(data_sync_response)
                    .await;
            }
        }
        Ok(())
    }

    async fn handle_new_event(&self, event: GenericKvLogEvent) -> Result<()> {
        self.data_sync.handle(event).await?;
        Ok(())
    }

    pub async fn handle_sync_request(&self, request: SyncRequest) -> Result<Vec<GenericKvLogEvent>> {
        self.data_sync.replication(request).await
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
            let repo = registry.state.empty.p_obj.server.repo.clone();
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
    use std::thread;
    use std::time::Duration;
    use tracing::{info, Instrument};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::setup_tracing;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::meta_tracing::{client_span, server_span};
    use crate::node::db::actions::sign_up_claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up_claim::test_action::SignUpClaimTestAction;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
    use tokio::runtime::{Builder};

    #[tokio::test]
    async fn test_sign_up() -> anyhow::Result<()> {
        setup_tracing()?;

        let registry = FixtureRegistry::extended().await?;

        info!("Executing 'sign up' claim");
        let client_p_obj = registry.state.base.empty.p_obj.client.clone();
        let client_user_creds = &registry.state.base.empty.user_creds;
        let _ = SignUpClaimTestAction::sign_up(client_p_obj.clone(), client_user_creds)
            .instrument(client_span())
            .await?;

        info!("Verify SignUpClaim");
        let client_user = registry.state.base.empty.user_creds.client.user();
        let client_claim_spec = SignUpClaimSpec {
            p_obj: registry.state.base.empty.p_obj.client.clone(),
            user: client_user.clone(),
            server_device: registry.state.base.empty.device_creds.server.device.clone(),
        };
        client_claim_spec
            .verify()
            .instrument(client_span())
            .await?;

        let client_db = registry.state.base.empty.p_obj.client.repo.get_db().await;
        assert_eq!(9, client_db.len());

        let server_app = registry.state.server_app.server_app.clone();
        thread::spawn(move || {
            let rt = Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async { 
                server_app.run().instrument(server_span()).await 
            })
        });

        //let client_service = registry.state.meta_client_service.client;
        //thread::spawn(move || {
        //    let rt = Builder::new_current_thread().enable_all().build().unwrap();
        //    rt.block_on(async {
        //        client_service.run().await;
        //    })
        //});

        async_std::task::sleep(Duration::from_secs(1)).await;

        let _ = registry.state.meta_client_service.sync_gateway.client_gw
            .sync()
            .instrument(client_span())
            .await;

        async_std::task::sleep(Duration::from_secs(1)).await;

        let server_app = registry.state.server_app.server_app.clone();
        let server_ss_device_log_events = {
            let ss_desc = SharedSecretDescriptor::SSDeviceLog(client_user.device.device_id.clone())
                .to_obj_desc();

            server_app
                .p_obj
                .get_object_events_from_beginning(ss_desc)
                .instrument(server_span())
                .await?
        };
        info!("SERVER SS device log EVENTS: {:?}", server_ss_device_log_events.len());

        let server_db = server_app.p_obj.repo.get_db().await;

        assert_eq!(server_db.len(), 19);

        let server_claim_spec = SignUpClaimSpec {
            p_obj: server_app.p_obj.clone(),
            user: client_user.clone(),
            server_device: registry.state.base.empty.device_creds.server.device,
        };

        server_claim_spec.verify().await?;

        Ok(())
    }
}
