use std::sync::Arc;

use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::db::events::object_id::Next;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::server::request::{
    ReadSyncRequest, ServerTailRequest, SyncRequest, WriteSyncRequest,
};
use crate::node::server::server_data_sync::{
    DataEventsResponse, DataSyncApi, DataSyncResponse, ServerSyncGateway, ServerTailResponse,
};
use anyhow::Result;
use tracing::{error, info, instrument};

pub struct ServerApp<Repo: KvLogEventRepo> {
    pub data_sync: ServerSyncGateway<Repo>,
    pub p_obj: Arc<PersistentObject<Repo>>,
    creds_repo: PersistentCredentials<Repo>,
}

impl<Repo: KvLogEventRepo> ServerApp<Repo> {
    pub fn new(repo: Arc<Repo>) -> Result<Self> {
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
                ReadSyncRequest::Ss(request) => {
                    let new_events = self
                        .data_sync
                        .ss_replication(request, server_creds.device.device_id.clone())
                        .await?;
                    Ok(DataSyncResponse::Data(DataEventsResponse(new_events)))
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
                self.data_sync.handle(server_creds.device, event).await?;
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

#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::server::server_app::ServerApp;
    use std::sync::Arc;

    pub struct ServerAppFixture {
        pub server_app: Arc<ServerApp<InMemKvLogEventRepo>>,
    }

    impl ServerAppFixture {
        pub fn try_from(registry: &FixtureRegistry<BaseState>) -> anyhow::Result<Self> {
            let repo = registry.state.empty.p_obj.server.repo.clone();
            let server_app = Arc::new(ServerApp::new(repo)?);
            Ok(Self { server_app })
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::states::ExtendedState;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::app::orchestrator::MetaOrchestrator;
    use crate::node::common::meta_tracing::{client_span, server_span, vd_span};
    use crate::node::common::model::user::common::UserData;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;

    use crate::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
    use anyhow::bail;
    use tracing::{info, Instrument};

    #[tokio::test]
    #[ignore]
    async fn test_sign_up_one_device() -> anyhow::Result<()> {
        //setup_tracing()?;

        let registry = FixtureRegistry::extended().await?;

        init_server(&registry).await?;
        let server_creds_spec = PersistentCredentialsSpec {
            p_obj: registry.state.base.empty.p_obj.server.clone(),
        };
        server_creds_spec.verify_device_creds().await?;

        info!("Executing 'sign up' claim");
        let client_p_obj = registry.state.base.empty.p_obj.client.clone();
        let client_user_creds = &registry.state.base.empty.user_creds;
        SignUpClaimTestAction::sign_up(client_p_obj.clone(), &client_user_creds.client)
            .instrument(client_span())
            .await?;

        sync_client(&registry).await?;
        sync_client(&registry).await?;

        info!("Verify SignUpClaim");
        let client_user = registry.state.base.empty.user_creds.client.user();
        let client_claim_spec = SignUpClaimSpec {
            p_obj: client_p_obj.clone(),
            user: client_user.clone(),
        };

        client_claim_spec.verify().instrument(client_span()).await?;

        let client_db = client_p_obj.repo.get_db().await;
        assert_eq!(17, client_db.len());

        registry
            .state
            .base
            .spec
            .client
            .verify_user_is_a_member()
            .await?;

        sync_client(&registry).await?;
        server_check(&registry, client_user).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_sign_up_and_join_two_devices() -> anyhow::Result<()> {
        //setup_tracing()?;

        let registry = FixtureRegistry::extended().await?;

        init_server(&registry).await?;
        let empty_state = &registry.state.base.empty;

        let server_creds_spec = PersistentCredentialsSpec {
            p_obj: empty_state.p_obj.server.clone(),
        };
        server_creds_spec.verify_device_creds().await?;

        info!("Executing 'sign up' claim");
        let vd_p_obj = empty_state.p_obj.vd.clone();
        let user_creds = &empty_state.user_creds;

        let vd_gw = registry
            .state
            .meta_client_service
            .sync_gateway
            .vd_gw
            .clone();
        vd_gw.sync().await?;
        // second sync to get new messages created on server
        vd_gw.sync().await?;

        SignUpClaimTestAction::sign_up(vd_p_obj.clone(), &user_creds.vd)
            .instrument(vd_span())
            .await?;

        vd_gw.sync().await?;
        vd_gw.sync().await?;

        info!("Verify SignUpClaim");
        let vd_user = empty_state.user_creds.vd.user();
        let vd_claim_spec = SignUpClaimSpec {
            p_obj: vd_p_obj.clone(),
            user: vd_user.clone(),
        };
        vd_claim_spec.verify().instrument(client_span()).await?;

        let vd_db = vd_p_obj.repo.get_db().await;
        assert_eq!(7, vd_db.len());

        registry
            .state
            .base
            .spec
            .vd
            .verify_user_is_a_member()
            .await?;

        vd_gw.sync().await?;
        server_check(&registry, vd_user).await?;

        let client_gw = registry
            .state
            .meta_client_service
            .sync_gateway
            .client_gw
            .clone();
        client_gw.sync().await?;
        client_gw.sync().await?;

        let client_p_obj = empty_state.p_obj.client.clone();
        SignUpClaimTestAction::sign_up(client_p_obj.clone(), &user_creds.client)
            .instrument(client_span())
            .await?;

        client_gw.sync().await?;
        client_gw.sync().await?;

        vd_gw.sync().await?;
        vd_gw.sync().await?;

        let orchestrator = MetaOrchestrator {
            p_obj: empty_state.p_obj.vd.clone(),
            user_creds: user_creds.vd.clone(),
        };
        orchestrator.orchestrate().await?;

        vd_gw.sync().await?;
        vd_gw.sync().await?;

        //accept join request by vd
        let vd_p_vault = PersistentVault {
            p_obj: empty_state.p_obj.vd.clone(),
        };
        let vault_status = vd_p_vault.find(empty_state.user_creds.vd.user()).await?;

        let VaultStatus::Member(member) = vault_status else {
            bail!("Virtual device is not a vault member");
        };

        let vd_vault_obj = vd_p_vault.get_vault(&member.user_data).await?;

        assert_eq!(2, vd_vault_obj.to_data().users.len());

        Ok(())
    }

    async fn server_check(
        registry: &FixtureRegistry<ExtendedState>,
        client_user: UserData,
    ) -> anyhow::Result<()> {
        let server_app = registry.state.server_app.server_app.clone();
        let server_ss_device_log_events = {
            let ss_desc = SsDeviceLogDescriptor::from(client_user.device.device_id.clone());

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

        assert_eq!(6, server_db.len());

        let server_claim_spec = SignUpClaimSpec {
            p_obj: server_app.p_obj.clone(),
            user: client_user.clone(),
        };

        server_claim_spec.verify().await?;
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

    async fn init_server(registry: &FixtureRegistry<ExtendedState>) -> anyhow::Result<()> {
        let server_app = registry.state.server_app.server_app.clone();
        server_app.init().await?;
        Ok(())
    }
}
