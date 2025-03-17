mod tests;

#[cfg(test)]
pub mod fixture {
    use std::sync::Arc;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::meta_tests::fixture_util::fixture::states::BaseState;
    use meta_secret_core::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
    use meta_server_node::server::server_sync_protocol::EmbeddedSyncProtocol;
    use meta_server_node::server::server_sync_protocol::fixture::SyncProtocolFixture;
    use crate::tests::meta_secret_test::fixture::ServerAppFixture;

    pub struct ExtendedFixtureRegistry;

    pub struct ExtendedFixtureState {
        pub base: BaseState,
        pub server_app: Arc<ServerAppFixture>,
        pub meta_client_service: MetaClientServiceFixture<EmbeddedSyncProtocol>,
        pub sync: SyncProtocolFixture,
    }

    impl ExtendedFixtureRegistry {
        pub async fn extended() -> anyhow::Result<FixtureRegistry<ExtendedFixtureState>> {
            let base = FixtureRegistry::base().await?;

            let server_app_fixture = Arc::new(ServerAppFixture::try_from(&base)?);
            let sync = SyncProtocolFixture::new(server_app_fixture.server_app.clone());
            let meta_client_service = MetaClientServiceFixture::from(&base.state, sync.sync_protocol.clone());

            let state = ExtendedFixtureState {
                base: base.state,
                server_app: server_app_fixture,
                meta_client_service,
                sync,
            };
            Ok(FixtureRegistry { state })
        }
    }
}
