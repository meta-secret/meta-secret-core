#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::{BaseState, EmptyState, ExtendedState};
    use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::db::objects::global_index::fixture::ServerPersistentGlobalIndexFixture;
    use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
    use crate::node::db::repo::persistent_credentials::fixture::PersistentCredentialsFixture;
    use crate::node::server::server_app::fixture::{ServerAppFixture, ServerDataTransferFixture};

    pub struct FixtureRegistry<S> {
        pub state: S,
    }

    impl FixtureRegistry<BaseState> {
        pub fn empty() -> FixtureRegistry<EmptyState> {
            let p_obj = PersistentObjectFixture::generate();
            let device_creds = DeviceCredentialsFixture::generate();
            let user_creds = UserCredentialsFixture::from(&device_creds);

            FixtureRegistry {
                state: EmptyState {
                    p_obj,
                    device_creds,
                    user_creds,
                },
            }
        }

        pub async fn base() -> anyhow::Result<FixtureRegistry<BaseState>> {
            let empty = FixtureRegistry::empty();
            let p_creds = PersistentCredentialsFixture::init(&empty.state).await?;
            let server_p_gi = ServerPersistentGlobalIndexFixture::from(&empty);

            let base = BaseState {
                empty: empty.state,
                p_creds,
                server_p_gi,
                server_dt: ServerDataTransferFixture::generate(),
            };

            Ok(FixtureRegistry { state: base })
        }

        //SyncGatewayFixture
        pub async fn extended() -> anyhow::Result<FixtureRegistry<ExtendedState>> {
            let base = FixtureRegistry::base().await?;

            let server_app = ServerAppFixture::try_from(&base)?;
            let meta_client_service = MetaClientServiceFixture::from(&base);

            let state = ExtendedState {
                base: base.state,
                server_app,
                meta_client_service,
            };
            Ok(FixtureRegistry { state })
        }
    }

    pub mod states {
        use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
        use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
        use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
        use crate::node::db::objects::global_index::fixture::ServerPersistentGlobalIndexFixture;
        use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
        use crate::node::db::repo::persistent_credentials::fixture::PersistentCredentialsFixture;
        use crate::node::server::server_app::fixture::{
            ServerAppFixture, ServerDataTransferFixture,
        };

        pub enum Fixture {
            Empty(EmptyState),
            Base(BaseState),
            Extended(ExtendedState),
        }

        pub struct EmptyState {
            pub device_creds: DeviceCredentialsFixture,
            pub user_creds: UserCredentialsFixture,

            pub p_obj: PersistentObjectFixture,
        }

        pub struct BaseState {
            pub empty: EmptyState,

            pub p_creds: PersistentCredentialsFixture,
            pub server_p_gi: ServerPersistentGlobalIndexFixture,

            pub server_dt: ServerDataTransferFixture,
        }

        pub struct ExtendedState {
            pub base: BaseState,
            pub server_app: ServerAppFixture,
            pub meta_client_service: MetaClientServiceFixture,
        }
    }
}
