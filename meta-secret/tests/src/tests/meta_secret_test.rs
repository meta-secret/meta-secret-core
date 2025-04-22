use anyhow::Result;
use meta_secret_core::node::common::model::device::common::DeviceId;
use meta_secret_core::node::common::model::secret::{SecretDistributionType, SsClaim};

#[cfg(test)]
pub mod fixture {
    use meta_secret_core::meta_tests::fixture_util::fixture::states::BaseState;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
    use meta_server_node::server::server_app::ServerApp;
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
    use crate::fixture::{ExtendedFixtureRegistry, ExtendedFixtureState};
    use crate::tests::meta_secret_test::SsClaimVerifierForTestRecovery;
    use anyhow::bail;
    use anyhow::Result;
    use meta_secret_core::meta_tests::fixture_util::fixture::states::EmptyState;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::meta_tests::spec::test_spec::TestSpec;
    use meta_secret_core::node::app::meta_app::messaging::{
        ClusterDistributionRequest, GenericAppStateRequest,
    };
    use meta_secret_core::node::common::meta_tracing::{client_span, server_span, vd_span};
    use meta_secret_core::node::common::model::crypto::aead::EncryptedMessage;
    use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;
    use meta_secret_core::node::common::model::secret::{SsDistributionId, SsDistributionStatus};
    use meta_secret_core::node::common::model::user::common::UserData;
    use meta_secret_core::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use meta_secret_core::node::common::model::vault::vault::VaultStatus;
    use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
    use meta_secret_core::node::db::actions::recover::RecoveryHandler;
    use meta_secret_core::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use meta_secret_core::node::db::actions::sign_up::claim::test_action::SignUpClaimTestAction;
    use meta_secret_core::node::db::descriptors::shared_secret_descriptor::{
        SsDeviceLogDescriptor, SsWorkflowDescriptor,
    };
    use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
    use meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject;
    use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use tracing::{info, Instrument};
    use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;

    struct ServerAppSignUpSpec {
        registry: FixtureRegistry<ExtendedFixtureState>,
    }

    impl ServerAppSignUpSpec {
        async fn build() -> Result<Self> {
            let registry = ExtendedFixtureRegistry::extended().await?;
            Ok(Self { registry })
        }

        fn empty_state(&self) -> &EmptyState {
            &self.registry.state.base.empty
        }

        fn user_creds(&self) -> &UserCredentialsFixture {
            &self.empty_state().user_creds
        }

        async fn sign_up_and_second_devices_joins(&self) -> Result<()> {
            //setup_tracing()?;
            
            self.init_server().await?;
            self.verify_server_device_creds().await?;

            info!("Executing 'sign up' claim");
            self.vd_gw_sync().await?;

            SignUpClaimTestAction::sign_up(
                self.registry.state.vd.p_obj.clone(),
                &self.user_creds().vd,
            )
            .instrument(vd_span())
            .await?;

            self.vd_gw_sync().await?;

            info!("Verify SignUpClaim");
            self.registry.state.vd_claim_spec
                .verify()
                .instrument(client_span())
                .await?;

            let vd_db = self.registry.state.vd.p_obj.repo.get_db().await;
            assert_eq!(7, vd_db.len());

            self.registry
                .state
                .base
                .spec
                .vd
                .verify_user_is_a_member()
                .await?;

            self.vd_gw_sync().await?;
            self.server_check(self.registry.state.vd.user.clone())
                .await?;

            self.client_gw_sync().await?;

            SignUpClaimTestAction::sign_up(
                self.registry.state.client.p_obj.clone(),
                &self.user_creds().client,
            )
            .instrument(client_span())
            .await?;

            self.client_gw_sync().await?;
            self.vd_gw_sync().await?;
            
            let vd_client = self.registry.state.vd.client_service.clone();
            let vd_app_state = vd_client.get_app_state().await?;
            let ApplicationState::Vault(VaultFullInfo::Member(vd_member_info)) = vd_app_state else {
                bail!("Vd is not a vault member");
            };

            assert_eq!(1, vd_member_info.vault_events.requests.len());
            let vault_action_event = vd_member_info.vault_events.requests.iter().next().unwrap();
            let VaultActionRequestEvent::JoinCluster(join_request) = vault_action_event else {
                 bail!("Join request is not found");
            };
            
            self.registry.state.vd.orchestrator.accept_join(join_request.clone()).await?;
            self.vd_gw_sync().await?;
            self.client_gw_sync().await?;

            //accept join request by vd
            let vault_status = self
                .registry
                .state
                .vd
                .p_vault
                .find(self.empty_state().user_creds.vd.user())
                .await?;

            let VaultStatus::Member(member) = vault_status else {
                bail!("Virtual device is not a vault member");
            };

            let vd_vault_obj = self
                .registry
                .state
                .vd
                .p_vault
                .get_vault(&member.user_data)
                .await?;

            assert_eq!(2, vd_vault_obj.to_data().users.len());

            Ok(())
        }

