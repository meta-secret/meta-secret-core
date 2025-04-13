mod tests;

#[cfg(test)]
pub mod fixture {
    use crate::tests::meta_secret_test::fixture::ServerAppFixture;
    use meta_secret_core::meta_tests::fixture_util::fixture::states::BaseState;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
    use meta_secret_core::node::app::meta_app::meta_client_service::MetaClientService;
    use meta_secret_core::node::app::orchestrator::MetaOrchestrator;
    use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
    use meta_secret_core::node::common::model::device::common::DeviceId;
    use meta_secret_core::node::common::model::user::common::UserData;
    use meta_secret_core::node::db::actions::sign_up::claim::spec::SignUpClaimSpec;
    use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
    use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
    use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
    use meta_secret_core::node::db::repo::persistent_credentials::spec::PersistentCredentialsSpec;
    use meta_server_node::server::server_sync_protocol::fixture::{EmbeddedSyncProtocol, SyncProtocolFixture};
    use std::sync::Arc;

    pub struct ExtendedFixtureRegistry;

    pub struct ExtendedFixtureState {
        pub base: BaseState,
        pub server_app: Arc<ServerAppFixture>,
        pub meta_client_service: MetaClientServiceFixture<EmbeddedSyncProtocol>,
        pub sync: SyncProtocolFixture,
        
        pub server_node: ServerNode,
        pub client: ActorNode,
        pub vd: ActorNode,
        pub vd_claim_spec: SignUpClaimSpec<InMemKvLogEventRepo>,
    }

    impl ExtendedFixtureRegistry {
        pub async fn extended() -> anyhow::Result<FixtureRegistry<ExtendedFixtureState>> {
            let base = FixtureRegistry::base().await?;

            let server_app_fixture = Arc::new(ServerAppFixture::try_from(&base)?);
            let sync = SyncProtocolFixture::new(server_app_fixture.server_app.clone());
            let meta_client_service = MetaClientServiceFixture::from(&base.state, sync.sync_protocol.clone());

            let empty_state = &base.state.empty;
            
            let server_node = {
                let server_p_obj = empty_state.p_obj.server.clone();
                ServerNode {
                    p_obj: server_p_obj.clone(),
                    p_vault: empty_state.p_vault.server.clone(),
                    p_ss: Arc::new(PersistentSharedSecret::from(server_p_obj.clone())),
                    creds_spec: PersistentCredentialsSpec::from(server_p_obj.clone())
                }
            };

            let client = ActorNode {
                user: empty_state.user_creds.client.user(),
                p_obj: empty_state.p_obj.client.clone(),
                gw: meta_client_service
                    .sync_gateway
                    .client_gw
                    .clone(),
                p_vault: empty_state.p_vault.client.clone(),
                p_ss: Arc::new(PersistentSharedSecret::from(
                    empty_state.p_obj.client.clone(),
                )),
                orchestrator: MetaOrchestrator {
                    p_obj: empty_state.p_obj.client.clone(),
                    user_creds: empty_state.user_creds.client.clone(),
                },
                client_service: meta_client_service.client.clone(),
            };

            let vd = ActorNode {
                user: empty_state.user_creds.vd.user(),
                p_obj: empty_state.p_obj.vd.clone(),
                gw: meta_client_service
                    .sync_gateway
                    .vd_gw
                    .clone(),
                p_vault: empty_state.p_vault.vd.clone(),
                p_ss: Arc::new(PersistentSharedSecret::from(empty_state.p_obj.vd.clone())),
                orchestrator: MetaOrchestrator {
                    p_obj: empty_state.p_obj.vd.clone(),
                    user_creds: empty_state.user_creds.vd.clone(),
                },
                client_service: meta_client_service.vd.clone(),
            };

            let vd_claim_spec = SignUpClaimSpec {
                p_obj: empty_state.p_obj.vd.clone(),
                user: empty_state.user_creds.vd.user(),
            };

            let state = ExtendedFixtureState {
                base: base.state,
                server_app: server_app_fixture,
                meta_client_service,
                sync,
                server_node,
                client,
                vd,
                vd_claim_spec
            };
            Ok(FixtureRegistry { state })
        }
    }

    #[allow(dead_code)]
    pub struct ServerNode {
        pub p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
        pub p_vault: Arc<PersistentVault<InMemKvLogEventRepo>>,
        pub p_ss: Arc<PersistentSharedSecret<InMemKvLogEventRepo>>,
        pub creds_spec: PersistentCredentialsSpec<InMemKvLogEventRepo>,
    }

    pub struct ActorNode {
        pub user: UserData,
        pub p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,

        pub gw: Arc<SyncGateway<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
        pub p_vault: Arc<PersistentVault<InMemKvLogEventRepo>>,
        pub p_ss: Arc<PersistentSharedSecret<InMemKvLogEventRepo>>,
        pub orchestrator: MetaOrchestrator<InMemKvLogEventRepo>,
        pub client_service: Arc<MetaClientService<InMemKvLogEventRepo, EmbeddedSyncProtocol>>,
    }

    impl ActorNode {
        pub fn device_id(&self) -> DeviceId {
            self.user.device.device_id.clone()
        }
    }
}
