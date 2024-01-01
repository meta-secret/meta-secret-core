use crate::node::common::model::{
    device::{DeviceCredentials, DeviceName},
    user::UserCredentials,
    vault::VaultName,
};

pub struct ClientDeviceFixture {
    pub device_creds: DeviceCredentials,
    pub user_creds: UserCredentials,
}

impl Default for ClientDeviceFixture {
    fn default() -> Self {
        let user_creds = UserCredentials::generate(DeviceName::from("client_device"), VaultName::from("test_vault"));
        let device_creds = user_creds.device_creds.clone();

        Self {
            device_creds,
            user_creds,
        }
    }
}