        async fn client_gw_sync(&self) -> Result<()> {
            let user = self.registry.state.client.user.clone();
            self.registry.state.client.gw.sync(user.clone()).await?;
            self.registry.state.client.gw.sync(user).await?;
            Ok(())
        }

        async fn vd_gw_sync(&self) -> Result<()> {
            let user = self.registry.state.vd.user.clone();
            self.registry.state.vd.gw.sync(user.clone()).await?;
            // second sync to get new messages created on server
            self.registry.state.vd.gw.sync(user).await?;
            Ok(())
        }

        async fn verify_server_device_creds(&self) -> Result<()> {
            self.registry
                .state
                .server_node
                .creds_spec
                .verify_device_creds()
                .await?;
            Ok(())
        }

        #[allow(unused_variables)]
        async fn server_check(&self, client_user: UserData) -> Result<()> {
            let server_app = self.registry.state.server_app.server_app.clone();
            let server_p_obj = self.registry.state.base.empty.p_obj.server.clone();

            let server_ss_device_log_events = {
                let ss_desc = SsDeviceLogDescriptor::from(client_user.device.device_id.clone());

                server_p_obj
                    .get_object_events_from_beginning(ss_desc)
                    .instrument(server_span())
                    .await?
            };
            info!(
                "SERVER SS device log EVENTS: {:?}",
                server_ss_device_log_events.len()
            );

            let server_db = server_p_obj.repo.get_db().await;

            assert_eq!(6, server_db.len());

            let server_claim_spec = SignUpClaimSpec {
                p_obj: server_p_obj.clone(),
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
            let client_client_service = self.spec.registry.state.client.client_service.clone();
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

            self.vd_gw_sync().await?;

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
                receiver: self.spec.registry.state.client.device_id(),
            };
            let ss_dist_desc = SsWorkflowDescriptor::Distribution(client_dist_id.clone());

            let client_ss_dist = self
                .spec
                .registry
                .state
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
            let vd_receiver_device_id = self.spec.registry.state.vd.device_id();
            let vd_dist_id = SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: vd_receiver_device_id.clone(),
            };
            let vd_ss_dist_desc = SsWorkflowDescriptor::Distribution(vd_dist_id.clone());

            let vd_ss_dist = self
                .spec
                .registry
                .state
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

        async fn vd_gw_sync(&self) -> Result<()> {
            let user = self.spec.registry.state.vd.user.clone();
            self.spec.registry.state.vd.gw.sync(user.clone()).await?;
            // second sync to get new messages created on server
            self.spec.registry.state.vd.gw.sync(user).await?;
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
        let spec = ServerAppSignUpSpec::build().await?;
        let split = SplitSpec { spec };

        split.spec.sign_up_and_second_devices_joins().await?;
        split.split().await?;

        Ok(())
    }

    #[tokio::test]
    #[allow(dead_code, unused_variables)]
    async fn test_recover() -> Result<()> {
        let split = {
            let spec = ServerAppSignUpSpec::build().await?;
            SplitSpec { spec }
        };

        split.spec.sign_up_and_second_devices_joins().await?;
        split.split().await?;

        let vd_client_service = split.spec.registry.state.vd.client_service.clone();
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
            .registry
            .state
            .vd
            .client_service
            .handle_client_request(vd_app_state, recover)
            .await?;

        // verify ss device log on vd
        let p_ss = PersistentSharedSecret::from(split.spec.registry.state.vd.p_obj.clone());
        let vd_device_id = split.spec.registry.state.vd.device_id();
        let vd_device_log_tail_event = p_ss
            .find_ss_device_log_tail_event(vd_device_id.clone())
            .await?
            .unwrap();
        let vd_ss_claim = vd_device_log_tail_event.to_distribution_request();
        let claim_verifier = SsClaimVerifierForTestRecovery {
            sender: vd_device_id.clone(),
            receiver: split.spec.registry.state.client.device_id(),
        };
        claim_verifier.verify(vd_ss_claim.clone())?;

        //verify server state
        let vault_name = split.spec.registry.state.client.user.vault_name.clone();
        let ss_log = split
            .spec
            .registry
            .state
            .server_node
            .p_ss
            .get_ss_log_obj(vault_name)
            .await?;

        // Verify that the SS log event has been created on the server
        assert_eq!(1, ss_log.claims.len());
        let recover_claim_on_server = ss_log.claims.values().next().unwrap().clone();

        // Verify the claim properties
        assert_eq!(vd_ss_claim, recover_claim_on_server);

        split.spec.client_gw_sync().await?;
        
        split
            .spec
            .registry
            .state
            .client
            .orchestrator
            .orchestrate()
            .await?;

        //----------- Start Recovery record verification on client -----------
        // Verify that the client has created a SsWorkflowObject::Recovery object
        let client_p_ss = split.spec.registry.state.client.p_ss.clone();

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
                assert_eq!(
                    event.value.vault_name,
                    split.spec.registry.state.client.user.vault_name
                );
                assert_eq!(event.value.claim_id, recovery_id.claim_id);

                // The event was created by the MetaOrchestrator
                println!("Found SsWorkflowObject::Recovery created by MetaOrchestrator");
            }
            _ => panic!("Expected Recovery workflow object but found something else"),
        }
        //----------- End Recovery record verification on client -----------

