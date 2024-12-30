use std::sync::Arc;

use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::{DeviceId, DeviceName};
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::Next;
use crate::node::db::objects::global_index::ServerPersistentGlobalIndex;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::server::request::SyncRequest;
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncApi, DataSyncRequest, DataSyncResponse, ServerSyncGateway,
    ServerTailResponse,
};
use anyhow::Result;
use tracing::{error, info, instrument};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerSyncGateway<Repo>,
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

        let data_sync = ServerSyncGateway {
            p_obj: p_obj.clone(),
        };

        let creds_repo = PersistentCredentials {
            p_obj: p_obj.clone(),
        };

        Ok(Self {
            data_sync,
            p_obj,
            creds_repo,
            server_dt,
        })
    }

    async fn init(&self) -> Result<DeviceCredentials> {
        let device_creds = self.get_creds().await?;

        let gi_obj = ServerPersistentGlobalIndex {
            p_obj: self.data_sync.p_obj.clone(),
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

        let server_creds = init_result?;

        info!("Started ServerApp with: {:?}", &server_creds.device);

        while let Ok(sync_message) = self.server_dt.dt.service_receive().await {
            let result = self
                .handle_client_request(server_creds.clone(), sync_message.clone())
                .await;
            if let Err(err) = &result {
                error!(
                    "Failed handling incoming request: {:?}, with error: {}",
                    sync_message, err
                );
            }

            result?
        }

        Ok(())
    }

    #[instrument(skip(self, server_creds))]
    async fn handle_client_request(
        &self,
        server_creds: DeviceCredentials,
        sync_message: DataSyncRequest,
    ) -> Result<()> {
        match sync_message {
            DataSyncRequest::SyncRequest(request) => {
                let new_events = self
                    .handle_sync_request(request, server_creds.device.device_id.clone())
                    .await?;

                self.server_dt
                    .dt
                    .send_to_client(DataSyncResponse::Data(DataEventsResponse(new_events)))
                    .await;
            }
            DataSyncRequest::Event(event) => {
                info!("Received new event: {:?}", event);
                self.data_sync.handle(server_creds.device, event).await?;
            }
            DataSyncRequest::ServerTailRequest(user) => {
                let p_device_log = PersistentDeviceLog {
                    p_obj: self.p_obj.clone(),
                };
                let device_log_tail = p_device_log
                    .find_tail_id(&user.user_id())
                    .await?
                    .map(|tail_id| tail_id.next());

                let p_ss = PersistentSharedSecret {
                    p_obj: self.p_obj.clone(),
                };

                let ss_device_log_free_id = p_ss
                    .find_ss_device_log_tail_id(&user.device.device_id)
                    .await?
                    .map(|tail_id| tail_id.next());

                let response = ServerTailResponse {
                    device_log_tail,
                    ss_device_log_tail: ss_device_log_free_id,
                };

                let data_sync_response = DataSyncResponse::ServerTailResponse(response);

                self.server_dt.dt.send_to_client(data_sync_response).await;
            }
        }
        Ok(())
    }

    pub async fn handle_sync_request(
        &self,
        request: SyncRequest,
        server_device: DeviceId,
    ) -> Result<Vec<GenericKvLogEvent>> {
        self.data_sync.replication(request, server_device).await
    }

    pub async fn get_creds(&self) -> Result<DeviceCredentials> {
        self.creds_repo
            .get_or_generate_device_creds(DeviceName::server())
            .await
    }
}

#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::data_transfer::MpscDataTransfer;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::server::server_app::{ServerApp, ServerDataTransfer};
    use std::sync::Arc;

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
            let server_dt = Arc::new(ServerDataTransfer {
                dt: MpscDataTransfer::new(),
            });

            Self { server_dt }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::states::ExtendedState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::setup_tracing;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::common::meta_tracing::{client_span, server_span};
    use crate::node::common::model::user::common::UserData;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use anyhow::bail;
    use std::thread;
    use std::time::Duration;
    use tokio::runtime::Builder;
    use tracing::{info, Instrument};

    #[tokio::test]
    #[ignore]
    async fn test_sign_up_one_device() -> anyhow::Result<()> {
        setup_tracing()?;

        let registry = FixtureRegistry::extended().await?;

        run_server(&registry).await?;

        info!("Executing 'sign up' claim");
        let client_p_obj = registry.state.base.empty.p_obj.client.clone();
        let client_user_creds = &registry.state.base.empty.user_creds;
        SignUpClaimTestAction::sign_up(client_p_obj.clone(), client_user_creds)
            .instrument(client_span())
            .await?;

        sync_client(&registry).await?;

        info!("Verify SignUpClaim");
        let client_user = registry.state.base.empty.user_creds.client.user();
        let client_claim_spec = SignUpClaimSpec {
            p_obj: client_p_obj.clone(),
            user: client_user.clone(),
            server_device: registry.state.base.empty.device_creds.server.device.clone(),
        };

        client_claim_spec.verify().instrument(client_span()).await?;

        let client_db = client_p_obj.repo.get_db().await;
        assert_eq!(11, client_db.len());

        async_std::task::sleep(Duration::from_secs(3)).await;

        p_vault_check(&registry).await?;

        async_std::task::sleep(Duration::from_secs(1)).await;

        sync_client(&registry).await?;

        async_std::task::sleep(Duration::from_secs(1)).await;

        server_check(registry, client_user).await?;

        Ok(())
    }

    async fn server_check(
        registry: FixtureRegistry<ExtendedState>,
        client_user: UserData,
    ) -> anyhow::Result<()> {
        let server_app = registry.state.server_app.server_app.clone();
        let server_ss_device_log_events = {
            let ss_desc = SharedSecretDescriptor::SsDeviceLog(client_user.device.device_id.clone())
                .to_obj_desc();

            server_app
                .p_obj
                .get_object_events_from_beginning(ss_desc)
                .instrument(server_span())
                .await?
        };
        info!(
            "SERVER SS device log EVENTS: {:?}",
            server_ss_device_log_events.len()
        );

        let server_db = server_app.p_obj.repo.get_db().await;

        assert_eq!(server_db.len(), 18);

        let server_claim_spec = SignUpClaimSpec {
            p_obj: server_app.p_obj.clone(),
            user: client_user.clone(),
            server_device: registry.state.base.empty.device_creds.server.device,
        };

        server_claim_spec.verify().await?;
        Ok(())
    }

    async fn p_vault_check(registry: &FixtureRegistry<ExtendedState>) -> anyhow::Result<()> {
        let p_vault = PersistentVault {
            p_obj: registry.state.base.empty.p_obj.client.clone(),
        };
        let vault_status = p_vault
            .find(registry.state.base.empty.user_creds.client.user())
            .await?;

        let VaultStatus::Member { .. } = &vault_status else {
            bail!("Client is not a vault member: {:?}", vault_status);
        };
        Ok(())
    }

    async fn sync_client(registry: &FixtureRegistry<ExtendedState>) -> anyhow::Result<()> {
        registry
            .state
            .meta_client_service
            .sync_gateway
            .client_gw
            .sync()
            .await?;
        Ok(())
    }

    async fn run_server(registry: &FixtureRegistry<ExtendedState>) -> anyhow::Result<()> {
        let server_app = registry.state.server_app.server_app.clone();
        server_app.init().await?;

        thread::spawn(move || {
            let rt = Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async { server_app.run().instrument(server_span()).await })
        });
        async_std::task::sleep(Duration::from_secs(1)).await;

        Ok(())
    }
}
