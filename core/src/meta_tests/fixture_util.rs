#[cfg(test)]
pub mod fixture {
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::user_creds::fixture::UserCredentialsFixture;

    struct FixtureRegistry<S> {
        state: S, // Current state (generic)
    }

    impl<S> FixtureRegistry<S> {
        pub fn init() -> Self {
            let empty = FixtureRegistry { state: EmptyRegistry };
            let device_creds = FixtureRegistry::from(empty);
            FixtureRegistry::from(device_creds)
        }
    }

    struct EmptyRegistry;


    impl From<FixtureRegistry<EmptyRegistry>> for FixtureRegistry<DeviceCredentialsFixture> {
        fn from(_: FixtureRegistry<EmptyRegistry>) -> Self {
            FixtureRegistry { state: DeviceCredentialsFixture::generate() }
        }
    }

    impl From<FixtureRegistry<DeviceCredentialsFixture>> for FixtureRegistry<UserCredentialsFixture> {
        fn from(registry: FixtureRegistry<DeviceCredentialsFixture>) -> Self {
            let user_creds = UserCredentialsFixture::from(&registry.state);
            FixtureRegistry { state: user_creds }
        }
    }
}