        split.spec.client_gw_sync().await?;

        //----------- Start Recovery record verification on the server -----------
        // Verify that the server has received and processed the SsWorkflow event
        let server_p_ss = split.spec.registry.state.server_node.p_ss.clone();
        let vault_name = split.spec.registry.state.client.user.vault_name.clone();

        // Get the updated SS log after the client has sent the recovery workflow
        let updated_ss_log = server_p_ss.get_ss_log_obj(vault_name.clone()).await?;

        // Verify that the claim status has been updated to reflect the recovery workflow
        let server_claim = updated_ss_log.claims.values().next().unwrap();

        // Check that the claim has been marked as sent to the client
        let recovery_id = vd_ss_claim.recovery_db_ids()[0].clone();

        // Check that the claim status has been updated for the client device
        let client_device_id = split.spec.registry.state.client.device_id();
        let status = server_claim.status.get(&client_device_id);
        assert!(
            status.is_some(),
            "Server claim should have status for client device"
        );
        assert!(
            matches!(status.unwrap(), SsDistributionStatus::Sent),
            "Server claim status for client device should be Sent"
        );

        // Verify that the SsWorkflow object exists in the server's repository
        let recovery_desc = SsWorkflowDescriptor::Recovery(recovery_id.clone());
        let server_recovery_event = server_p_ss.p_obj.find_tail_event(recovery_desc).await?;

        assert!(
            server_recovery_event.is_some(),
            "Server has to have the SsWorkflow recovery event"
        );

        if let Some(SsWorkflowObject::Recovery(event)) = server_recovery_event {
            // Verify the recovery event properties
            assert_eq!(event.value.vault_name, vault_name);
            assert_eq!(event.value.claim_id, recovery_id.claim_id);

            println!("Server has correctly processed the SsWorkflow recovery event");
        } else {
            panic!("Expected SsWorkflow::Recovery event on server but found something else");
        }
        //----------- End Recovery record verification on the server -----------

        split.spec.client_gw_sync().await?;

        // Verify that client has received the recovery claim
        let client_p_ss =
            PersistentSharedSecret::from(split.spec.registry.state.client.p_obj.clone());
        let client_vault_name = split.spec.registry.state.client.user.vault_name.clone();

        // Get the shared secret log for the client
        let client_ss_log = client_p_ss.get_ss_log_obj(client_vault_name).await?;

        // Get the recovery claim from the client's shared secret log
        let client_recovery_claim = client_ss_log.claims.values().next().unwrap().clone();

        claim_verifier.verify(client_recovery_claim.clone())?;

        //Update app state
        let _app_state_after_full_recover = split
            .spec
            .registry
            .state
            .vd
            .client_service
            .get_app_state()
            .await?;

        //TODO we have to check if orchestrate breaks distribution (recovery)
        //split.spec.vd.orchestrator.orchestrate().await?;

        split.spec.vd_gw_sync().await?;
        
        let vd_db = split.spec.registry.state.vd.p_obj.repo.get_db().await;
        let vd_db = vd_db.into_values().collect::<Vec<GenericKvLogEvent>>();

        let recovery_event = vd_db.iter().find(|event| {
            matches!(
                event,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(_))
            )
        });
        let recovery_event = recovery_event.unwrap();

        //let event_json = serde_json::to_string_pretty(&recovery_event)?;
        //println!("DbEvent:");
        //println!(" event: {}", &event_json);

        let recovery_handler = RecoveryHandler {
            p_obj: split.spec.registry.state.vd.p_obj.clone(),
        };

        let pass = recovery_handler
            .recover(vault_name, split.spec.user_creds().vd.clone(), recovery_id)
            .await?;

        assert_eq!("2bee|~", pass.text);

        //let app_state_after_recover_json = serde_json::to_string_pretty(&app_state_after_recover)?;
        //println!("{}", app_state_after_recover_json);

        Ok(())
    }
}

#[allow(dead_code)]
struct SsClaimVerifierForTestRecovery {
    sender: DeviceId,
    receiver: DeviceId,
}

impl SsClaimVerifierForTestRecovery {
    #[allow(dead_code)]
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
