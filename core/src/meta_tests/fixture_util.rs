#[cfg(test)]
pub mod fixture {
    use crate::meta_tests::fixture_util::fixture::states::{BaseState, ExtendedState};
    use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
    use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
    use crate::node::server::server_app::fixture::{ServerAppFixture, ServerDataTransferFixture};

    pub struct FixtureRegistry<S> {
        pub state: S,
    }
    
    impl FixtureRegistry<BaseState> {
        pub fn base() -> FixtureRegistry<BaseState> {
            let device_creds = DeviceCredentialsFixture::generate();
            let user_creds = UserCredentialsFixture::from(&device_creds);
            let base = BaseState {
                device_creds,
                user_creds,
                server_dt: ServerDataTransferFixture::generate(),
                p_obj: PersistentObjectFixture::generate(),
            };

            FixtureRegistry { state: base }
        }
        
        //SyncGatewayFixture
        pub fn extended() -> anyhow::Result<FixtureRegistry<ExtendedState>> {
            let base = FixtureRegistry::base();
            
            let server_app = ServerAppFixture::try_from(&base)?;
            let meta_client_service = MetaClientServiceFixture::from(&base);
            
            let state = ExtendedState { base: base.state, server_app, meta_client_service };
            Ok(FixtureRegistry { state })
        }
    }

    pub mod states {
        use crate::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
        use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
        use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;
        use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
        use crate::node::server::server_app::fixture::{ServerAppFixture, ServerDataTransferFixture};

        #[derive(Eq, Hash, PartialEq)]
        pub enum StateDescriptor {
            Empty,
            Base,
            Extended,
        }
        
        pub enum Fixture {
            Empty(EmptyState),
            Base(BaseState),
            Extended(ExtendedState),
        }
        
        pub struct EmptyState;

        pub struct BaseState {
            pub device_creds: DeviceCredentialsFixture,
            pub user_creds: UserCredentialsFixture,
            pub server_dt: ServerDataTransferFixture,
            pub p_obj: PersistentObjectFixture
        }
        
        pub struct ExtendedState {
            pub base: BaseState,
            pub server_app: ServerAppFixture,
            pub meta_client_service: MetaClientServiceFixture
        }
    }
}
