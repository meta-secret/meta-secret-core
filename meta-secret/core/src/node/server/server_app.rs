use std::sync::Arc;

use crate::node::common::model::device::common::{DeviceId, DeviceName};
use crate::node::common::model::device::device_creds::DeviceCredentials;
use crate::node::common::model::secret::{SecretDistributionType, SsClaim, SsDistributionStatus};
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
    use std::process::exit;
    use std::sync::Arc;

    use crate::node::app::meta_app::messaging::{
        ClusterDistributionRequest, GenericAppStateRequest,
    };
    use crate::node::app::sync::sync_gateway::SyncGateway;
    use crate::node::app::sync::sync_protocol::EmbeddedSyncProtocol;

    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;

    use crate::meta_tests::setup_tracing;
    use crate::node::app::meta_app::meta_client_service::MetaClientService;
    use crate::node::common::model::crypto::aead::EncryptedMessage;
    use crate::node::common::model::device::common::DeviceId;
    use crate::node::common::model::device::device_link::DeviceLink;
    use crate::node::common::model::secret::{SecretDistributionType, SsDistributionId, SsDistributionStatus};
    use crate::node::common::model::{ApplicationState, VaultFullInfo};
    use crate::node::db::descriptors::shared_secret_descriptor::{
        SsDeviceLogDescriptor, SsWorkflowDescriptor,
    };
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::shared_secret_event::SsWorkflowObject;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
    use crate::node::server::server_app::SsClaimVerifierForTestRecovery;
    use anyhow::bail;
    use anyhow::Result;
    use tracing::{info, Instrument};

    struct ActorNode {
        user: UserData,
        p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        gw: Arc<SyncGateway<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
        p_vault: Arc<PersistentVault<InMemKvLogEventRepo>>,
        p_ss: PersistentSharedSecret<InMemKvLogEventRepo>,
        orchestrator: MetaOrchestrator<InMemKvLogEventRepo>,
        client_service: Arc<MetaClientService<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
    }

    impl ActorNode {
        pub fn device_id(&self) -> DeviceId {
            self.user.device.device_id.clone()
        }
    }

    struct ServerNode {
        p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        p_vault: Arc<PersistentVault<InMemKvLogEventRepo>>,
        p_ss: PersistentSharedSecret<InMemKvLogEventRepo>,
        server_creds_spec: PersistentCredentialsSpec<InMemKvLogEventRepo>,
    }

    struct ServerAppSignUpSpec {
        registry: FixtureRegistry<ExtendedState>,

        client: ActorNode,
        vd: ActorNode,
        server: ServerNode,

        vd_claim_spec: SignUpClaimSpec<InMemKvLogEventRepo>,
    }

    impl ServerAppSignUpSpec {
        async fn build() -> Result<Self> {
            let registry = FixtureRegistry::extended().await?;
            let empty_state = &registry.state.base.empty;
            let server_p_obj = empty_state.p_obj.server.clone();
            let server_creds_spec = PersistentCredentialsSpec::from(server_p_obj.clone());

            let client = ActorNode {
                user: empty_state.user_creds.client.user(),
                p_obj: empty_state.p_obj.client.clone(),
                gw: registry
                    .state
                    .meta_client_service
                    .sync_gateway
                    .client_gw
                    .clone(),
                p_vault: empty_state.p_vault.client.clone(),
                p_ss: PersistentSharedSecret::from(empty_state.p_obj.client.clone()),
                orchestrator: MetaOrchestrator {
                    p_obj: empty_state.p_obj.client.clone(),
                    user_creds: empty_state.user_creds.client.clone(),
                },
                client_service: registry.state.meta_client_service.client.clone(),
            };

            let vd = ActorNode {
                user: empty_state.user_creds.vd.user(),
                p_obj: empty_state.p_obj.vd.clone(),
                gw: registry
                    .state
                    .meta_client_service
                    .sync_gateway
                    .vd_gw
                    .clone(),
                p_vault: empty_state.p_vault.vd.clone(),
                p_ss: PersistentSharedSecret::from(empty_state.p_obj.vd.clone()),
                orchestrator: MetaOrchestrator {
                    p_obj: empty_state.p_obj.vd.clone(),
                    user_creds: empty_state.user_creds.vd.clone(),
                },
                client_service: registry.state.meta_client_service.vd.clone(),
            };

            let vd_claim_spec = SignUpClaimSpec {
                p_obj: empty_state.p_obj.vd.clone(),
                user: empty_state.user_creds.vd.user(),
            };

            let server = ServerNode {
                p_obj: empty_state.p_obj.server.clone(),
                p_vault: empty_state.p_vault.server.clone(),
                p_ss: PersistentSharedSecret::from(empty_state.p_obj.server.clone()),
                server_creds_spec,
            };

            Ok(Self {
                registry,
                server,
                client,
                vd,
                vd_claim_spec,
            })
        }

        fn empty_state(&self) -> &EmptyState {
            &self.registry.state.base.empty
        }

        fn user_creds(&self) -> &UserCredentialsFixture {
            &self.empty_state().user_creds
        }

        async fn sign_up_and_second_devices_joins(&self) -> Result<()> {
            //setup_tracing()?;
            let vd_gw = self.vd.gw.clone();
            let client_gw = self.client.gw.clone();

            self.init_server().await?;
            self.server.server_creds_spec.verify_device_creds().await?;

            info!("Executing 'sign up' claim");
            vd_gw.sync().await?;
            // second sync to get new messages created on server
            vd_gw.sync().await?;

            SignUpClaimTestAction::sign_up(self.vd.p_obj.clone(), &self.user_creds().vd)
                .instrument(vd_span())
                .await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            info!("Verify SignUpClaim");
            self.vd_claim_spec
                .verify()
                .instrument(client_span())
                .await?;

            let vd_db = self.vd.p_obj.repo.get_db().await;
            assert_eq!(7, vd_db.len());

            self.registry
                .state
                .base
                .spec
                .vd
                .verify_user_is_a_member()
                .await?;

            vd_gw.sync().await?;
            self.server_check(self.vd.user.clone()).await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            SignUpClaimTestAction::sign_up(self.client.p_obj.clone(), &self.user_creds().client)
                .instrument(client_span())
                .await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            self.vd.orchestrator.orchestrate().await?;

            vd_gw.sync().await?;
            vd_gw.sync().await?;

            client_gw.sync().await?;
            client_gw.sync().await?;

            //accept join request by vd
            let vault_status = self
                .vd
                .p_vault
                .find(self.empty_state().user_creds.vd.user())
                .await?;

            let VaultStatus::Member(member) = vault_status else {
                bail!("Virtual device is not a vault member");
            };

            let vd_vault_obj = self.vd.p_vault.get_vault(&member.user_data).await?;

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

    struct SplitSpec {
        spec: ServerAppSignUpSpec,
    }

    impl SplitSpec {
        async fn split(&self) -> Result<()> {
            let client_client_service = self.spec.client.client_service.clone();
            let app_state = client_client_service.build_service_state().await?.app_state;

            let pass_id = MetaPasswordId::build("test_pass");
            let dist_request = {
                let dist_request = ClusterDistributionRequest {
                    pass_id: pass_id.clone(),
                    pass: "2bee|~".to_string(),
                };
                GenericAppStateRequest::ClusterDistribution(dist_request)
            };

            let new_app_state = client_client_service
                .handle_client_request(app_state, dist_request)
                .await?;
            //println!("{:?}", new_app_state);

            let ApplicationState::Vault(VaultFullInfo::Member(member)) = &new_app_state else {
                bail!("Has to be Vault");
            };

            assert_eq!(1, member.member.vault.secrets.len());

            self.spec.vd.gw.sync().await?;
            self.spec.vd.gw.sync().await?;

            // let client_db: HashMap<ArtifactId, GenericKvLogEvent> =
            //     self.sign_up.vd.p_obj.repo.get_db().await;
            // for (id, event) in client_db {
            //     let event_json = serde_json::to_string_pretty(&event)?;
            //     println!("DbEvent:");
            //     println!(" id: {:?}", &id);
            //     println!(" event: {}", &event_json);
            // }

            let client_dist_id = SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: self.spec.client.device_id(),
            };
            let ss_dist_desc = SsWorkflowDescriptor::Distribution(client_dist_id.clone());

            let client_ss_dist = self
                .spec
                .client
                .p_obj
                .find_tail_event(ss_dist_desc)
                .await?
                .unwrap();
            let SsWorkflowObject::Distribution(client_dist_event) = client_ss_dist else {
                panic!("No split events found on the client");
            };

            let secret_text = client_dist_event
                .value
                .secret_message
                .cipher_text()
                .decrypt(
                    &self
                        .spec
                        .user_creds()
                        .client
                        .device_creds
                        .secret_box
                        .transport
                        .sk,
                )?;
            let _share_plain_text = String::try_from(&secret_text.msg)?;
            //println!("{}", share_plain_text);

            //verify distribution share is present on vd device
            let vd_receiver_device_id = self.spec.vd.device_id();
            let vd_dist_id = SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: vd_receiver_device_id.clone(),
            };
            let vd_ss_dist_desc = SsWorkflowDescriptor::Distribution(vd_dist_id.clone());

            let vd_ss_dist = self
                .spec
                .vd
                .p_obj
                .find_tail_event(vd_ss_dist_desc)
                .await?
                .unwrap();
            let SsWorkflowObject::Distribution(vd_dist_event) = vd_ss_dist else {
                panic!("No split events found on the vd device");
            };

            let EncryptedMessage::CipherShare { share } = vd_dist_event.value.secret_message;
            assert_eq!(
                vd_receiver_device_id,
                share.channel.receiver().to_device_id()
            );

            //let new_app_state_json = serde_json::to_string_pretty(&new_app_state)?;
            //println!("{}", new_app_state_json);

            Ok(())
        }
    }

    #[tokio::test]
    async fn test_sign_up_and_join_two_devices() -> Result<()> {
        let spec = ServerAppSignUpSpec::build().await?;
        spec.sign_up_and_second_devices_joins().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_secret_split() -> Result<()> {
        setup_tracing()?;

        let spec = ServerAppSignUpSpec::build().await?;
        let split = SplitSpec { spec };

        split.spec.sign_up_and_second_devices_joins().await?;
        split.split().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_recover() -> Result<()> {
        let split = {
            let spec = ServerAppSignUpSpec::build().await?;
            SplitSpec { spec }
        };

        split.spec.sign_up_and_second_devices_joins().await?;
        split.split().await?;

        let vd_client_service = split.spec.vd.client_service.clone();
        let vd_app_state = vd_client_service.build_service_state().await?.app_state;

        let ApplicationState::Vault(VaultFullInfo::Member(vd_user_state)) = &vd_app_state else {
            bail!("Has to be Vault");
        };

        let recover = {
            let pass_id = vd_user_state
                .member
                .vault
                .secrets
                .iter()
                .next()
                .unwrap()
                .clone();
            GenericAppStateRequest::Recover(pass_id)
        };

        let _app_state = split
            .spec
            .vd
            .client_service
            .handle_client_request(vd_app_state, recover)
            .await?;

        // verify ss device log on vd
        let p_ss = PersistentSharedSecret::from(split.spec.vd.p_obj.clone());
        let vd_device_id = split.spec.vd.device_id();
        let vd_device_log_tail_event = p_ss
            .find_ss_device_log_tail_event(vd_device_id.clone())
            .await?
            .unwrap();
        let vd_ss_claim = vd_device_log_tail_event.to_distribution_request();
        let claim_verifier = SsClaimVerifierForTestRecovery {
            sender: vd_device_id.clone(),
            receiver: split.spec.client.device_id(),
        };
        claim_verifier.verify(vd_ss_claim.clone())?;

        //verify server state
        let vault_name = split.spec.client.user.vault_name.clone();
        let ss_log = split.spec.server.p_ss.get_ss_log_obj(vault_name).await?;

        // Verify that the SS log event has been created on the server
        assert_eq!(1, ss_log.claims.len());
        let recover_claim_on_server = ss_log.claims.values().next().unwrap().clone();

        // Verify the claim properties
        assert_eq!(vd_ss_claim, recover_claim_on_server);

        split.spec.client.gw.sync().await?;
        split.spec.client.gw.sync().await?;

        split.spec.client.orchestrator.orchestrate().await?;

        //----------- Start Recovery record verification on client -----------
        // Verify that the client has created a SsWorkflowObject::Recovery object
        let client_p_ss = PersistentSharedSecret::from(split.spec.client.p_obj.clone());
        
        // Iterate through all recovery IDs from the claim
        let recovery_id = vd_ss_claim.recovery_db_ids()[0].clone();
        // Create a descriptor for the recovery workflow object
        let recovery_desc = SsWorkflowDescriptor::Recovery(recovery_id.clone());

        // Try to find the recovery workflow object in the client's repository
        let recovery_event = client_p_ss
            .p_obj
            .find_tail_event(recovery_desc)
            .await?
            .unwrap();

        // Verify it's the correct type
        match recovery_event {
            SsWorkflowObject::Recovery(event) => {
                // Verify the recovery event has the correct data
                assert_eq!(event.value.vault_name, split.spec.client.user.vault_name);
                assert_eq!(event.value.claim_id, recovery_id.claim_id);

                // The event was created by the MetaOrchestrator
                println!("Found SsWorkflowObject::Recovery created by MetaOrchestrator");
            }
            _ => panic!("Expected Recovery workflow object but found something else"),
        }
        //----------- End Recovery record verification on client -----------

        split.spec.client.gw.sync().await?;

        //----------- Start Recovery record verification on the server -----------
        // Verify that the server has received and processed the SsWorkflow event
        let server_p_ss = split.spec.server.p_ss;
        let vault_name = split.spec.client.user.vault_name.clone();
        
        // Get the updated SS log after the client has sent the recovery workflow
        let updated_ss_log = server_p_ss.get_ss_log_obj(vault_name.clone()).await?;
        
        // Verify that the claim status has been updated to reflect the recovery workflow
        let server_claim = updated_ss_log.claims.values().next().unwrap();
        
        // Check that the claim has been marked as sent to the client
        let recovery_id = vd_ss_claim.recovery_db_ids()[0].clone();
        
        // Check that the claim status has been updated for the client device
        let client_device_id = split.spec.client.device_id();
        let status = server_claim.status.get(&client_device_id);
        assert!(status.is_some(), "Server claim should have status for client device");
        assert!(matches!(status.unwrap(), SsDistributionStatus::Sent), 
                "Server claim status for client device should be Sent");
        
        // Verify that the SsWorkflow object exists in the server's repository
        let recovery_desc = SsWorkflowDescriptor::Recovery(recovery_id.clone());
        let server_recovery_event = server_p_ss
            .p_obj
            .find_tail_event(recovery_desc)
            .await?;
        
        assert!(server_recovery_event.is_some(), "Server has to have the SsWorkflow recovery event");
        
        if let Some(SsWorkflowObject::Recovery(event)) = server_recovery_event {
            // Verify the recovery event properties
            assert_eq!(event.value.vault_name, vault_name);
            assert_eq!(event.value.claim_id, recovery_id.claim_id);
            
            println!("Server has correctly processed the SsWorkflow recovery event");
        } else {
            panic!("Expected SsWorkflow::Recovery event on server but found something else");
        }
        //----------- End Recovery record verification on the server -----------
        
        split.spec.client.gw.sync().await?;
        
        

        // Verify that client has received the recovery claim
        let client_p_ss = PersistentSharedSecret::from(split.spec.client.p_obj.clone());
        let client_vault_name = split.spec.client.user.vault_name.clone();

        // Get the shared secret log for the client
        let client_ss_log = client_p_ss.get_ss_log_obj(client_vault_name).await?;

        // Get the recovery claim from the client's shared secret log
        let client_recovery_claim = client_ss_log.claims.values().next().unwrap().clone();

        assert_eq!(vd_ss_claim, client_recovery_claim);

        //Update app state
        let _app_state_after_full_recover = split.spec.vd.client_service.get_app_state().await?;

        split.spec.vd.orchestrator.orchestrate().await?;

        split.spec.vd.gw.sync().await?;
        split.spec.vd.gw.sync().await?;

        let vd_db = split.spec.vd.p_obj.repo.get_db().await;
        for (id, event) in vd_db {
            if matches!(
                event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
            ) {
                let event_json = serde_json::to_string_pretty(&event)?;
                println!("DbEvent:");
                println!(" id: {:?}", &id);
                println!(" event: {}", &event_json);
            } else {
                println!("skip event")
            }
        }

        //let app_state_after_recover_json = serde_json::to_string_pretty(&app_state_after_recover)?;
        //println!("{}", app_state_after_recover_json);

        Ok(())
    }
}

struct SsClaimVerifierForTestRecovery {
    sender: DeviceId,
    receiver: DeviceId,
}

impl SsClaimVerifierForTestRecovery {
    pub fn verify(&self, claim: SsClaim) -> Result<()> {
        // Verify the claim properties
        assert_eq!(claim.sender, self.sender);
        assert_eq!(claim.distribution_type, SecretDistributionType::Recover);
        assert_eq!(claim.receivers.len(), 1);
        assert_eq!(claim.receivers[0], self.receiver);

        // Verify that the recovery database IDs are present
        let claim_ids = claim.recovery_db_ids();
        assert_eq!(1, claim_ids.len());

        Ok(())
    }
}
