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
    use crate::meta_tests::fixture_util::fixture::states::{EmptyState, ExtendedState};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::spec::test_spec::TestSpec;
    use crate::node::app::orchestrator::MetaOrchestrator;
    use crate::node::common::meta_tracing::{client_span, server_span, vd_span};
    use crate::node::common::model::user::common::UserData;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use crate::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::node::app::meta_app::messaging::{
        ClusterDistributionRequest, GenericAppStateRequest,
    };
    use crate::node::app::sync::sync_gateway::SyncGateway;
    use crate::node::app::sync::sync_protocol::EmbeddedSyncProtocol;

    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;

    use crate::crypto::encoding::base64::Base64Text;
    use crate::node::common::model::secret::SsDistributionId;
    use crate::node::common::model::{ApplicationState, VaultFullInfo};
    use crate::node::db::descriptors::shared_secret_descriptor::{
        SsDescriptor, SsDeviceLogDescriptor,
    };
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::object_id::ArtifactId;
    use crate::node::db::events::shared_secret_event::SsObject;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
    use anyhow::bail;
    use anyhow::Result;
    use tracing::{info, Instrument};

    struct ServerAppSignUpSpec {
        registry: FixtureRegistry<ExtendedState>,
        server_creds_spec: PersistentCredentialsSpec<InMemKvLogEventRepo>,
        vd_p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        vd_gw: Arc<SyncGateway<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
        vd_user: UserData,
        vd_claim_spec: SignUpClaimSpec<InMemKvLogEventRepo>,
        client_gw: Arc<SyncGateway<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
        client_p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        vd_orchestrator: MetaOrchestrator<InMemKvLogEventRepo>,
        vd_p_vault: PersistentVault<InMemKvLogEventRepo>,
    }

    impl ServerAppSignUpSpec {
        async fn build() -> Result<Self> {
            let registry = FixtureRegistry::extended().await?;
            let empty_state = &registry.state.base.empty;
            let server_p_obj = empty_state.p_obj.server.clone();
            let server_creds_spec = PersistentCredentialsSpec::from(server_p_obj.clone());
            let vd_p_obj = empty_state.p_obj.vd.clone();
            let user_creds = &empty_state.user_creds;
            let vd_gw = registry
                .state
                .meta_client_service
                .sync_gateway
                .vd_gw
                .clone();
            let vd_user = empty_state.user_creds.vd.user();
            let vd_claim_spec = SignUpClaimSpec {
                p_obj: vd_p_obj.clone(),
                user: vd_user.clone(),
            };

            let client_gw = registry
                .state
                .meta_client_service
                .sync_gateway
                .client_gw
                .clone();
            let client_p_obj = empty_state.p_obj.client.clone();
            let vd_orchestrator = MetaOrchestrator {
                p_obj: empty_state.p_obj.vd.clone(),
                user_creds: user_creds.vd.clone(),
            };
            let vd_p_vault = PersistentVault {
                p_obj: empty_state.p_obj.vd.clone(),
            };

            Ok(Self {
                registry,
                server_creds_spec,
                vd_p_obj,
                vd_gw,
                vd_user,
                vd_claim_spec,
                client_gw,
                client_p_obj,
                vd_orchestrator,
                vd_p_vault,
            })
        }

        fn empty_state(&self) -> &EmptyState {
            &self.registry.state.base.empty
        }

        fn user_creds(&self) -> &UserCredentialsFixture {
            &self.empty_state().user_creds
        }

        async fn sign_up_and_join_for_two_devices(&self) -> Result<()> {
            //setup_tracing()?;
            let vd_gw = self.vd_gw.clone();
            let client_gw = self.client_gw.clone();

            self.init_server().await?;
            self.server_creds_spec.verify_device_creds().await?;

            info!("Executing 'sign up' claim");
            vd_gw.sync().await?;
            // second sync to get new messages created on server
            vd_gw.sync().await?;

            SignUpClaimTestAction::sign_up(self.vd_p_obj.clone(), &self.user_creds().vd)
                .instrument(vd_span())
                .await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            info!("Verify SignUpClaim");
            self.vd_claim_spec
                .verify()
                .instrument(client_span())
                .await?;

            let vd_db = self.vd_p_obj.repo.get_db().await;
            assert_eq!(7, vd_db.len());

            self.registry
                .state
                .base
                .spec
                .vd
                .verify_user_is_a_member()
                .await?;

            vd_gw.sync().await?;
            self.server_check(self.vd_user.clone()).await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            SignUpClaimTestAction::sign_up(self.client_p_obj.clone(), &self.user_creds().client)
                .instrument(client_span())
                .await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            self.vd_orchestrator.orchestrate().await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            //accept join request by vd
            let vault_status = self
                .vd_p_vault
                .find(self.empty_state().user_creds.vd.user())
                .await?;

            let VaultStatus::Member(member) = vault_status else {
                bail!("Virtual device is not a vault member");
            };

            let vd_vault_obj = self.vd_p_vault.get_vault(&member.user_data).await?;

            assert_eq!(2, vd_vault_obj.to_data().users.len());

            Ok(())
        }

        async fn server_check(&self, client_user: UserData) -> Result<()> {
            let server_app = self.registry.state.server_app.server_app.clone();
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

        async fn init_server(&self) -> Result<()> {
            let server_app = self.registry.state.server_app.server_app.clone();
            server_app.init().await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_sign_up_and_join_two_devices() -> Result<()> {
        let spec = ServerAppSignUpSpec::build().await?;
        spec.sign_up_and_join_for_two_devices().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_secret_split() -> Result<()> {
        let spec = ServerAppSignUpSpec::build().await?;
        spec.sign_up_and_join_for_two_devices().await?;

        let client_service = spec.registry.state.meta_client_service.client.clone();
        let app_state = client_service.build_service_state().await?.app_state;

        let request = {
            let dist_request = ClusterDistributionRequest {
                pass_id: MetaPasswordId::build("test_pass"),
                pass: "2bee|~".to_string(),
            };
            GenericAppStateRequest::ClusterDistribution(dist_request)
        };

        let new_app_state = client_service
            .handle_client_request(spec.client_p_obj.clone(), app_state, request)
            .await?;
        //println!("{:?}", new_app_state);

        let ApplicationState::Vault(VaultFullInfo::Member(member)) = &new_app_state else {
            bail!("Has to be Vault");
        };

        assert_eq!(1, member.member.vault.secrets.len());

        //let client_db: HashMap<ArtifactId, GenericKvLogEvent> = spec.client_p_obj.repo.get_db().await;
        // for (id, event) in client_db {
        //     let event_json = serde_json::to_string_pretty(&event)?;
        //     println!("DbEvent:");
        //     println!(" id: {:?}", &id);
        //     println!(" event: {}", &event_json);
        // }

        let ss_dist_desc = SsDescriptor::SsDistribution(SsDistributionId {
            pass_id: member.member.vault.secrets.iter().next().unwrap().clone(),
            receiver: spec
                .user_creds()
                .client
                .device_creds
                .device
                .device_id
                .clone(),
        });

        let client_ss_dist = spec
            .client_p_obj
            .find_tail_event(ss_dist_desc)
            .await?
            .unwrap();
        let SsObject::SsDistribution(client_dist_event) = client_ss_dist else {
            panic!("No split events found on the client");
        };

        let secret_text = client_dist_event
            .value
            .secret_message
            .cipher_text()
            .decrypt(
                &spec
                    .user_creds()
                    .client
                    .device_creds
                    .secret_box
                    .transport
                    .sk,
            )?;
        let share_plain_text = String::try_from(&secret_text.msg)?;
        println!("{}", share_plain_text);

        //let new_app_state_json = serde_json::to_string_pretty(&new_app_state)?;
        //println!("{}", new_app_state_json);

        Ok(())
    }

    #[tokio::test]
    async fn test_secret_split_and_recover() -> Result<()> {
        let spec = ServerAppSignUpSpec::build().await?;
        spec.sign_up_and_join_for_two_devices().await?;

        let client_service = spec.registry.state.meta_client_service.client.clone();
        let app_state = client_service.build_service_state().await?.app_state;

        let split = {
            let dist_request = ClusterDistributionRequest {
                pass_id: MetaPasswordId::build("test_pass"),
                pass: "2bee|~".to_string(),
            };
            GenericAppStateRequest::ClusterDistribution(dist_request)
        };

        let new_app_state = client_service
            .handle_client_request(spec.client_p_obj.clone(), app_state, split)
            .await?;
        //println!("{:?}", new_app_state);

        let ApplicationState::Vault(VaultFullInfo::Member(member)) = &new_app_state else {
            bail!("Has to be Vault");
        };

        assert_eq!(1, member.member.vault.secrets.len());

        // Recover secret
        let recover = {
            let pass_id = member.member.vault.secrets.iter().next().unwrap().clone();
            GenericAppStateRequest::Recover(pass_id)
        };

        let app_state_after_recover = client_service
            .handle_client_request(spec.client_p_obj.clone(), new_app_state, recover)
            .await?;

        let app_state_after_recover_json = serde_json::to_string_pretty(&app_state_after_recover)?;
        println!("{}", app_state_after_recover_json);

        Ok(())
    }
}
