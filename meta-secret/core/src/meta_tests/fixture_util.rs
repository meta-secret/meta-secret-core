#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::specs::BaseSpec;
    use crate::meta_tests::fixture_util::fixture::states::{BaseState, EmptyState, ExtendedState};
    use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
    use crate::node::app::sync::sync_protocol::fixture::SyncProtocolFixture;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::common::model::vault::vault_data::fixture::VaultDataFixture;
    use crate::node::db::actions::vault::vault_action::fixture::ServerVaultActionFixture;
    use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
    use crate::node::db::objects::persistent_vault::fixture::PersistentVaultFixture;
    use crate::node::db::objects::persistent_vault::spec::VaultSpec;
    use crate::node::db::repo::persistent_credentials::fixture::PersistentCredentialsFixture;
    use crate::node::server::server_app::fixture::ServerAppFixture;
    use crate::crypto::keys::fixture::KeyManagerFixture;
    use std::sync::Arc;

    pub struct FixtureRegistry<S> {
        pub state: S,
    }

    impl FixtureRegistry<BaseState> {
        pub fn empty() -> FixtureRegistry<EmptyState> {
            let key_manager = KeyManagerFixture::generate();
            let device_creds = DeviceCredentialsFixture::from_km(&key_manager);
            let user_creds = UserCredentialsFixture::from(&device_creds);
            let p_obj = PersistentObjectFixture::generate();
            let p_vault = PersistentVaultFixture::generate(&p_obj);
            let vault_data = VaultDataFixture::from(&user_creds);

            FixtureRegistry {
                state: EmptyState {
                    device_creds,
                    user_creds,
                    p_obj,
                    p_vault,
                    vault_data,
                    key_manager,
                },
            }
        }

        pub async fn base() -> anyhow::Result<FixtureRegistry<BaseState>> {
            let empty = FixtureRegistry::empty();
            let p_creds = PersistentCredentialsFixture::init(&empty.state).await?;

            let base_spec = BaseSpec {
                client: VaultSpec {
                    p_obj: empty.state.p_obj.client.clone(),
                    user: empty.state.user_creds.client.user(),
                },
                client_b: VaultSpec {
                    p_obj: empty.state.p_obj.client_b.clone(),
                    user: empty.state.user_creds.client_b.user(),
                },
                vd: VaultSpec {
                    p_obj: empty.state.p_obj.vd.clone(),
                    user: empty.state.user_creds.vd.user(),
                },
            };

            // Create fixtures that depend on other fixtures
            let server_vault_action = ServerVaultActionFixture::from(&empty.state);

            let base = BaseState {
                empty: empty.state,
                spec: base_spec,
                p_creds,
                server_vault_action
            };

            Ok(FixtureRegistry { state: base })
        }

        //SyncGatewayFixture
        pub async fn extended() -> anyhow::Result<FixtureRegistry<ExtendedState>> {
            let base = FixtureRegistry::base().await?;

            let server_app = Arc::new(ServerAppFixture::try_from(&base)?);
            let sync = SyncProtocolFixture::new(server_app.clone());
            let meta_client_service = MetaClientServiceFixture::from(&base.state, &sync);

            let state = ExtendedState {
                base: base.state,
                server_app,
                meta_client_service,
                sync,
            };
            Ok(FixtureRegistry { state })
        }
    }

    pub mod states {
        use std::sync::Arc;

        use crate::meta_tests::fixture_util::fixture::BaseSpec;
        use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
        use crate::node::app::sync::sync_protocol::fixture::SyncProtocolFixture;
        use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
        use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
        use crate::node::common::model::vault::vault_data::fixture::VaultDataFixture;
        use crate::node::db::actions::vault::vault_action::fixture::ServerVaultActionFixture;
        use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
        use crate::node::db::objects::persistent_vault::fixture::PersistentVaultFixture;
        use crate::node::db::repo::persistent_credentials::fixture::PersistentCredentialsFixture;
        use crate::node::server::server_app::fixture::ServerAppFixture;
        use crate::crypto::keys::fixture::KeyManagerFixture;

        pub enum Fixture {
            Empty(EmptyState),
            Base(BaseState),
            Extended(ExtendedState),
        }

        pub struct EmptyState {
            pub device_creds: DeviceCredentialsFixture,
            pub user_creds: UserCredentialsFixture,
            pub p_obj: PersistentObjectFixture,
            pub p_vault: PersistentVaultFixture,
            pub vault_data: VaultDataFixture,
            pub key_manager: KeyManagerFixture,
        }

        pub struct BaseState {
            pub empty: EmptyState,
            pub spec: BaseSpec,
            pub p_creds: PersistentCredentialsFixture,
            pub server_vault_action: ServerVaultActionFixture,
        }

        pub struct ExtendedState {
            pub base: BaseState,
            pub server_app: Arc<ServerAppFixture>,
            pub meta_client_service: MetaClientServiceFixture,
            pub sync: SyncProtocolFixture,
        }
    }

    pub mod specs {
        use crate::node::db::in_mem_db::InMemKvLogEventRepo;
        use crate::node::db::objects::persistent_vault::spec::VaultSpec;

        pub struct BaseSpec {
            pub client: VaultSpec<InMemKvLogEventRepo>,
            pub client_b: VaultSpec<InMemKvLogEventRepo>,
            pub vd: VaultSpec<InMemKvLogEventRepo>,
        }
    }
}
